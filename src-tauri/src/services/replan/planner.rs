//! 重新规划 · 求解器（A1 简化版 + A2 可行性 + A3 简化分配）
//!
//! 依据《目标树重新规划功能重构设计文档》§3.2~§3.4：
//! - 真实剩余量回填（FR-1）：W = Σ max(0, plan_qty − actual_qty)，修复 G-01/G-02；
//! - 真实容量（FR-2，A1 简化版）：逐日 base × weekend_factor × intensity，
//!   reserved = 外部负载 + 锁定任务当日 cost，available = max(0, cap − reserved)；
//! - 可行性（A2）：feasible ⇔ W ≤ C 且关键路径长度 ≤ 可用日数；
//! - 分配（A3 简化版）：拓扑序逐日填充，最早可容纳日优先，
//!   容量耗尽时顺延视界末尾（overflow_tail warning，不报错）。
//!
//! `solve` 为只读装配 + 纯计算：永不落库（`dry_run` 语义天然成立）；
//! `solve_inner` 为纯函数（不访问数据库、不读时钟），单测入口。

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use chrono::{Datelike, Duration, NaiveDate};
use serde_json::json;
use sqlx::{QueryBuilder, Sqlite, SqlitePool};
use validator::Validate;

use crate::db::models::{
    DayLoadAfter, FeasibilityReport, Goal, PlanWarning, ReplanMetrics, ReplanOption, ReplanPlan,
    ReplanSolveInput, TaskPlacement,
};
use crate::error::{AppError, AppResult};
use crate::services::planner::feasibility::longest_chain_length;
use crate::services::planner::snapshot::{load_snapshot, HorizonSpan, PlanSnapshot};
use crate::services::replan::explain;
use crate::services::replan::learning::{apply_profile, load_planning_profile, PlanningProfile};
use crate::services::split_planner::sha256_hex;
use crate::util::parse_date_or_param;

/// 守恒校验容差（INV-1：误差 < 1e-6）
pub const CONSERVATION_EPS: f64 = 1e-6;
/// 强度系数硬上限（raise_intensity 的 risky 判定阈值）
pub const MAX_INTENSITY_HARD: f64 = 1.5;
/// 无截止日期时的滚动窗口天数（与 constraints::DEFAULT_WINDOW_CAP_DAYS 对齐）
const DEFAULT_WINDOW_DAYS: i64 = 30;
/// 容量视为耗尽的浮点噪声阈值
const CAP_EPS: f64 = 1e-9;

/// 协同候选默认聚类窗口（deadline 90 天内，§4.12.3 选项 B）
const COORDINATE_DEFAULT_DAYS: i64 = 90;
/// A5 迭代再平衡上限（§4.12 阶段四）
const COORDINATE_MAX_ROUNDS: usize = 5;

/// 协同目标上下文（A5 阶段一产物）
struct CoordinateCtx {
    goal_id: String,
    goal: Goal,
    policy: GoalPolicy,
    snapshot: PlanSnapshot,
    /// 该目标视界结束日
    end: NaiveDate,
    /// 剩余工作量 W = Σ max(0, plan_qty − actual_qty)
    workload: f64,
    /// 目标优先级 Pri（goals.priority）
    priority: f64,
    /// 每日配额（阶段二分摊产物，(date, quota)）
    quota: HashMap<NaiveDate, f64>,
    /// 阶段三求解产物
    plan: Option<ReplanPlan>,
}

/// 单目标单日容量口径（与 solve_inner 的 A1 简化版完全一致，
/// 供 A5 阶段二全局额度分摊与内部求解共用，保证口径统一）
fn own_day_cap(
    date: NaiveDate,
    profile: &PlanningProfile,
    policy: &GoalPolicy,
    base_capacity: Option<f64>,
    intensity: f64,
) -> f64 {
    let weekday = date.weekday().num_days_from_sunday() as u8;
    if policy.rest_days.contains(&weekday) {
        return 0.0;
    }
    let base = base_capacity
        .or_else(|| profile.weekday_hours.get(&weekday).copied())
        .unwrap_or(1.0);
    // B 组偏好基线（weekday_hours × weekend_factor × 强度）
    let raw = base * profile.weekend_factor * intensity;
    // A 组学习修正（阶段 4 §4.6.5）：corr × weekday_capacity[wd]；
    // 冷启动（samples=0）时静默返回 base，行为与阶段 3 一致
    let raw = apply_profile(raw, date, profile, 1.0);
    let mut cap = raw;
    if let Some(m) = policy.max_daily_hours {
        cap = cap.min(m);
    }
    if let Some(m) = policy.max_daily_tasks {
        cap = cap.min(m);
    }
    cap
}

/// 单日预算（A1 简化版产物）
#[derive(Debug, Clone)]
struct DayBudget {
    /// 日期
    date: NaiveDate,
    /// 单日容量上限（min(raw, max_daily_hours?, max_daily_tasks?)）
    #[allow(dead_code)] // Phase 4 消费：过载告警/负载展示需要原始容量口径
    cap: f64,
    /// 扣外部负载与锁定任务占位后的可分配额度
    available: f64,
}

/// 目标级规划策略（goal_planning_policy 行，未设置时全缺省）
#[derive(Debug, Clone, Default)]
pub struct GoalPolicy {
    /// 强度系数（缺省 1.0）
    intensity: f64,
    /// 单日时长上限（小时）
    max_daily_hours: Option<f64>,
    /// 单日任务条数上限
    max_daily_tasks: Option<f64>,
    /// 休息日星期集合（0=周日）
    rest_days: HashSet<u8>,
}

/// 单位模式：hours | quantity
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnitsMode {
    Hours,
    Quantity,
}

// ============================================================
// 只读装配层（solve 专用，非纯函数）
// ============================================================

/// 解析规划结束日：overrides.deadline > goal.deadline > today+30，保底 today+1
fn resolve_end(req: &ReplanSolveInput, root_goal: &Goal, today: NaiveDate) -> AppResult<NaiveDate> {
    let tomorrow = today + Duration::days(1);
    let end = match req.overrides.as_ref().and_then(|o| o.deadline.as_ref()) {
        Some(s) => parse_date_or_param(s, "截止")?,
        None => match &root_goal.deadline {
            Some(s) => parse_date_or_param(s, "截止")?,
            None => today + Duration::days(DEFAULT_WINDOW_DAYS),
        },
    };
    Ok(end.max(tomorrow))
}

/// 读取目标级策略（goal_planning_policy；未设置行时全缺省）
async fn load_goal_policy(pool: &SqlitePool, goal_id: &str) -> AppResult<GoalPolicy> {
    let row: Option<(Option<f64>, Option<f64>, Option<f64>, Option<String>)> = sqlx::query_as(
        "SELECT intensity, max_daily_hours, max_daily_tasks, rest_days
         FROM goal_planning_policy WHERE goal_id = ?",
    )
    .bind(goal_id)
    .fetch_optional(pool)
    .await?;
    let mut policy = GoalPolicy {
        intensity: 1.0,
        ..Default::default()
    };
    let Some((intensity, max_hours, max_tasks, rest_days_json)) = row else {
        return Ok(policy);
    };
    if let Some(i) = intensity {
        if i.is_finite() && i > 0.0 {
            policy.intensity = i;
        }
    }
    policy.max_daily_hours = max_hours.filter(|m| m.is_finite() && *m > 0.0);
    policy.max_daily_tasks = max_tasks.filter(|m| m.is_finite() && *m > 0.0);
    if let Some(raw) = rest_days_json {
        if let Ok(days) = serde_json::from_str::<Vec<u8>>(&raw) {
            policy.rest_days = days.into_iter().filter(|&w| w <= 6).collect();
        }
    }
    Ok(policy)
}

// ============================================================
// 纯求解（solve_inner，单测入口）
// ============================================================

/// 任务单位成本（容量消耗）：hours 模式占 estimated_hours（缺省 1.0），
/// quantity 模式占计划量额度
fn cost_of(task: &crate::db::models::Task, units: UnitsMode, allocable: f64) -> f64 {
    match units {
        UnitsMode::Hours => task.estimated_hours.filter(|h| h.is_finite()).unwrap_or(1.0),
        UnitsMode::Quantity => allocable.max(0.0),
    }
}

/// 拓扑排序（仅集合内依赖；检测到环时返回降级顺序 + 环标记）
///
/// 同层按 (sort_order, 输入序) 稳定排序；环时退化为 sort_order 序
/// （cycle_detected 由调用方转 warning，与 dag_scheduler 的 Err 语义
/// 相比更适合"不可行不抛错"的降级策略）。
fn topo_order(
    tasks: &[crate::db::models::Task],
    deps: &HashMap<String, Vec<String>>,
    include: &HashSet<&str>,
) -> (Vec<String>, bool) {
    let in_set: HashSet<&str> = include.iter().copied().collect();
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for t in tasks {
        if !in_set.contains(t.id.as_str()) {
            continue;
        }
        in_degree.entry(t.id.as_str()).or_insert(0);
        if let Some(preds) = deps.get(&t.id) {
            for p in preds {
                if in_set.contains(p.as_str()) {
                    adj.entry(p.as_str()).or_default().push(t.id.as_str());
                    *in_degree.entry(t.id.as_str()).or_insert(0) += 1;
                }
            }
        }
    }
    let input_index: HashMap<&str, usize> = tasks
        .iter()
        .enumerate()
        .filter(|(_, t)| in_set.contains(t.id.as_str()))
        .map(|(i, t)| (t.id.as_str(), i))
        .collect();
    let sort_key = |t: &crate::db::models::Task| (t.sort_order, input_index[t.id.as_str()]);
    let mut ready: Vec<&crate::db::models::Task> = tasks
        .iter()
        .filter(|t| in_set.contains(t.id.as_str()) && in_degree[t.id.as_str()] == 0)
        .collect();
    ready.sort_by_key(|t| sort_key(t));
    let mut queue: VecDeque<&crate::db::models::Task> = ready.into_iter().collect();
    let mut order: Vec<String> = Vec::with_capacity(in_set.len());
    while let Some(t) = queue.pop_front() {
        order.push(t.id.clone());
        if let Some(nexts) = adj.get(t.id.as_str()) {
            let mut newly_ready: Vec<&crate::db::models::Task> = Vec::new();
            for nid in nexts {
                if let Some(deg) = in_degree.get_mut(*nid) {
                    *deg -= 1;
                    if *deg == 0 {
                        if let Some(nt) = tasks.iter().find(|x| x.id.as_str() == *nid) {
                            newly_ready.push(nt);
                        }
                    }
                }
            }
            newly_ready.sort_by_key(|nt| sort_key(nt));
            queue.extend(newly_ready);
        }
    }
    let has_cycle = order.len() < in_set.len();
    if has_cycle {
        // 降级：按 (sort_order, 输入序) 排序
        let mut all: Vec<&crate::db::models::Task> = tasks
            .iter()
            .filter(|t| in_set.contains(t.id.as_str()))
            .collect();
        all.sort_by_key(|t| sort_key(t));
        order = all.into_iter().map(|t| t.id.clone()).collect();
    }
    (order, has_cycle)
}

/// 纯求解：快照 + 画像 + 策略 → 完整计划（永不落库）
///
/// 数量守恒 INV-1：参与重排任务的 Σ new_plan_qty 必须等于
/// Σ max(0, plan_qty − actual_qty)（1e-6 容差），违反返回 Err。
pub fn solve_inner(
    snapshot: &PlanSnapshot,
    req: &ReplanSolveInput,
    profile: &PlanningProfile,
    policy: &GoalPolicy,
    // 跨目标协同注入的每日配额（A5 阶段二产物）：对该目标单日可分配
    // 额度施加上限；None（单目标）时不限制。
    daily_quota: Option<&HashMap<NaiveDate, f64>>,
) -> AppResult<ReplanPlan> {
    let today = snapshot.today;
    let tomorrow = today + Duration::days(1);
    let horizon_start = snapshot
        .days
        .first()
        .map(|d| d.date)
        .unwrap_or(tomorrow);
    let horizon_end = snapshot
        .days
        .last()
        .map(|d| d.date)
        .unwrap_or(tomorrow);

    // ---------- 1. 范围过滤（scope=single 只留直属任务） ----------
    let root_id = snapshot.root_goal.id.as_str();
    let tasks: Vec<crate::db::models::Task> = if req.scope == "single" {
        snapshot
            .active_tasks
            .iter()
            .filter(|t| t.goal_id == root_id)
            .cloned()
            .collect()
    } else {
        snapshot.active_tasks.clone()
    };
    let deps: HashMap<String, Vec<String>> = if req.scope == "single" {
        let ids: HashSet<&str> = tasks.iter().map(|t| t.id.as_str()).collect();
        snapshot
            .deps
            .iter()
            .filter_map(|(k, v)| {
                if !ids.contains(k.as_str()) {
                    return None;
                }
                let filtered: Vec<String> = v
                    .iter()
                    .filter(|d| ids.contains(d.as_str()))
                    .cloned()
                    .collect();
                Some((k.clone(), filtered))
            })
            .collect()
    } else {
        snapshot.deps.clone()
    };

    // ---------- 2. 任务分类（锁定/手动/排除不参与重排） ----------
    let respect_locks = req
        .overrides
        .as_ref()
        .and_then(|o| o.respect_locks)
        .unwrap_or(true);
    let excluded: HashSet<&str> = req
        .overrides
        .as_ref()
        .and_then(|o| o.excluded_task_ids.as_ref())
        .map(|v| v.iter().map(|s| s.as_str()).collect())
        .unwrap_or_default();
    let locked_ids: HashSet<&str> = tasks
        .iter()
        .filter(|t| t.schedule_locked.is_some() && respect_locks)
        .map(|t| t.id.as_str())
        .collect();
    let manual_ids: HashSet<&str> = tasks
        .iter()
        .filter(|t| t.is_manual == 1)
        .map(|t| t.id.as_str())
        .collect();
    let realloc_ids: HashSet<&str> = tasks
        .iter()
        .filter(|t| {
            !locked_ids.contains(t.id.as_str())
                && !manual_ids.contains(t.id.as_str())
                && !excluded.contains(t.id.as_str())
        })
        .map(|t| t.id.as_str())
        .collect();

    // ---------- 3. 真实剩余量（FR-1） ----------
    let allocable_of = |t: &crate::db::models::Task| (t.plan_qty - t.actual_qty).max(0.0);
    let workload: f64 = tasks.iter().map(|t| allocable_of(t)).sum();
    let w_realloc: f64 = tasks
        .iter()
        .filter(|t| realloc_ids.contains(t.id.as_str()))
        .map(|t| allocable_of(t))
        .sum();

    // ---------- 4. 真实容量（FR-2，A1 简化版） ----------
    let units = if snapshot.root_goal.daily_capacity.is_some()
        || tasks.iter().any(|t| t.estimated_hours.is_some())
    {
        UnitsMode::Hours
    } else {
        UnitsMode::Quantity
    };
    let base_capacity = req
        .overrides
        .as_ref()
        .and_then(|o| o.daily_capacity)
        .or(snapshot.root_goal.daily_capacity);
    let intensity = req
        .overrides
        .as_ref()
        .and_then(|o| o.intensity)
        .unwrap_or(policy.intensity);
    let mut budgets: Vec<DayBudget> = Vec::new();
    let mut date = horizon_start;
    while date <= horizon_end {
        let weekday = date.weekday().num_days_from_sunday() as u8;
        if !policy.rest_days.contains(&weekday) {
            // 单日容量（own_day_cap 与 A5 阶段二分摊共用同一口径）
            let cap = own_day_cap(date, profile, policy, base_capacity, intensity);
            let date_str = date.format("%Y-%m-%d").to_string();
            let ext = snapshot.external_load.get(&date).copied().unwrap_or(0.0);
            let locked_cost: f64 = tasks
                .iter()
                .filter(|t| locked_ids.contains(t.id.as_str()))
                .filter(|t| t.plan_date.as_deref() == Some(date_str.as_str()))
                .map(|t| cost_of(t, units, allocable_of(t)))
                .sum();
            // 协同配额上限：min(自身可用额, 分摊额度)；单目标（None）不限
            let quota_cap = daily_quota
                .and_then(|q| q.get(&date).copied())
                .unwrap_or(f64::INFINITY);
            budgets.push(DayBudget {
                date,
                cap,
                available: (cap - ext - locked_cost).max(0.0).min(quota_cap),
            });
        }
        date += Duration::days(1);
    }
    let capacity: f64 = budgets.iter().map(|b| b.available).sum();

    // ---------- 5. 可行性判定（A2） ----------
    let avail_days = budgets.iter().filter(|b| b.available > CAP_EPS).count();
    let chain = longest_chain_length(&tasks, &deps);
    let capacity_ok = workload <= capacity + CONSERVATION_EPS;
    let chain_ok = chain.map(|l| l <= avail_days).unwrap_or(false);
    let feasible = capacity_ok && chain_ok;
    let reason = if !capacity_ok {
        Some("capacity_shortfall")
    } else if !chain_ok {
        Some("dependency_chain_too_long")
    } else {
        None
    };
    let slack = if capacity > 0.0 {
        (capacity - workload) / capacity
    } else {
        0.0
    };
    let level = if slack < 0.05 {
        "tight"
    } else if slack < 0.25 {
        "normal"
    } else {
        "relaxed"
    };

    // 不可行三候选量化
    let mut options: Vec<ReplanOption> = Vec::new();
    if !feasible {
        let deficit = (workload - capacity).max(0.0);
        let avg_avail = if avail_days > 0 {
            capacity / avail_days as f64
        } else {
            0.0
        };
        let extend_days = if avg_avail > 0.0 {
            (deficit / avg_avail).ceil() as i64
        } else {
            1
        }
        .max(1);
        let new_end = horizon_end + Duration::days(extend_days);
        options.push(ReplanOption {
            kind: "extend_deadline".into(),
            label: format!(
                "延长截止日期 {} 天（至 {}）",
                extend_days,
                new_end.format("%Y-%m-%d")
            ),
            payload: json!({
                "days": extend_days,
                "new_deadline": new_end.format("%Y-%m-%d").to_string(),
            }),
            risky: false,
        });
        let min_intensity = if capacity > CAP_EPS {
            workload / capacity
        } else {
            f64::INFINITY
        };
        let capped = min_intensity > MAX_INTENSITY_HARD;
        options.push(ReplanOption {
            kind: "raise_intensity".into(),
            label: format!(
                "提高强度至 {:.1} 倍{}",
                min_intensity,
                if capped { "（超出上限，需确认）" } else { "" }
            ),
            payload: json!({
                "min_intensity": if min_intensity.is_finite() { json!(min_intensity) } else { json!(null) },
                "capped": capped,
            }),
            risky: capped,
        });
        options.push(ReplanOption {
            kind: "trim_scope".into(),
            label: format!("缩减范围：总量减至 {:.1} 以内", capacity),
            payload: json!({
                "max_workload": capacity,
                "suggested_cut": deficit,
            }),
            risky: false,
        });
    }

    // ---------- 6. 分配（A3 简化版） ----------
    let task_by_id: HashMap<&str, &crate::db::models::Task> =
        tasks.iter().map(|t| (t.id.as_str(), t)).collect();
    // 非参与任务（锁定/手动/排除）的完成日预置为原计划日期（依赖下界依据）
    let mut finish: HashMap<String, NaiveDate> = HashMap::new();
    for t in tasks.iter().filter(|t| !realloc_ids.contains(t.id.as_str())) {
        if let Some(ds) = &t.plan_date {
            if let Ok(d) = NaiveDate::parse_from_str(ds, "%Y-%m-%d") {
                finish.insert(t.id.clone(), d);
            }
        }
    }
    let (order, has_cycle) = topo_order(&tasks, &deps, &realloc_ids);
    let mut used: HashMap<NaiveDate, f64> = budgets.iter().map(|b| (b.date, 0.0)).collect();
    let mut overflow_ids: Vec<String> = Vec::new();
    let mut placements: Vec<TaskPlacement> = Vec::new();

    for tid in &order {
        let task = task_by_id
            .get(tid.as_str())
            .copied()
            .ok_or_else(|| AppError::Internal(format!("重排任务 {} 不在快照中", tid)))?;
        let allocable = allocable_of(task);
        let cost = cost_of(task, units, allocable);

        // est：依赖下界（FR-4：后继日期 > 前置日期）+ 逾期前移（FR-3）
        let mut est = tomorrow;
        let mut dep_lb: Option<NaiveDate> = None;
        if let Some(preds) = deps.get(tid) {
            for p in preds {
                if let Some(&fd) = finish.get(p) {
                    let next = fd + Duration::days(1);
                    if next > est {
                        est = next;
                        dep_lb = Some(next);
                    }
                }
            }
        }

        // 首个可容纳日（剩余容量 ≥ cost）
        let mut chosen: Option<NaiveDate> = None;
        for b in &budgets {
            if b.date >= est && (b.available - used[&b.date]) >= cost - CAP_EPS {
                chosen = Some(b.date);
                break;
            }
        }
        let (date, reason_kind) = match chosen {
            Some(d) => {
                *used.get_mut(&d).unwrap_or(&mut 0.0) += cost;
                (d, "load_balance")
            }
            None => {
                overflow_ids.push(tid.clone());
                (horizon_end, "capacity_trim")
            }
        };
        finish.insert(tid.clone(), date);

        let new_date = Some(date.format("%Y-%m-%d").to_string());
        let date_changed = task.plan_date.as_deref() != new_date.as_deref();
        let qty_changed = (task.plan_qty - allocable).abs() > 1e-9;
        let change_type = if date_changed && qty_changed {
            "both"
        } else if date_changed {
            "date_moved"
        } else if qty_changed {
            "qty_changed"
        } else {
            "retained"
        };
        // 逾期前移（FR-3）：原日期早于今天的任务标记 overdue_pull
        let mut reason = reason_kind;
        if reason == "load_balance" {
            if let Some(od) = &task.plan_date {
                if let Ok(od) = NaiveDate::parse_from_str(od, "%Y-%m-%d") {
                    if od < today {
                        reason = "overdue_pull";
                    } else if dep_lb.is_some() {
                        reason = "dep_lower_bound";
                    }
                }
            } else if dep_lb.is_some() {
                reason = "dep_lower_bound";
            }
        }
        placements.push(TaskPlacement {
            task_id: task.id.clone(),
            name: task.name.clone(),
            old_plan_date: task.plan_date.clone(),
            new_plan_date: new_date.clone(),
            old_plan_qty: task.plan_qty,
            new_plan_qty: allocable,
            unit: task.unit.clone(),
            change_type: change_type.to_string(),
            reason: reason.to_string(),
            reason_text: explain::reason_text(
                reason,
                &task.name,
                task.plan_date.as_deref(),
                new_date.as_deref(),
            ),
            locked: false,
            dep_lower_bound: dep_lb.map(|d| d.format("%Y-%m-%d").to_string()),
        });
    }

    // 锁定任务：原位保留（INV-4）
    for t in tasks.iter().filter(|t| locked_ids.contains(t.id.as_str())) {
        placements.push(TaskPlacement {
            task_id: t.id.clone(),
            name: t.name.clone(),
            old_plan_date: t.plan_date.clone(),
            new_plan_date: t.plan_date.clone(),
            old_plan_qty: t.plan_qty,
            new_plan_qty: t.plan_qty,
            unit: t.unit.clone(),
            change_type: "locked".to_string(),
            reason: "locked".to_string(),
            reason_text: explain::reason_text("locked", &t.name, None, None),
            locked: true,
            dep_lower_bound: None,
        });
    }
    // 手动 / 排除任务：保留原值
    for t in tasks.iter().filter(|t| {
        !locked_ids.contains(t.id.as_str())
            && (manual_ids.contains(t.id.as_str()) || excluded.contains(t.id.as_str()))
    }) {
        placements.push(TaskPlacement {
            task_id: t.id.clone(),
            name: t.name.clone(),
            old_plan_date: t.plan_date.clone(),
            new_plan_date: t.plan_date.clone(),
            old_plan_qty: t.plan_qty,
            new_plan_qty: t.plan_qty,
            unit: t.unit.clone(),
            change_type: "retained".to_string(),
            reason: "manual".to_string(),
            reason_text: explain::reason_text("manual", &t.name, None, None),
            locked: false,
            dep_lower_bound: None,
        });
    }

    // ---------- 数量守恒 INV-1（显式校验） ----------
    let sum_new: f64 = placements
        .iter()
        .filter(|p| realloc_ids.contains(p.task_id.as_str()))
        .map(|p| p.new_plan_qty)
        .sum();
    if (sum_new - w_realloc).abs() >= CONSERVATION_EPS {
        return Err(AppError::Internal(format!(
            "数量守恒校验失败：Σnew_plan_qty({}) != W({})",
            sum_new, w_realloc
        )));
    }

    // ---------- 7. metrics ----------
    let mut moved = 0usize;
    let mut qty_changed_total = 0usize;
    let mut retained_total = 0usize;
    let mut locked_total = 0usize;
    let mut overdue_recovered = 0usize;
    let mut predicted_finish: Option<NaiveDate> = None;
    for p in &placements {
        match p.change_type.as_str() {
            "date_moved" | "both" => moved += 1,
            "qty_changed" => qty_changed_total += 1,
            "retained" => retained_total += 1,
            "locked" => locked_total += 1,
            _ => {}
        }
        if p.change_type == "both" {
            qty_changed_total += 1;
        }
        if let Some(nd) = &p.new_plan_date {
            if let Ok(d) = NaiveDate::parse_from_str(nd, "%Y-%m-%d") {
                predicted_finish = Some(predicted_finish.map_or(d, |m| m.max(d)));
            }
        }
        if let (Some(od), Some(nd)) = (&p.old_plan_date, &p.new_plan_date) {
            if let (Ok(od), Ok(nd)) = (
                NaiveDate::parse_from_str(od, "%Y-%m-%d"),
                NaiveDate::parse_from_str(nd, "%Y-%m-%d"),
            ) {
                if od < today && nd >= today {
                    overdue_recovered += 1;
                }
            }
        }
    }

    // ---------- 8. day_load（前后对比，含外部负载） ----------
    let mut before: BTreeMap<NaiveDate, (i64, f64)> = BTreeMap::new();
    let mut after: BTreeMap<NaiveDate, (i64, f64)> = BTreeMap::new();
    for t in &tasks {
        if let Some(ds) = &t.plan_date {
            if let Ok(d) = NaiveDate::parse_from_str(ds, "%Y-%m-%d") {
                if d >= horizon_start && d <= horizon_end {
                    let e = before.entry(d).or_insert((0, 0.0));
                    e.0 += 1;
                    e.1 += t.plan_qty;
                }
            }
        }
    }
    for p in &placements {
        if let Some(nd) = &p.new_plan_date {
            if let Ok(d) = NaiveDate::parse_from_str(nd, "%Y-%m-%d") {
                let e = after.entry(d).or_insert((0, 0.0));
                e.0 += 1;
                e.1 += p.new_plan_qty;
            }
        }
    }
    let mut day_dates: BTreeSet<NaiveDate> = before.keys().copied().collect();
    day_dates.extend(after.keys().copied());
    day_dates.extend(
        snapshot
            .external_load
            .keys()
            .filter(|d| **d >= horizon_start && **d <= horizon_end)
            .copied(),
    );
    let mut day_load: Vec<DayLoadAfter> = Vec::new();
    for d in day_dates {
        let (bt, bq) = before.get(&d).copied().unwrap_or((0, 0.0));
        let (at, aq) = after.get(&d).copied().unwrap_or((0, 0.0));
        let ext = snapshot.external_load.get(&d).copied().unwrap_or(0.0);
        let bq = bq + ext;
        let aq = aq + ext;
        if bt > 0 || at > 0 || bq > 0.0 || aq > 0.0 {
            day_load.push(DayLoadAfter {
                date: d.format("%Y-%m-%d").to_string(),
                before_tasks: bt,
                before_qty: bq,
                after_tasks: at,
                after_qty: aq,
            });
        }
    }

    // ---------- 9. warnings ----------
    let mut warnings: Vec<PlanWarning> = Vec::new();
    if !feasible {
        let (kind, message) = match reason {
            Some("capacity_shortfall") => (
                "capacity_tight",
                format!(
                    "剩余工作量（{:.1}）超出可用容量（{:.1}），建议延长截止日期或提高强度",
                    workload, capacity
                ),
            ),
            _ => (
                "dependency_chain_too_long",
                "任务依赖链长度超过可用天数，建议延长截止日期或缩减范围".to_string(),
            ),
        };
        warnings.push(PlanWarning {
            kind: kind.to_string(),
            message,
            task_ids: vec![],
        });
    }
    if !overflow_ids.is_empty() {
        warnings.push(PlanWarning {
            kind: "overflow_tail".to_string(),
            message: format!("容量耗尽，{} 个任务顺延至视界末尾", overflow_ids.len()),
            task_ids: overflow_ids,
        });
    }
    if has_cycle {
        warnings.push(PlanWarning {
            kind: "cycle_detected".to_string(),
            message: "任务依赖存在环，已按排序顺序降级分配".to_string(),
            task_ids: vec![],
        });
    }

    // ---------- 10. plan_hash（确定性：按 task_id 排序；coordinate 附加 goal_ids） ----------
    let plan_hash = build_plan_hash(
        &req.goal_id,
        &req.scope,
        req.goal_ids.as_deref(),
        workload,
        capacity,
        horizon_start,
        horizon_end,
        &placements,
    );

    Ok(ReplanPlan {
        goal_id: req.goal_id.clone(),
        scope: req.scope.clone(),
        feasibility: FeasibilityReport {
            feasible,
            workload,
            capacity,
            slack,
            level: level.to_string(),
            reason: reason.map(|s| s.to_string()),
            options,
        },
        placements,
        day_load,
        warnings,
        metrics: ReplanMetrics {
            moved,
            qty_changed: qty_changed_total,
            retained: retained_total,
            locked: locked_total,
            overdue_recovered,
            jain_index: None,
            predicted_finish: predicted_finish.map(|d| d.format("%Y-%m-%d").to_string()),
            elapsed_ms: 0,
        },
        plan_hash,
        trigger: None,
    })
}

/// 只读求解入口（永不落库）
///
/// 装配快照（复用 `planner::snapshot::load_snapshot`）+ 画像/策略后
/// 调用纯求解 [`solve_inner`]。
pub async fn solve(pool: &SqlitePool, req: &ReplanSolveInput) -> AppResult<ReplanPlan> {
    // S-05：入参校验（命令层已校验，此处防御性复验）
    req.validate()?;

    // 跨目标协同（A5）：目标集分摊 + 各目标内部求解 + 迭代再平衡
    if req.scope == "coordinate" {
        return solve_coordinate(pool, req).await;
    }

    let today = chrono::Local::now().date_naive();
    // 目标存在性（E-NOTFOUND）与 deadline 解析
    let root_goal: Goal = sqlx::query_as("SELECT * FROM goals WHERE id = ?")
        .bind(&req.goal_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("目标不存在".into()))?;
    let end = resolve_end(req, &root_goal, today)?;
    let start = today + Duration::days(1);

    let snapshot = load_snapshot(pool, &req.goal_id, &HorizonSpan { start, end }).await?;
    let profile = load_planning_profile(pool).await?;
    let policy = load_goal_policy(pool, &req.goal_id).await?;

    let started = std::time::Instant::now();
    let mut plan = solve_inner(&snapshot, req, &profile, &policy, None)?;
    plan.metrics.elapsed_ms = started.elapsed().as_millis() as u64;
    Ok(plan)
}

// ============================================================
// 跨目标协同（A5，§4.12）
// ============================================================

/// 确定性 plan_hash：scope=coordinate 时前缀附加按序 goal_ids 摘要
/// （保证 apply 以相同 goal_ids 重解时 hash 一致）。
/// 条目按 task_id 排序，逐项含 old/new 日期与数量、变更类型与理由。
fn build_plan_hash(
    goal_id: &str,
    scope: &str,
    goal_ids: Option<&[String]>,
    workload: f64,
    capacity: f64,
    start: NaiveDate,
    end: NaiveDate,
    placements: &[TaskPlacement],
) -> String {
    let mut sorted: Vec<&TaskPlacement> = placements.iter().collect();
    sorted.sort_by(|a, b| a.task_id.cmp(&b.task_id));
    let mut canonical = if scope == "coordinate" {
        let mut ids: Vec<String> = goal_ids.unwrap_or_default().to_vec();
        ids.sort();
        format!(
            "replan|v1|{}|coordinate|{}|{:.6}|{:.6}|{}|{}|{}",
            goal_id,
            ids.join(","),
            workload,
            capacity,
            start.format("%Y-%m-%d"),
            end.format("%Y-%m-%d"),
            sorted.len()
        )
    } else {
        format!(
            "replan|v1|{}|{}|{:.6}|{:.6}|{}|{}|{}",
            goal_id,
            scope,
            workload,
            capacity,
            start.format("%Y-%m-%d"),
            end.format("%Y-%m-%d"),
            sorted.len()
        )
    };
    for p in sorted {
        canonical.push_str(&format!(
            "|{};{};{};{:.6};{:.6};{};{}",
            p.task_id,
            p.old_plan_date.as_deref().unwrap_or(""),
            p.new_plan_date.as_deref().unwrap_or(""),
            p.old_plan_qty,
            p.new_plan_qty,
            p.change_type,
            p.reason
        ));
    }
    format!("sha256:{}", sha256_hex(canonical.as_bytes()))
}

/// 协同目标集合（§4.12.3）：`goal_ids` 显式传入则按其精确集合（去重排序）；
/// 空/None 时按 deadline 90 天内聚类（选项 B 默认行为），并恒含主目标。
async fn resolve_coordinate_goals(
    pool: &SqlitePool,
    req: &ReplanSolveInput,
    today: NaiveDate,
) -> AppResult<Vec<String>> {
    let mut ids: Vec<String> = match &req.goal_ids {
        Some(list) if !list.is_empty() => list.clone(),
        _ => {
            let cutoff = today + Duration::days(COORDINATE_DEFAULT_DAYS);
            let goals: Vec<Goal> =
                sqlx::query_as("SELECT * FROM goals").fetch_all(pool).await?;
            let mut clustered: Vec<String> = goals
                .into_iter()
                .filter(|g| {
                    g.id != req.goal_id
                        && g.deadline.as_deref().is_some_and(|ds| {
                            NaiveDate::parse_from_str(ds, "%Y-%m-%d")
                                .map(|d| d <= cutoff)
                                .unwrap_or(false)
                        })
                })
                .map(|g| g.id)
                .collect();
            if !clustered.contains(&req.goal_id) {
                clustered.push(req.goal_id.clone());
            }
            clustered
        }
    };
    ids.sort();
    ids.dedup();
    Ok(ids)
}

/// 重算外部负载（协同口径）：排除协同目标集合（含其子树）后，其余目标的
/// 任务按日聚合。协同集合内各目标共享全局额度，彼此不视为外部负载。
async fn compute_external_excluding(
    pool: &SqlitePool,
    coord_paths: &[String],
    start: NaiveDate,
    end: NaiveDate,
) -> AppResult<HashMap<NaiveDate, f64>> {
    // 1. 协同集合的完整目标 id 集（自身 + 子树）
    let all: Vec<(String, String)> = sqlx::query_as("SELECT id, path FROM goals")
        .fetch_all(pool)
        .await?;
    let mut excl: Vec<String> = all
        .into_iter()
        .filter(|(_, path)| {
            coord_paths.iter().any(|p| {
                path == p || path.starts_with(&format!("{}/", p))
            })
        })
        .map(|(id, _)| id)
        .collect();
    excl.sort();
    excl.dedup();
    let mut ext: HashMap<NaiveDate, f64> = HashMap::new();
    if excl.is_empty() {
        return Ok(ext);
    }
    // 2. 其余目标任务按日聚合（口径与 snapshot.rs 一致：排除 skipped）
    let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new(
        "SELECT plan_date, CAST(COALESCE(SUM(plan_qty),0) AS REAL) AS total_qty
         FROM tasks WHERE plan_date IS NOT NULL",
    );
    qb.push(" AND plan_date >= ");
    qb.push_bind(start.format("%Y-%m-%d").to_string());
    qb.push(" AND plan_date <= ");
    qb.push_bind(end.format("%Y-%m-%d").to_string());
    qb.push(" AND status != 'skipped' AND goal_id NOT IN (");
    let mut sep = qb.separated(", ");
    for id in &excl {
        sep.push_bind(id);
    }
    sep.push_unseparated(") GROUP BY plan_date");
    let rows: Vec<(Option<String>, f64)> = qb.build_query_as().fetch_all(pool).await?;
    for (date_str, total_qty) in rows {
        if let Some(ds) = date_str {
            if let Ok(d) = NaiveDate::parse_from_str(&ds, "%Y-%m-%d") {
                ext.insert(d, total_qty);
            }
        }
    }
    Ok(ext)
}

/// 从配额 map 按比例让渡 `total_reduce`（供 A5 阶段四 donor 扣减）
fn reduce_quota(quota: &mut HashMap<NaiveDate, f64>, total_reduce: f64) {
    let sum: f64 = quota.values().sum();
    if sum <= CAP_EPS {
        return;
    }
    let scale = ((sum - total_reduce) / sum).max(0.0);
    for v in quota.values_mut() {
        *v *= scale;
    }
}

/// 向配额 map 按比例增补 `total_add`（供 A5 阶段四 shortfall 目标增额）；
/// 空 map 时按视界日期均摊。
fn augment_quota(quota: &mut HashMap<NaiveDate, f64>, total_add: f64, dates: &[NaiveDate]) {
    if dates.is_empty() {
        return;
    }
    if quota.is_empty() {
        let per = total_add / dates.len() as f64;
        for d in dates {
            *quota.entry(*d).or_insert(0.0) += per;
        }
    } else {
        let per = total_add / quota.len() as f64;
        for v in quota.values_mut() {
            *v += per;
        }
    }
}

/// Jain 公平指数（§4.12.1）：J = (Σx)² / (n·Σx²)，x_i = 已分配_i / 需求_i。
/// 需求为零的目标按 1.0 计（已满足不惩罚）。
fn jain_index(allocated: &[f64], demands: &[f64]) -> f64 {
    let n = allocated.len();
    if n < 2 {
        return 1.0;
    }
    let xs: Vec<f64> = allocated
        .iter()
        .zip(demands)
        .map(|(a, d)| {
            if *d <= CAP_EPS {
                1.0
            } else {
                a / d
            }
        })
        .collect();
    let sum: f64 = xs.iter().sum();
    let sq: f64 = xs.iter().map(|x| x * x).sum();
    if sq <= CAP_EPS {
        return 1.0;
    }
    (sum * sum) / (n as f64 * sq)
}

/// 跨目标协同求解（A5，§4.12）：永不落库。
///
/// 阶段一 计算各目标 W / Pri；阶段二 最大最小公平 + 轮转分摊全局每日额度
/// （全局额度 = Σ 目标单日容量，口径与 [`own_day_cap`] 一致）；阶段三 注入
/// per-goal quota 复用 [`solve_inner`]；阶段四 迭代再平衡 ≤5 轮，不收敛记
/// `low_fairness` warning。返回合并后的单一 [`ReplanPlan`]（placements/day_load
/// 为全局合并，plan_hash 附加按序 goal_ids 摘要）。
pub async fn solve_coordinate(pool: &SqlitePool, req: &ReplanSolveInput) -> AppResult<ReplanPlan> {
    let started = std::time::Instant::now();
    let today = chrono::Local::now().date_naive();
    let profile = load_planning_profile(pool).await?;

    // ---------- 1. 目标集合（§4.12.3） ----------
    let ids = resolve_coordinate_goals(pool, req, today).await?;

    // ---------- 2. 装配各目标上下文（阶段一：W / Pri） ----------
    let overrides = req.overrides.clone();
    let override_capacity = overrides.as_ref().and_then(|o| o.daily_capacity);
    let override_intensity = overrides.as_ref().and_then(|o| o.intensity);
    let mut ctxs: Vec<CoordinateCtx> = Vec::new();
    for gid in &ids {
        let goal: Goal = sqlx::query_as("SELECT * FROM goals WHERE id = ?")
            .bind(gid)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| AppError::NotFound("目标不存在".into()))?;
        let end = resolve_end(req, &goal, today)?;
        let start = today + Duration::days(1);
        let snapshot = load_snapshot(pool, gid, &HorizonSpan { start, end }).await?;
        let workload: f64 = snapshot
            .active_tasks
            .iter()
            .map(|t| (t.plan_qty - t.actual_qty).max(0.0))
            .sum();
        let policy = load_goal_policy(pool, gid).await?;
        ctxs.push(CoordinateCtx {
            goal_id: gid.clone(),
            goal,
            policy,
            snapshot,
            end,
            workload,
            priority: 1.0,
            quota: HashMap::new(),
            plan: None,
        });
    }
    ctxs.retain(|c| c.workload > 0.0);
    if ctxs.is_empty() {
        return Err(AppError::Business("协同目标集中没有剩余工作量".into()));
    }
    // 各目标优先级（goals.priority，缺省 1.0）
    for c in &mut ctxs {
        c.priority = c.goal.priority.max(0.01);
    }

    // 协同集合内目标不互为外部负载：重算排除 S（含子树）后的外部负载，
    // 覆盖各快照原始 external_load（其会把协同目标误计为外部负载）。
    let coord_paths: Vec<String> = ctxs.iter().map(|c| c.goal.path.clone()).collect();
    let ext_start = today + Duration::days(1);
    let ext_end = ctxs.iter().map(|c| c.end).max().unwrap_or(ext_start);
    let ext_map = compute_external_excluding(pool, &coord_paths, ext_start, ext_end).await?;
    for c in &mut ctxs {
        c.snapshot.external_load = ext_map.clone();
    }

    // ---------- 3. 全局额度分摊（阶段二：最大最小公平 + 轮转） ----------
    let horizon_start = today + Duration::days(1);
    let horizon_end = ctxs.iter().map(|c| c.end).max().unwrap_or(horizon_start);
    // 需求序：按 W/Pri 降序（轮转基准序，平局按 goal_id）
    ctxs.sort_by(|a, b| {
        let ka = a.workload / a.priority;
        let kb = b.workload / b.priority;
        kb.partial_cmp(&ka)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.goal_id.cmp(&b.goal_id))
    });
    let mut date = horizon_start;
    while date <= horizon_end {
        let global_cap: f64 = ctxs
            .iter()
            .map(|c| {
                own_day_cap(
                    date,
                    &profile,
                    &c.policy,
                    override_capacity.or(c.goal.daily_capacity),
                    override_intensity.unwrap_or(c.policy.intensity),
                )
            })
            .sum();
        let mut remaining = global_cap;
        let n = ctxs.len();
        let mut idx = 0usize;
        while remaining > CAP_EPS {
            let mut progress = false;
            for _ in 0..n {
                let c = &mut ctxs[idx % n];
                idx += 1;
                let own = own_day_cap(
                    date,
                    &profile,
                    &c.policy,
                    override_capacity.or(c.goal.daily_capacity),
                    override_intensity.unwrap_or(c.policy.intensity),
                );
                let already = c.quota.get(&date).copied().unwrap_or(0.0);
                let headroom = (own - already).max(0.0);
                if headroom <= CAP_EPS {
                    continue;
                }
                let take = remaining.min(headroom);
                *c.quota.entry(date).or_insert(0.0) += take;
                remaining -= take;
                progress = true;
                if remaining <= CAP_EPS {
                    break;
                }
            }
            if !progress {
                break;
            }
        }
        date += Duration::days(1);
    }

    // ---------- 4/5. 各目标内部求解 + 迭代再平衡（阶段三/四，≤5 轮） ----------
    let all_ids: Vec<String> = ctxs.iter().map(|c| c.goal_id.clone()).collect();
    let mut low_fairness = false;
    for _round in 0..=COORDINATE_MAX_ROUNDS {
        let mut shortfall: HashMap<String, f64> = HashMap::new();
        let mut slack_qty: HashMap<String, f64> = HashMap::new();
        for c in &mut ctxs {
            let per_req = ReplanSolveInput {
                goal_id: c.goal_id.clone(),
                scope: "coordinate".to_string(),
                goal_ids: Some(all_ids.clone()),
                dry_run: true,
                overrides: overrides.clone(),
                expected_plan_hash: None,
            };
            let plan = solve_inner(&c.snapshot, &per_req, &profile, &c.policy, Some(&c.quota))?;
            let overflow_qty: f64 = plan
                .placements
                .iter()
                .filter(|p| p.reason == "capacity_trim")
                .map(|p| p.new_plan_qty)
                .sum();
            shortfall.insert(c.goal_id.clone(), overflow_qty);
            slack_qty.insert(
                c.goal_id.clone(),
                (plan.feasibility.capacity - plan.feasibility.workload).max(0.0),
            );
            c.plan = Some(plan);
        }
        let total_shortfall: f64 = shortfall.values().sum();
        if total_shortfall <= CAP_EPS {
            break;
        }
        if _round == COORDINATE_MAX_ROUNDS {
            low_fairness = true;
            break;
        }
        let donors: Vec<&CoordinateCtx> = ctxs
            .iter()
            .filter(|c| slack_qty.get(&c.goal_id).copied().unwrap_or(0.0) > CAP_EPS)
            .collect();
        let total_slack: f64 = donors
            .iter()
            .map(|c| slack_qty.get(&c.goal_id).copied().unwrap_or(0.0))
            .sum();
        if donors.is_empty() || total_slack <= CAP_EPS {
            low_fairness = true;
            break;
        }
        let transfer = total_shortfall.min(total_slack);
        // donor 按 slack 比例让渡
        for c in &mut ctxs {
            let s = slack_qty.get(&c.goal_id).copied().unwrap_or(0.0);
            if s <= CAP_EPS {
                continue;
            }
            let reduce_by = transfer * (s / total_slack);
            reduce_quota(&mut c.quota, reduce_by);
        }
        // shortfall 目标按缺口比例增补
        for c in &mut ctxs {
            let sf = shortfall.get(&c.goal_id).copied().unwrap_or(0.0);
            if sf <= CAP_EPS {
                continue;
            }
            let add = transfer * (sf / total_shortfall);
            let dates: Vec<NaiveDate> = {
                let start_d = today + Duration::days(1);
                let mut ds = Vec::new();
                let mut d = start_d;
                while d <= c.end {
                    ds.push(d);
                    d += Duration::days(1);
                }
                ds
            };
            augment_quota(&mut c.quota, add, &dates);
        }
    }

    // ---------- 6. 合并（MultiGoalPlan → ReplanPlan） ----------
    let mut placements: Vec<TaskPlacement> = Vec::new();
    let mut warnings: Vec<PlanWarning> = Vec::new();
    let mut moved = 0usize;
    let mut qty_changed_total = 0usize;
    let mut retained_total = 0usize;
    let mut locked_total = 0usize;
    let mut overdue_recovered = 0usize;
    let mut workload_total = 0.0;
    let mut capacity_total = 0.0;
    let mut feasible = true;
    let mut reason_shortfall = false;
    let mut reason_chain = false;
    let mut options: Vec<ReplanOption> = Vec::new();
    let mut predicted_finish: Option<NaiveDate> = None;
    // 全局 day_load（含各目标自身任务，外部负载已被整体纳入视界，不再重复加）
    let mut day_map: BTreeMap<NaiveDate, (i64, f64, i64, f64)> = BTreeMap::new();
    for c in &ctxs {
        let p = c.plan.as_ref().ok_or_else(|| {
            AppError::Internal(format!("协同目标 {} 未完成求解", c.goal_id))
        })?;
        placements.extend(p.placements.iter().cloned());
        warnings.extend(p.warnings.iter().cloned());
        moved += p.metrics.moved;
        qty_changed_total += p.metrics.qty_changed;
        retained_total += p.metrics.retained;
        locked_total += p.metrics.locked;
        overdue_recovered += p.metrics.overdue_recovered;
        workload_total += p.feasibility.workload;
        capacity_total += p.feasibility.capacity;
        feasible &= p.feasibility.feasible;
        if !p.feasibility.feasible {
            match p.feasibility.reason.as_deref() {
                Some("capacity_shortfall") => reason_shortfall = true,
                _ => reason_chain = true,
            }
            if options.is_empty() {
                options = p.feasibility.options.clone();
            }
        }
        if let Some(nd) = &p.metrics.predicted_finish {
            if let Ok(d) = NaiveDate::parse_from_str(nd, "%Y-%m-%d") {
                predicted_finish = Some(predicted_finish.map_or(d, |m| m.max(d)));
            }
        }
        for dl in &p.day_load {
            if let Ok(d) = NaiveDate::parse_from_str(&dl.date, "%Y-%m-%d") {
                let e = day_map.entry(d).or_insert((0, 0.0, 0, 0.0));
                e.0 += dl.before_tasks;
                e.1 += dl.before_qty;
                e.2 += dl.after_tasks;
                e.3 += dl.after_qty;
            }
        }
    }
    let day_load: Vec<DayLoadAfter> = day_map
        .into_iter()
        .map(|(d, (bt, bq, at, aq))| DayLoadAfter {
            date: d.format("%Y-%m-%d").to_string(),
            before_tasks: bt,
            before_qty: bq,
            after_tasks: at,
            after_qty: aq,
        })
        .collect();
    if low_fairness {
        warnings.push(PlanWarning {
            kind: "low_fairness".to_string(),
            message: "跨目标协同迭代 5 轮仍未收敛，部分目标被挤压，建议手动调整".to_string(),
            task_ids: vec![],
        });
    }
    // 公平指数（Jain，§4.12.1）：x = 分摊额度 / 需求
    let allocated: Vec<f64> = ctxs.iter().map(|c| c.quota.values().sum::<f64>()).collect();
    let demands: Vec<f64> = ctxs.iter().map(|c| c.workload).collect();
    let jain = jain_index(&allocated, &demands);

    let slack_total = if capacity_total > 0.0 {
        (capacity_total - workload_total) / capacity_total
    } else {
        0.0
    };
    let level = if slack_total < 0.05 {
        "tight"
    } else if slack_total < 0.25 {
        "normal"
    } else {
        "relaxed"
    };
    let reason = if !feasible {
        if reason_shortfall {
            Some("capacity_shortfall")
        } else if reason_chain {
            Some("dependency_chain_too_long")
        } else {
            None
        }
    } else {
        None
    };

    let plan_hash = build_plan_hash(
        &req.goal_id,
        "coordinate",
        Some(&all_ids),
        workload_total,
        capacity_total,
        horizon_start,
        horizon_end,
        &placements,
    );

    Ok(ReplanPlan {
        goal_id: req.goal_id.clone(),
        scope: "coordinate".to_string(),
        feasibility: FeasibilityReport {
            feasible,
            workload: workload_total,
            capacity: capacity_total,
            slack: slack_total,
            level: level.to_string(),
            reason: reason.map(|s| s.to_string()),
            options,
        },
        placements,
        day_load,
        warnings,
        metrics: ReplanMetrics {
            moved,
            qty_changed: qty_changed_total,
            retained: retained_total,
            locked: locked_total,
            overdue_recovered,
            jain_index: Some(jain),
            predicted_finish: predicted_finish.map(|d| d.format("%Y-%m-%d").to_string()),
            elapsed_ms: started.elapsed().as_millis() as u64,
        },
        plan_hash,
        trigger: None,
    })
}

// ============================================================
// 单元测试（纯函数层：solve_inner，不碰数据库、不读时钟）
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::Task;
    use crate::services::planner::snapshot::{DaySlot, PlanSnapshot};

    fn make_goal() -> Goal {
        Goal {
            id: "goal-1".into(),
            name: "测试目标".into(),
            description: None,
            parent_id: None,
            path: "/goal-1".into(),
            deadline: Some("2026-11-30".into()),
            total_qty: 100.0,
            unit: "词".into(),
            sort_order: 0,
            created_at: "2026-10-09 10:00:00".into(),
            // daily_capacity 非空 → Hours 模式：每任务成本 = estimated_hours（缺省 1.0）
            daily_capacity: Some(1.0),
            priority: 1.0,
            auto_replan: 1,
            auto_apply: 0,
            risk_flag: None,
        }
    }

    fn make_task(id: &str, plan_qty: f64, plan_date: Option<&str>) -> Task {
        Task {
            id: id.into(),
            goal_id: "goal-1".into(),
            description: None,
            stage_id: None,
            parent_id: None,
            path: format!("/goal-1/{}", id),
            name: format!("task-{}", id),
            plan_date: plan_date.map(|s| s.to_string()),
            overdue_date: None,
            plan_qty,
            actual_qty: 0.0,
            unit: "词".into(),
            status: "pending".into(),
            is_manual: 0,
            source: "auto".into(),
            sort_order: 0,
            created_at: "2026-10-09 10:00:00".into(),
            estimated_hours: Some(1.0),
            repeat_series_id: None,
            schedule_locked: None,
            updated_at: None,
        }
    }

    /// 快照：today=2026-10-09，视界自 2026-10-10 起共 `days` 个全可用日槽
    fn make_snapshot(
        tasks: Vec<Task>,
        deps: HashMap<String, Vec<String>>,
        days: usize,
    ) -> PlanSnapshot {
        let start = NaiveDate::from_ymd_opt(2026, 10, 10).unwrap();
        let slots: Vec<DaySlot> = (0..days)
            .map(|i| DaySlot {
                date: start + Duration::days(i as i64),
                available: true,
                weight: 1.0,
                is_rest: false,
            })
            .collect();
        PlanSnapshot {
            today: NaiveDate::from_ymd_opt(2026, 10, 9).unwrap(),
            root_goal: make_goal(),
            goals: vec![make_goal()],
            days: slots,
            active_tasks: tasks,
            deps,
            external_load: HashMap::new(),
        }
    }

    fn default_profile() -> PlanningProfile {
        // PlanningProfile::default() 已含冷启动安全值（speed_factor=1.0、
        // weekday_capacity 全 1.0、weekend_factor=1.0），容量修正静默
        PlanningProfile::default()
    }

    fn default_policy() -> GoalPolicy {
        // 注意：GoalPolicy::default() 的 intensity 为 0.0（f64 缺省），
        // 会使单日容量归零；生产装配路径在 load_goal_policy 中显式置 1.0
        GoalPolicy {
            intensity: 1.0,
            ..Default::default()
        }
    }

    fn default_req() -> ReplanSolveInput {
        ReplanSolveInput {
            goal_id: "goal-1".into(),
            scope: "single".to_string(),
            goal_ids: None,
            dry_run: true,
            overrides: None,
            expected_plan_hash: None,
        }
    }

    fn placement<'a>(plan: &'a ReplanPlan, id: &str) -> &'a TaskPlacement {
        plan.placements
            .iter()
            .find(|p| p.task_id == id)
            .unwrap_or_else(|| panic!("placement {} 不存在", id))
    }

    #[test]
    fn test_remaining_qty_backfill() {
        // FR-1/G-02：计划 10、已完成 4 → 剩余量回填为 6，日期不变仅数量变化
        let mut t = make_task("A", 10.0, Some("2026-10-10"));
        t.actual_qty = 4.0;
        let snapshot = make_snapshot(vec![t], HashMap::new(), 10);
        let plan = solve_inner(
            &snapshot,
            &default_req(),
            &default_profile(),
            &default_policy(),
            None,
        )
        .unwrap();        let p = placement(&plan, "A");
        assert_eq!(p.new_plan_qty, 6.0);
        assert_eq!(p.old_plan_qty, 10.0);
        assert_eq!(p.new_plan_date.as_deref(), Some("2026-10-10"));
        assert_eq!(p.change_type, "qty_changed");
        assert_eq!(plan.feasibility.workload, 6.0);
    }

    #[test]
    fn test_conservation_invariant() {
        // INV-1：Σ new_plan_qty == W（1e-6 容差），两任务各 5 共 10
        let a = make_task("A", 5.0, Some("2026-10-10"));
        let b = make_task("B", 5.0, Some("2026-10-10"));
        let snapshot = make_snapshot(vec![a, b], HashMap::new(), 10);
        let plan = solve_inner(
            &snapshot,
            &default_req(),
            &default_profile(),
            &default_policy(),
            None,
        )
        .unwrap();        let sum: f64 = plan.placements.iter().map(|p| p.new_plan_qty).sum();
        assert!((sum - 10.0).abs() < CONSERVATION_EPS);
        assert_eq!(plan.feasibility.workload, 10.0);
        // 单日容量 1.0、单任务成本 1.0 → 两个任务落在不同日期（负载均衡）
        let da = placement(&plan, "A").new_plan_date.clone();
        let db = placement(&plan, "B").new_plan_date.clone();
        assert_ne!(da, db);
    }

    #[test]
    fn test_locked_task_unchanged() {
        // INV-4：schedule_locked 非空 → 原位保留，new == old
        let mut t = make_task("A", 10.0, Some("2026-10-10"));
        t.schedule_locked = Some("user".into());
        let snapshot = make_snapshot(vec![t], HashMap::new(), 10);
        let plan = solve_inner(
            &snapshot,
            &default_req(),
            &default_profile(),
            &default_policy(),
            None,
        )
        .unwrap();        let p = placement(&plan, "A");
        assert_eq!(p.change_type, "locked");
        assert!(p.locked);
        assert_eq!(p.new_plan_qty, 10.0);
        assert_eq!(p.new_plan_date.as_deref(), Some("2026-10-10"));
        assert_eq!(p.reason, "locked");
    }

    #[test]
    fn test_dependency_lower_bound() {
        // FR-4：B 依赖 A → B 最早 = A 完成次日（2026-10-11）
        let a = make_task("A", 1.0, Some("2026-10-10"));
        let mut b = make_task("B", 1.0, Some("2026-10-10"));
        b.sort_order = 1;
        let mut deps = HashMap::new();
        deps.insert("B".to_string(), vec!["A".to_string()]);
        let snapshot = make_snapshot(vec![a, b], deps, 10);
        let plan = solve_inner(
            &snapshot,
            &default_req(),
            &default_profile(),
            &default_policy(),
            None,
        )
        .unwrap();        assert_eq!(placement(&plan, "A").new_plan_date.as_deref(), Some("2026-10-10"));
        let pb = placement(&plan, "B");
        assert_eq!(pb.new_plan_date.as_deref(), Some("2026-10-11"));
        assert_eq!(pb.reason, "dep_lower_bound");
        assert_eq!(pb.dep_lower_bound.as_deref(), Some("2026-10-11"));
        assert_eq!(pb.change_type, "date_moved");
    }

    #[test]
    fn test_overdue_pull() {
        // FR-3：原计划日早于今天的任务被前移回收，标记 overdue_pull
        let t = make_task("A", 3.0, Some("2026-10-05"));
        let snapshot = make_snapshot(vec![t], HashMap::new(), 10);
        let plan = solve_inner(
            &snapshot,
            &default_req(),
            &default_profile(),
            &default_policy(),
            None,
        )
        .unwrap();        let p = placement(&plan, "A");
        assert_eq!(p.reason, "overdue_pull");
        let new = NaiveDate::parse_from_str(p.new_plan_date.as_deref().unwrap(), "%Y-%m-%d").unwrap();
        assert!(new >= snapshot.today);
        assert_eq!(p.change_type, "date_moved");
        assert_eq!(plan.metrics.overdue_recovered, 1);
    }

    #[test]
    fn test_infeasible_degrade() {
        // A2：工作量 50 > 容量 5 → 不可行但不抛错，给出三候选 + capacity_tight
        let tasks: Vec<Task> = (0..10)
            .map(|i| make_task(&format!("T{}", i), 5.0, None))
            .collect();
        let snapshot = make_snapshot(tasks, HashMap::new(), 5);
        let plan = solve_inner(
            &snapshot,
            &default_req(),
            &default_profile(),
            &default_policy(),
            None,
        )
        .unwrap();        assert!(!plan.feasibility.feasible);
        assert_eq!(plan.feasibility.reason.as_deref(), Some("capacity_shortfall"));
        assert_eq!(plan.feasibility.options.len(), 3);
        assert!(plan.warnings.iter().any(|w| w.kind == "capacity_tight"));
        // 容量耗尽的任务顺延视界末尾
        assert!(plan.warnings.iter().any(|w| w.kind == "overflow_tail"));
    }

    #[test]
    fn test_plan_hash_idempotent() {
        // 同一输入重复求解 → plan_hash 逐字节一致（确定性）
        let a = make_task("A", 5.0, Some("2026-10-10"));
        let b = make_task("B", 3.0, Some("2026-10-11"));
        let snapshot = make_snapshot(vec![a, b], HashMap::new(), 10);
        let req = default_req();
        let p1 = solve_inner(
            &snapshot,
            &req,
            &default_profile(),
            &default_policy(),
            None,
        )
        .unwrap();        let p2 = solve_inner(
            &snapshot,
            &req,
            &default_profile(),
            &default_policy(),
            None,
        )
        .unwrap();        assert!(p1.plan_hash.starts_with("sha256:"));
        assert_eq!(p1.plan_hash, p2.plan_hash);
    }

    // ============================================================
    // 跨目标协同（A5）合成用例：需临时库（加载全部迁移）
    // ============================================================

    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use sqlx::SqlitePool;

    /// 临时库（应用全部迁移）
    async fn setup_coord_db() -> SqlitePool {
        let dir = std::env::temp_dir().join("selfpilot_test_coord");
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("coord.db");
        let _ = std::fs::remove_file(&db_path);
        let opts = SqliteConnectOptions::new()
            .filename(&db_path)
            .create_if_missing(true)
            .busy_timeout(std::time::Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    /// 插入目标 + `n_tasks` 个待办任务（每天 1 个，plan_qty=1）。
    /// daily_capacity 置 None → Quantity 模式：单日容量 1.0，任务成本 = 数量。
    async fn insert_coord_goal(
        pool: &SqlitePool,
        gid: &str,
        days_to_deadline: i64,
        n_tasks: i64,
    ) {
        let today = chrono::Local::now().date_naive();
        let deadline = (today + Duration::days(days_to_deadline))
            .format("%Y-%m-%d")
            .to_string();
        let path = format!("/{}", gid);
        sqlx::query(
            "INSERT INTO goals
             (id, name, parent_id, path, deadline, total_qty, unit, sort_order,
              created_at, priority, auto_replan, auto_apply, daily_capacity)
             VALUES (?, ?, NULL, ?, ?, ?, '词', 0, '2026-01-01T00:00:00', 1.0, 1, 0, NULL)",
        )
        .bind(gid)
        .bind("测试目标")
        .bind(&path)
        .bind(&deadline)
        .bind(n_tasks as f64)
        .execute(pool)
        .await
        .unwrap();
        for i in 0..n_tasks {
            let tid = format!("{}-t{}", gid, i + 1);
            let tpath = format!("/{}/{}", gid, tid);
            let plan = (today + Duration::days(i + 1))
                .format("%Y-%m-%d")
                .to_string();
            sqlx::query(
                "INSERT INTO tasks
                 (id, goal_id, stage_id, parent_id, path, name, plan_date, plan_qty,
                  actual_qty, unit, status, is_manual, source, sort_order, created_at)
                 VALUES (?, ?, NULL, NULL, ?, '任务', ?, 1, 0, '词', 'pending', 0, 'auto', 0, '2026-01-01T00:00:00')",
            )
            .bind(&tid)
            .bind(gid)
            .bind(&tpath)
            .bind(&plan)
            .execute(pool)
            .await
            .unwrap();
        }
    }

    #[tokio::test]
    async fn test_coordinate_jain_fairness() {
        // A5/DG-16：3 目标等需求等容量 → 配额均摊 x 相等，Jain=1 ≥ 0.85；
        // 合并计划全部任务产出放置，plan_hash 确定可重解。
        let pool = setup_coord_db().await;
        for gid in ["g1", "g2", "g3"] {
            insert_coord_goal(&pool, gid, 5, 5).await;
        }
        let req = ReplanSolveInput {
            goal_id: "g1".into(),
            scope: "coordinate".into(),
            goal_ids: Some(vec!["g1".into(), "g2".into(), "g3".into()]),
            dry_run: true,
            overrides: None,
            expected_plan_hash: None,
        };
        let plan = solve_coordinate(&pool, &req).await.unwrap();
        assert_eq!(plan.scope, "coordinate");
        assert!(plan.feasibility.feasible, "三目标等量应可行");
        assert_eq!(plan.placements.len(), 15, "三目标 × 5 任务全部产出放置");
        let jain = plan.metrics.jain_index.unwrap();
        assert!(jain >= 0.85, "n=3 公平指数应 ≥ 0.85，实际 {jain}");
        assert!(plan.plan_hash.starts_with("sha256:"));
        // 确定性：同输入重解 hash 一致（apply 重解依赖）
        let plan2 = solve_coordinate(&pool, &req).await.unwrap();
        assert_eq!(plan.plan_hash, plan2.plan_hash);
    }
}
