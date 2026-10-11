//! 目标树智能规划引擎（v2）——A1 约束编译器
//!
//! 把「统一入参 + 目标属性 + 规划快照」按 §2.6.1 字段级适配规则表
//! 编译为最终可执行的约束集 [`CompiledSpec`]（回落链与冲突优先级严格照表），
//! 并生成扣除 blackout / 非允许星期 / 末尾缓冲后的可用日序列。
//!
//! 本模块为纯编译层：不访问数据库、不读时钟（today 由快照注入）。

use std::collections::BTreeSet;

use chrono::NaiveDate;

use crate::db::models::Goal;
use crate::error::{AppError, AppResult};
use crate::services::planner::snapshot::{build_day_slots, DaySlot, PlanSnapshot};
use crate::services::planner::types::{SmoothingPolicy, UnifiedPlanRequest};
use crate::util::parse_date_or_param;

/// 无截止日期时的滚动窗口上限（天），对应 EffectivePolicy.window_cap_days 默认值
pub const DEFAULT_WINDOW_CAP_DAYS: i64 = 30;

/// 跨目标单日任务预算默认值（global_daily_task_budget 回落值）
pub const DEFAULT_GLOBAL_DAILY_TASK_BUDGET: u32 = 6;

/// 编译后的约束集（compile_constraints 的输出，A1 产物）
///
/// 全部字段均为「定稿值」：冲突优先级已裁决、回落链已走完，
/// 求解器直接消费本结构，不再回查请求或目标。
#[derive(Debug, Clone)]
pub struct CompiledSpec {
    /// 规划起始日（恒不低于 today + 1，与现有拆解「从明天开始」一致）
    pub start: NaiveDate,
    /// 规划结束日（含末尾缓冲；保底 today + 1）
    pub end: NaiveDate,
    /// 末尾预留缓冲天数（0 ≤ v < 规划总天数）
    pub buffer_days: u32,
    /// 实际拆解总量（A 覆盖 > goal.total_qty）
    pub total_qty: f64,
    /// 实际单位（A 覆盖 > goal.unit）
    pub unit: String,
    /// 每日容量（B override > goal.daily_capacity → 画像；None = 未设置）
    pub daily_capacity: Option<f64>,
    /// 单日计划量上限（None = 不限）
    pub max_daily_qty: Option<f64>,
    /// 单日时长上限（小时；None = 不限）
    pub max_daily_hours: Option<f64>,
    /// 单日任务条数上限（None = 不限）
    pub max_daily_tasks: Option<f64>,
    /// 单任务最小粒度（None = 未设置）
    pub min_chunk_qty: Option<f64>,
    /// 跨目标单日任务预算
    pub global_daily_task_budget: u32,
    /// 编译后黑名单日集合（blackout_dates ∪ blackout_ranges 展开 ∪ day_availability 表）
    pub blackout: BTreeSet<NaiveDate>,
    /// 编译后允许的星期（升序去重；None 输入回落为全周 0..=6）
    pub allowed_weekdays: Vec<u8>,
    /// 周末容量系数（A 请求 > 画像 > 默认 1.0）
    pub weekend_factor: f64,
    /// 实际生效的习惯画像信任度（门控值 × 用户信任度；画像缺失时门控 = 1.0）
    pub adapt_strength: f64,
    /// 平滑与迟滞参数
    pub smoothing: SmoothingPolicy,
    /// 编译后可用日序列（缓冲区内的日不可排布）
    pub days: Vec<DaySlot>,
    /// 有效天数（available 且 weight > 0 的日数；0 权重日不承载任务）
    pub effective_days: u32,
    /// 有效容量合计（Σ weight > 0 的可用日 weight）
    pub total_capacity: f64,
}

/// 编译统一入参为最终约束集
///
/// 按 §2.6.1 字段级适配规则表执行合并，关键回落链与优先级：
/// - `horizon.end`：A 请求 > goal.deadline > today + 30（滚动窗口）；
/// - `horizon.start`：`max(请求值, today + 1)`；
/// - `total_qty`：A 覆盖 > goal.total_qty（≤ 0 时返回 E-ZERO-QTY 等价文案）；
/// - `daily_capacity`：B override > goal.daily_capacity → 画像（本批次画像回落链尚未接入）；
/// - `blackout`：请求侧并集（day_availability 表 024 迁移后并入）；
/// - `allowed_weekdays`：交集（请求 ∩ 策略休息日补集；缺失回落全周）；
/// - `weekend_factor`：A 请求 > 画像 > 默认 1.0；
/// - `adapt_strength`：门控值 × 用户信任度（相乘；画像缺失时门控 = 1.0）。
pub fn compile_constraints(
    unified: &UnifiedPlanRequest,
    goal: &Goal,
    snapshot: &PlanSnapshot,
) -> AppResult<CompiledSpec> {
    let tomorrow = snapshot.today + chrono::Duration::days(1);

    // ---------- 时间视界回落链 ----------
    let req_start: Option<NaiveDate> = match &unified.horizon {
        Some(horizon) => match &horizon.start_date {
            Some(s) => Some(parse_date_or_param(s, "时间范围起始")?),
            None => None,
        },
        None => None,
    };
    let req_end: Option<NaiveDate> = match &unified.horizon {
        Some(horizon) => match &horizon.end_date {
            Some(s) => Some(parse_date_or_param(s, "时间范围结束")?),
            None => None,
        },
        None => None,
    };
    // end：A 请求 > goal.deadline > today + window_cap_days
    let goal_deadline: Option<NaiveDate> = match &goal.deadline {
        Some(s) => Some(parse_date_or_param(s, "截止")?),
        None => None,
    };
    let end = req_end
        .or(goal_deadline)
        .unwrap_or(snapshot.today + chrono::Duration::days(DEFAULT_WINDOW_CAP_DAYS))
        .max(tomorrow); // 保底 1 天（源 B A1 第 5 行）
    // start：恒不低于 today + 1
    let start = req_start.map_or(tomorrow, |s| s.max(tomorrow));
    if end < start {
        return Err(AppError::Param(
            "规划视界开始日期不能晚于结束日期".into(),
        ));
    }

    // ---------- 缓冲天数（0 ≤ v < 规划总天数） ----------
    let buffer_days = unified.buffer_days.unwrap_or(0);
    let span = (end - start).num_days() + 1;
    if buffer_days as i64 >= span {
        return Err(AppError::Param(format!(
            "buffer_days({})必须小于规划总天数({})",
            buffer_days, span
        )));
    }

    // ---------- 总量与单位（A 覆盖 > goal） ----------
    let total_qty = match unified.total_qty_override {
        Some(qty) if qty.is_finite() && qty > 0.0 => qty,
        Some(_) => return Err(AppError::Business("目标总量必须大于 0".into())),
        None => goal.total_qty,
    };
    if !(total_qty.is_finite() && total_qty > 0.0) {
        return Err(AppError::Business("目标总量必须大于 0".into()));
    }
    let unit = unified
        .unit_override
        .as_ref()
        .filter(|u| !u.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| goal.unit.clone());

    // ---------- 每日容量（B override > goal.daily_capacity → 画像） ----------
    let daily_capacity = unified
        .constraints
        .daily_capacity
        .filter(|c| c.is_finite() && *c > 0.0)
        .or(goal
            .daily_capacity
            .filter(|c| c.is_finite() && *c > 0.0));

    // ---------- 上限类硬约束（取更严 min 的合并值由适配层定稿，此处透传） ----------
    let max_daily_qty = unified
        .constraints
        .max_daily_qty
        .filter(|v| v.is_finite() && *v > 0.0);
    let max_daily_hours = unified
        .constraints
        .max_daily_hours
        .filter(|v| v.is_finite() && *v > 0.0);
    let max_daily_tasks = unified
        .constraints
        .max_daily_tasks
        .filter(|v| v.is_finite() && *v > 0.0);
    let min_chunk_qty = unified
        .constraints
        .min_chunk_qty
        .filter(|v| v.is_finite() && *v > 0.0);
    let global_daily_task_budget = unified
        .constraints
        .global_daily_task_budget
        .unwrap_or(DEFAULT_GLOBAL_DAILY_TASK_BUDGET);

    // ---------- 黑名单日并集（请求 ∪ day_availability 表[024 后并入]） ----------
    let mut blackout: BTreeSet<NaiveDate> = BTreeSet::new();
    for date_str in &unified.constraints.blackout_dates {
        blackout.insert(parse_date_or_param(date_str, "不可用日")?);
    }
    for range in &unified.constraints.blackout_ranges {
        let from = parse_date_or_param(&range.from, "不可用日区间起始")?;
        let to = parse_date_or_param(&range.to, "不可用日区间结束")?;
        if from > to {
            return Err(AppError::Param(
                "blackout_ranges 的 from 不能晚于 to".into(),
            ));
        }
        let mut date = from;
        while date <= to {
            blackout.insert(date);
            date += chrono::Duration::days(1);
        }
    }

    // ---------- 允许星期交集（请求 ∩ 策略休息日补集；缺失回落全周） ----------
    let allowed_weekdays: Vec<u8> = unified
        .constraints
        .allowed_weekdays
        .as_ref()
        .map(|weekdays| {
            let mut days: Vec<u8> = weekdays.clone();
            days.sort_unstable();
            days.dedup();
            days
        })
        .unwrap_or_else(|| (0..=6).collect());

    // ---------- 周末系数（A 请求 > 画像 > 默认 1.0） ----------
    let weekend_factor = unified
        .constraints
        .weekend_factor
        .filter(|f| f.is_finite() && *f >= 0.0)
        .unwrap_or(1.0);

    // ---------- 习惯信任度（门控 × 用户信任度；画像缺失时门控 = 1.0） ----------
    let adapt_strength = unified
        .learning
        .adapt_strength
        .filter(|s| s.is_finite() && (0.0..=1.0).contains(s))
        .unwrap_or(1.0);

    // ---------- 可用日序列（扣黑名单/非允许星期/末尾缓冲） ----------
    let mut days = build_day_slots(start, end, &blackout, Some(&allowed_weekdays), weekend_factor);
    if buffer_days > 0 {
        let buffer_start = end - chrono::Duration::days(buffer_days as i64 - 1);
        for slot in days.iter_mut() {
            // 末尾缓冲区不参与任务排布（available=false、weight=0，非休息日性质）
            if slot.date >= buffer_start && slot.available {
                slot.available = false;
                slot.weight = 0.0;
            }
        }
    }
    // 有效天数：available 且 weight > 0（0 权重可用日——如 weekend_factor=0 的
    // 周末——配额为 0 无法承载任务，计入会稀释 A2 的依赖链拦截与日均口径）
    let effective_days = days
        .iter()
        .filter(|d| d.available && d.weight > 0.0)
        .count() as u32;
    let total_capacity: f64 = days
        .iter()
        .filter(|d| d.available && d.weight > 0.0)
        .map(|d| d.weight)
        .sum();

    Ok(CompiledSpec {
        start,
        end,
        buffer_days,
        total_qty,
        unit,
        daily_capacity,
        max_daily_qty,
        max_daily_hours,
        max_daily_tasks,
        min_chunk_qty,
        global_daily_task_budget,
        blackout,
        allowed_weekdays,
        weekend_factor,
        adapt_strength,
        smoothing: SmoothingPolicy::default(),
        days,
        effective_days,
        total_capacity,
    })
}
