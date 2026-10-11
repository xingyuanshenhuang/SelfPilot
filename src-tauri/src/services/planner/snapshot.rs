//! 目标树智能规划引擎（v2）——A1 快照装配器
//!
//! 一次性装配规划所需的全部只读输入（目标子树 + 活跃任务 + 依赖边 + 外部负载
//! + 时间视界日槽），替代逐目标循环查询。装配规则见整合设计方案 §4.1.1。
//!
//! - 数据加载全部走 `sqlx` 绑定参数（NFR-K6），目标子树用 `path` 前缀一次查询；
//! - `build_day_slots` 为纯函数：可用日计算扣除 blackout 与 allowed_weekdays
//!   之外的日，weight 默认 1.0、周末乘 weekend_factor（来自软约束）；
//! - 剩余工作量 W 的口径（修复 G-01/G-02 的核心）：
//!   `W = Σ max(0, plan_qty − actual_qty)`，求和范围为
//!   `status NOT IN ('done','skipped')` 的活跃任务（done 已在历史中，
//!   skipped 视为不存在，与 progress_service 口径一致）。

use std::collections::{BTreeSet, HashMap};

use chrono::{Datelike, NaiveDate};
use sqlx::SqlitePool;

use crate::db::models::{Goal, Task};
use crate::error::{AppError, AppResult};

/// 规划时间视界跨度（含起止两端）
#[derive(Debug, Clone)]
pub struct HorizonSpan {
    /// 起始日
    pub start: NaiveDate,
    /// 结束日
    pub end: NaiveDate,
}

/// 单日槽位（A1 约束编译的原子单位）
#[derive(Debug, Clone)]
pub struct DaySlot {
    /// 日期
    pub date: NaiveDate,
    /// 是否可用于排期（false = 落入 blackout 或 allowed_weekdays 之外）
    pub available: bool,
    /// 当日权重（默认 1.0；周末 × weekend_factor；不可用日为 0.0）
    pub weight: f64,
    /// 是否为休息日/不可用日（黑名单日或非允许星期）
    pub is_rest: bool,
}

/// 规划快照（输入聚合根，只读）
///
/// 一次规划所需的全部输入数据；求解器只依赖本结构，不回查数据库。
pub struct PlanSnapshot {
    /// 今天（由装配时注入，求解层不读时钟）
    pub today: NaiveDate,
    /// 规划根目标
    pub root_goal: Goal,
    /// 目标子树内全部目标（含根）
    pub goals: Vec<Goal>,
    /// 时间视界内的日槽列表（装配时的基础口径：无黑名单、全周可用、
    /// weekend_factor=1.0；编译后的最终口径见 constraints::CompiledSpec.days）
    pub days: Vec<DaySlot>,
    /// 活跃任务列表（子树内 status NOT IN ('done','skipped')）
    pub active_tasks: Vec<Task>,
    /// 依赖边：task_id → 其前置任务 id 列表（子树内任务间的依赖）
    pub deps: HashMap<String, Vec<String>>,
    /// 既有负载：日期 → 子树外目标当日计划量合计（用于扣除当日可用额度）
    pub external_load: HashMap<NaiveDate, f64>,
}

impl PlanSnapshot {
    /// 剩余工作量 W
    ///
    /// 口径：`W = Σ max(0, plan_qty − actual_qty)`，求和范围为
    /// `status NOT IN ('done','skipped')` 的活跃任务（`active_tasks`
    /// 已在装配时按该口径过滤）。
    pub fn remaining_qty(&self) -> f64 {
        self.active_tasks
            .iter()
            .map(|t| (t.plan_qty - t.actual_qty).max(0.0))
            .sum()
    }
}

/// 构造时间视界内的日槽序列（纯函数）
///
/// - `end < start` 时返回空序列；
/// - 落入 `blackout` 或星期不在 `allowed_weekdays` 的日为不可用日
///   （`available = false`、`is_rest = true`、`weight = 0.0`）；
/// - 可用日 weight 默认 1.0，周末（周日/周六）乘 `weekend_factor`
///   （来自软约束，默认 1.0 即不衰减）；
/// - `allowed_weekdays = None` 表示全周可用。
pub fn build_day_slots(
    start: NaiveDate,
    end: NaiveDate,
    blackout: &BTreeSet<NaiveDate>,
    allowed_weekdays: Option<&[u8]>,
    weekend_factor: f64,
) -> Vec<DaySlot> {
    if end < start {
        return Vec::new();
    }
    let allowed: Option<BTreeSet<u8>> = allowed_weekdays
        .map(|weekdays| weekdays.iter().copied().collect());
    let span = (end - start).num_days() + 1;
    let mut slots = Vec::with_capacity(span.max(0) as usize);
    let mut date = start;
    while date <= end {
        // chrono: 0=周日 … 6=周六，与约束口径一致
        let weekday = date.weekday().num_days_from_sunday() as u8;
        let in_blackout = blackout.contains(&date);
        let weekday_ok = allowed
            .as_ref()
            .map(|set| set.contains(&weekday))
            .unwrap_or(true);
        let is_rest = in_blackout || !weekday_ok;
        let is_weekend = weekday == 0 || weekday == 6;
        let weight = if is_rest {
            0.0
        } else if is_weekend {
            weekend_factor
        } else {
            1.0
        };
        slots.push(DaySlot {
            date,
            available: !is_rest,
            weight,
            is_rest,
        });
        date += chrono::Duration::days(1);
    }
    slots
}

/// 装配规划快照
///
/// 一次查询加载目标子树（`path` 前缀匹配，禁止逐目标循环）：
/// 1. 根目标（不存在时返回 `AppError::NotFound`，对应错误码 E-NOTFOUND）；
/// 2. 子树全部目标与活跃任务（status NOT IN ('done','skipped')）；
/// 3. 子树内任务间依赖边；
/// 4. 视界内子树外目标的既有日负载（status != 'skipped'，与 get_daily_load 口径一致）；
/// 5. 基础口径日槽（无黑名单、全周可用、weekend_factor=1.0）。
///
/// `horizon.start > horizon.end` 视为参数错误（E-PARAM）。
pub async fn load_snapshot(
    pool: &SqlitePool,
    goal_id: &str,
    horizon: &HorizonSpan,
) -> AppResult<PlanSnapshot> {
    if horizon.end < horizon.start {
        return Err(AppError::Param(
            "规划视界结束日期不能早于开始日期".into(),
        ));
    }
    let today = chrono::Local::now().date_naive();

    // 1. 根目标（E-NOTFOUND）
    let root_goal: Goal = sqlx::query_as("SELECT * FROM goals WHERE id = ?")
        .bind(goal_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("目标不存在".into()))?;

    // 子树匹配：path = 根路径 或 path LIKE '根路径/%'
    let path_pattern = format!("{}/%", root_goal.path);

    // 2a. 子树全部目标
    let goals: Vec<Goal> = sqlx::query_as(
        "SELECT * FROM goals WHERE path = ? OR path LIKE ? ORDER BY sort_order, created_at",
    )
    .bind(&root_goal.path)
    .bind(&path_pattern)
    .fetch_all(pool)
    .await?;

    // 2b. 子树内活跃任务（status NOT IN ('done','skipped')，W 口径基础）
    let active_tasks: Vec<Task> = sqlx::query_as(
        "WITH subtree AS (SELECT id FROM goals WHERE path = ? OR path LIKE ?)
         SELECT t.* FROM tasks t
         JOIN subtree s ON s.id = t.goal_id
         WHERE t.status NOT IN ('done','skipped')
         ORDER BY t.plan_date, t.sort_order",
    )
    .bind(&root_goal.path)
    .bind(&path_pattern)
    .fetch_all(pool)
    .await?;

    // 3. 依赖边：task_id → 前置任务集合（仅子树内任务间的依赖）
    let dep_rows: Vec<(String, String)> = sqlx::query_as(
        "WITH subtree AS (SELECT id FROM goals WHERE path = ? OR path LIKE ?)
         SELECT d.task_id, d.depends_on_id
         FROM task_dependencies d
         JOIN tasks t1 ON t1.id = d.task_id
         JOIN tasks t2 ON t2.id = d.depends_on_id
         JOIN subtree s1 ON s1.id = t1.goal_id
         JOIN subtree s2 ON s2.id = t2.goal_id",
    )
    .bind(&root_goal.path)
    .bind(&path_pattern)
    .fetch_all(pool)
    .await?;
    let mut deps: HashMap<String, Vec<String>> = HashMap::new();
    for (task_id, depends_on_id) in dep_rows {
        deps.entry(task_id).or_default().push(depends_on_id);
    }

    // 4. 子树外目标的既有日负载（复用 get_daily_load 的统计口径：排除 skipped）
    let start_str = horizon.start.format("%Y-%m-%d").to_string();
    let end_str = horizon.end.format("%Y-%m-%d").to_string();
    let load_rows: Vec<(Option<String>, f64)> = sqlx::query_as(
        "WITH subtree AS (SELECT id FROM goals WHERE path = ? OR path LIKE ?)
         SELECT t.plan_date,
                CAST(COALESCE(SUM(t.plan_qty), 0) AS REAL) AS total_qty
         FROM tasks t
         WHERE t.plan_date IS NOT NULL
           AND t.plan_date >= ? AND t.plan_date <= ?
           AND t.status != 'skipped'
           AND t.goal_id NOT IN (SELECT id FROM subtree)
         GROUP BY t.plan_date",
    )
    .bind(&root_goal.path)
    .bind(&path_pattern)
    .bind(&start_str)
    .bind(&end_str)
    .fetch_all(pool)
    .await?;
    let mut external_load: HashMap<NaiveDate, f64> = HashMap::new();
    for (date_str, total_qty) in load_rows {
        if let Some(date_str) = date_str {
            if let Ok(date) = NaiveDate::parse_from_str(&date_str, "%Y-%m-%d") {
                external_load.insert(date, total_qty);
            }
        }
    }

    // 5. 基础口径日槽（无黑名单、全周可用、weekend_factor=1.0）
    let days = build_day_slots(horizon.start, horizon.end, &BTreeSet::new(), None, 1.0);

    Ok(PlanSnapshot {
        today,
        root_goal,
        goals,
        days,
        active_tasks,
        deps,
        external_load,
    })
}
