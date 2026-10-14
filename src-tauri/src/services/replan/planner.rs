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
use sqlx::SqlitePool;
use validator::Validate;

use crate::db::models::{
    DayLoadAfter, FeasibilityReport, Goal, PlanWarning, ReplanMetrics, ReplanOption, ReplanPlan,
    ReplanSolveInput, TaskPlacement,
};
use crate::error::{AppError, AppResult};
use crate::services::planner::feasibility::longest_chain_length;
use crate::services::planner::snapshot::{load_snapshot, HorizonSpan, PlanSnapshot};
use crate::services::replan::explain;
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

/// 习惯画像（planning_profile settings JSON 解析结果）
#[derive(Debug, Clone, Default)]
pub struct HabitProfile {
    /// 各星期容量（0=周日 … 6=周六；缺省 1.0）
    weekday_hours: HashMap<u8, f64>,
    /// 周末容量系数（缺省 1.0）
    weekend_factor: f64,
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

/// 读取习惯画像（settings.planning_profile）
async fn load_habit_profile(pool: &SqlitePool) -> AppResult<HabitProfile> {
    let row: Option<String> =
        sqlx::query_scalar("SELECT value FROM settings WHERE key = 'planning_profile'")
            .fetch_optional(pool)
            .await?;
    let mut profile = HabitProfile::default();
    profile.weekend_factor = 1.0;
    let Some(raw) = row else {
        return Ok(profile);
    };
    let parsed: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|_| AppError::Internal("规划画像 JSON 解析失败".into()))?;
    if let Some(weekday_hours) = parsed.get("weekday_hours").and_then(|v| v.as_object()) {
        for (k, v) in weekday_hours.iter() {
            if let (Ok(w), Some(f)) = (k.parse::<u8>(), v.as_f64()) {
                profile.weekday_hours.insert(w, f);
            }
        }
    }
    if let Some(f) = parsed.get("weekend_factor").and_then(|v| v.as_f64()) {
        if f.is_finite() && f >= 0.0 {
            profile.weekend_factor = f;
        }
    }
    Ok(profile)
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
    profile: &HabitProfile,
    policy: &GoalPolicy,
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
            let base = base_capacity
                .or_else(|| profile.weekday_hours.get(&weekday).copied())
                .unwrap_or(1.0);
            let raw = base * profile.weekend_factor * intensity;
            let mut cap = raw;
            if let Some(m) = policy.max_daily_hours {
                cap = cap.min(m);
            }
            if let Some(m) = policy.max_daily_tasks {
                cap = cap.min(m);
            }
            let date_str = date.format("%Y-%m-%d").to_string();
            let ext = snapshot.external_load.get(&date).copied().unwrap_or(0.0);
            let locked_cost: f64 = tasks
                .iter()
                .filter(|t| locked_ids.contains(t.id.as_str()))
                .filter(|t| t.plan_date.as_deref() == Some(date_str.as_str()))
                .map(|t| cost_of(t, units, allocable_of(t)))
                .sum();
            budgets.push(DayBudget {
                date,
                cap,
                available: (cap - ext - locked_cost).max(0.0),
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

    // ---------- 10. plan_hash（确定性：按 task_id 排序） ----------
    let mut sorted: Vec<&TaskPlacement> = placements.iter().collect();
    sorted.sort_by(|a, b| a.task_id.cmp(&b.task_id));
    let mut canonical = format!(
        "replan|v1|{}|{}|{:.6}|{:.6}|{}|{}|{}",
        req.goal_id,
        req.scope,
        workload,
        capacity,
        horizon_start.format("%Y-%m-%d"),
        horizon_end.format("%Y-%m-%d"),
        sorted.len()
    );
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
    let plan_hash = format!("sha256:{}", sha256_hex(canonical.as_bytes()));

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
    let profile = load_habit_profile(pool).await?;
    let policy = load_goal_policy(pool, &req.goal_id).await?;

    let started = std::time::Instant::now();
    let mut plan = solve_inner(&snapshot, req, &profile, &policy)?;
    plan.metrics.elapsed_ms = started.elapsed().as_millis() as u64;
    Ok(plan)
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

    fn default_profile() -> HabitProfile {
        // 注意：HabitProfile::default() 的 weekend_factor 为 0.0（f64 缺省），
        // 会使单日容量归零；生产装配路径在 load_habit_profile 中显式置 1.0
        HabitProfile {
            weekday_hours: HashMap::new(),
            weekend_factor: 1.0,
        }
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
        )
        .unwrap();        let p2 = solve_inner(
            &snapshot,
            &req,
            &default_profile(),
            &default_policy(),
        )
        .unwrap();        assert!(p1.plan_hash.starts_with("sha256:"));
        assert_eq!(p1.plan_hash, p2.plan_hash);
    }
}
