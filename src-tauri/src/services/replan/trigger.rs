//! 智能规划（阶段 4）· 事件触发决策引擎（A6，整合设计 §4.13）
//!
//! - [`should_replan`]：严格按 §4.13.2 伪代码的五层闸门：
//!   severity=Info → Suppress；auto_replan 关闭且非 ManualRequest → Suppress；
//!   冷却期（`planning_cooldown_hours`，ManualRequest 豁免）；幂等
//!   （snapshot_hash 对比 `replan_trigger_state.last_snapshot_hash`）；
//!   影响面 < `planning_min_impact_ratio` → Suppress；
//!   事件 ∈ auto_apply_events 且 `goal.auto_apply=1` → AutoApply；否则 Proceed。
//! - [`dispatch_event`]：命令钩子等离散事件的投递入口：
//!   Proceed 写 `replan_events(status='pending')`，AutoApply 直接求解+应用
//!   （可行才自动应用，不可行降级 pending），Suppress 记 `suppressed_count+1`。
//! - [`check_triggers`]：批量评估 deadline 90 天内活跃目标
//!   （OverdueAccumulated / LoadOverload / PeriodicReview 三类）。
//!   轮询抑制不落库（避免每次进入视图产生重复 suppressed 事件噪声）；
//!   应用计划会刷新触发冷却，防止应用后立即再次建议。

use std::collections::HashSet;

use chrono::{Duration, Local, NaiveDate, NaiveDateTime};
use serde_json::json;
use sqlx::SqlitePool;

use crate::db::models::{
    Goal, ReplanApplyInput, ReplanSolveInput, TriggerGoalResult, TriggerReport,
};
use crate::error::{AppError, AppResult};
use crate::services::planner::snapshot::{load_snapshot, HorizonSpan, PlanSnapshot};
use crate::services::replan::planner::solve;
use crate::services::split_planner::sha256_hex;
use crate::util::{new_uuid, now_local_ts};

// ============================================================
// 事件类型与常量（§4.13.1）
// ============================================================

/// 事件：任务完成（完成量 ≥ 当日计划 120%）
pub const EV_TASK_COMPLETED: &str = "task_completed";
/// 事件：任务跳过（累计跳过 ≥ 3）
pub const EV_TASK_SKIPPED: &str = "task_skipped";
/// 事件：逾期累积（逾期任务数 ≥ overdue_threshold）
pub const EV_OVERDUE_ACCUMULATED: &str = "overdue_accumulated";
/// 事件：目标截止日期变动（≥ 1 天）
pub const EV_GOAL_DEADLINE_CHANGED: &str = "goal_deadline_changed";
/// 事件：目标总量变动
pub const EV_GOAL_TOTAL_CHANGED: &str = "goal_total_changed";
/// 事件：进度滞后（提速建议，不自动改计划）
pub const EV_PROGRESS_LAG: &str = "progress_lag";
/// 事件：单日过载（≥ load_threshold）
pub const EV_LOAD_OVERLOAD: &str = "load_overload";
/// 事件：周期复评（距上次自动规划 ≥ 24h）
pub const EV_PERIODIC_REVIEW: &str = "periodic_review";
/// 事件：用户手动请求（豁免冷却）
pub const EV_MANUAL_REQUEST: &str = "manual_request";

/// 冷却默认小时数（settings.planning_cooldown_hours）
const DEFAULT_COOLDOWN_HOURS: f64 = 6.0;
/// 影响面默认阈值（settings.planning_min_impact_ratio）
const DEFAULT_MIN_IMPACT_RATIO: f64 = 0.10;
/// 周期复评间隔（小时）
const PERIODIC_REVIEW_HOURS: i64 = 24;
/// 协同候选：deadline 90 天内的目标
const COORDINATE_DEADLINE_DAYS: i64 = 90;
/// 无 deadline 时的滚动窗口（与 planner::DEFAULT_WINDOW_DAYS 对齐）
const DEFAULT_WINDOW_DAYS: i64 = 30;
/// 触发求解的规划范围（目标级自动触发覆盖整个子树）
const TRIGGER_SCOPE: &str = "subtree";

/// 事件严重度（§4.13.2 闸门 1：Info 直接抑制）
pub fn severity_for(kind: &str) -> &'static str {
    match kind {
        EV_PROGRESS_LAG => "info",
        _ => "warning",
    }
}

/// 触发事件（钩子 / check_triggers 构造）
#[derive(Debug, Clone)]
pub struct TriggerEvent {
    pub goal_id: String,
    /// 9 类事件类型（见上方常量）
    pub kind: &'static str,
    /// 严重度：info | warning | critical
    pub severity: String,
    /// 人类可读触发原因（写入事件与建议条）
    pub reason_text: String,
    /// 关联任务 id（单任务事件，可空）
    pub task_id: Option<String>,
    /// 附加数据（钩子可注入 impact_ratio 等）
    pub payload: Option<serde_json::Value>,
}

impl TriggerEvent {
    pub fn new(goal_id: String, kind: &'static str, reason_text: String) -> Self {
        TriggerEvent {
            goal_id,
            severity: severity_for(kind).to_string(),
            kind,
            reason_text,
            task_id: None,
            payload: None,
        }
    }
}

// ============================================================
// 决策与策略（§4.13.2）
// ============================================================

/// 决策结果
#[derive(Debug, Clone)]
pub enum Decision {
    /// 放行：写 pending 事件，等待用户确认
    Proceed,
    /// 直接应用（需事件 ∈ auto_apply_events 且 goal.auto_apply=1）
    AutoApply,
    /// 抑制（原因见 payload，供 UI 提示）
    Suppress(String),
}

/// 单次闸门评估结果（含快照哈希与影响面，供 settle 落库）
#[derive(Debug, Clone)]
pub struct GateResult {
    pub decision: Decision,
    pub snapshot_hash: String,
    pub impact_ratio: Option<f64>,
}

/// 触发策略（settings 装配，未设置时用默认值）
pub struct TriggerPolicy {
    pub cooldown_hours: f64,
    pub min_impact_ratio: f64,
    /// 允许自动应用的事件集合（默认仅 TaskCompleted）
    pub auto_apply_events: HashSet<String>,
}

/// 触发状态（replan_trigger_state 行映射）
#[derive(Debug, Clone)]
pub struct TriggerState {
    pub last_snapshot_hash: Option<String>,
    pub last_auto_replan_at: Option<String>,
    pub suppressed_count: i64,
    pub updated_at: String,
}

// ============================================================
// 策略 / 状态读写
// ============================================================

async fn read_f64_setting(pool: &SqlitePool, key: &str) -> AppResult<Option<f64>> {
    let v: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await?;
    Ok(v.and_then(|s| s.parse::<f64>().ok()))
}

/// 装配触发策略（settings.planning_cooldown_hours / planning_min_impact_ratio /
/// planning_auto_apply_events）
pub async fn load_policy(pool: &SqlitePool) -> AppResult<TriggerPolicy> {
    let cooldown_hours = read_f64_setting(pool, "planning_cooldown_hours")
        .await?
        .filter(|f| f.is_finite() && *f > 0.0)
        .unwrap_or(DEFAULT_COOLDOWN_HOURS);
    let min_impact_ratio = read_f64_setting(pool, "planning_min_impact_ratio")
        .await?
        .filter(|f| f.is_finite() && *f >= 0.0)
        .unwrap_or(DEFAULT_MIN_IMPACT_RATIO);
    let mut auto_apply_events: HashSet<String> = HashSet::new();
    auto_apply_events.insert(EV_TASK_COMPLETED.to_string());
    if let Some(raw) = sqlx::query_scalar::<_, Option<String>>(
        "SELECT value FROM settings WHERE key = 'planning_auto_apply_events'",
    )
    .fetch_optional(pool)
    .await?
    {
        if let Ok(list) = serde_json::from_str::<Vec<String>>(raw.as_deref().unwrap_or("")) {
            auto_apply_events = list.into_iter().collect();
        }
    }
    Ok(TriggerPolicy {
        cooldown_hours,
        min_impact_ratio,
        auto_apply_events,
    })
}

async fn load_trigger_state(pool: &SqlitePool, goal_id: &str) -> AppResult<Option<TriggerState>> {
    let row: Option<(Option<String>, Option<String>, i64, String)> = sqlx::query_as(
        "SELECT last_snapshot_hash, last_auto_replan_at, suppressed_count, updated_at
         FROM replan_trigger_state WHERE goal_id = ?",
    )
    .bind(goal_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(h, t, c, u)| TriggerState {
        last_snapshot_hash: h,
        last_auto_replan_at: t,
        suppressed_count: c,
        updated_at: u,
    }))
}

async fn save_trigger_state(
    pool: &SqlitePool,
    goal_id: &str,
    state: &TriggerState,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO replan_trigger_state
            (goal_id, last_snapshot_hash, last_auto_replan_at, suppressed_count, updated_at)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(goal_id) DO UPDATE SET
            last_snapshot_hash   = excluded.last_snapshot_hash,
            last_auto_replan_at  = excluded.last_auto_replan_at,
            suppressed_count     = excluded.suppressed_count,
            updated_at           = excluded.updated_at",
    )
    .bind(goal_id)
    .bind(&state.last_snapshot_hash)
    .bind(&state.last_auto_replan_at)
    .bind(state.suppressed_count)
    .bind(&state.updated_at)
    .execute(pool)
    .await?;
    Ok(())
}

// ============================================================
// 快照哈希（L1 触发幂等）
// ============================================================

/// 确定性快照摘要：目标 + 任务（按 id 排序）+ 依赖边 + 外部负载。
/// 仅依赖数据库状态，同状态两次调用结果一致。
fn snapshot_hash(snapshot: &PlanSnapshot) -> String {
    let mut canonical = format!(
        "snap|v1|{}|{}",
        snapshot.root_goal.id,
        snapshot.today.format("%Y-%m-%d")
    );
    let mut tasks: Vec<&crate::db::models::Task> = snapshot.active_tasks.iter().collect();
    tasks.sort_by(|a, b| a.id.cmp(&b.id));
    for t in tasks {
        canonical.push_str(&format!(
            "|{};{};{:.6};{:.6};{};{}",
            t.id,
            t.plan_date.as_deref().unwrap_or(""),
            t.plan_qty,
            t.actual_qty,
            t.status,
            t.goal_id
        ));
    }
    let mut deps: Vec<(String, Vec<String>)> = snapshot
        .deps
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    deps.sort_by(|a, b| a.0.cmp(&b.0));
    for (k, mut vs) in deps {
        vs.sort();
        canonical.push_str(&format!("|dep;{};{}", k, vs.join(",")));
    }
    let mut ext: Vec<(NaiveDate, f64)> = snapshot
        .external_load
        .iter()
        .map(|(d, f)| (*d, *f))
        .collect();
    ext.sort_by(|a, b| a.0.cmp(&b.0));
    for (d, f) in ext {
        canonical.push_str(&format!("|ext;{};{:.6}", d.format("%Y-%m-%d"), f));
    }
    format!("sha256:{}", sha256_hex(canonical.as_bytes()))
}

/// 时间戳解析（兼容 `yyyy-MM-ddTHH:mm:ss` 与 `yyyy-MM-dd HH:mm:ss`）
fn parse_ts(s: &str) -> Option<NaiveDateTime> {
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Some(dt);
    }
    NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").ok()
}

/// 距给定时间戳的小时数（解析失败返回 None）
fn hours_since(ts: &str) -> Option<f64> {
    let dt = parse_ts(ts)?;
    let elapsed = Local::now().naive_local().signed_duration_since(dt);
    Some(elapsed.num_seconds() as f64 / 3600.0)
}

/// 为目标装配快照（视界 = deadline 或 today+30 起至 today 后一天）
async fn load_snapshot_for_goal(pool: &SqlitePool, goal_id: &str) -> AppResult<PlanSnapshot> {
    let goal: Goal = sqlx::query_as("SELECT * FROM goals WHERE id = ?")
        .bind(goal_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("目标不存在".into()))?;
    let today = Local::now().date_naive();
    let end = goal
        .deadline
        .as_deref()
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        .unwrap_or(today + Duration::days(DEFAULT_WINDOW_DAYS));
    let start = today + Duration::days(1);
    load_snapshot(pool, goal_id, &HorizonSpan { start, end }).await
}

// ============================================================
// 决策引擎（§4.13.2）
// ============================================================

/// 影响面估计（0~1，与 min_impact_ratio 比较）
///
/// 单任务事件优先取钩子注入的 payload.impact_ratio；聚合事件按占比计算；
/// 全局性事件（改期/总量/复评/手动/过载）视为 1.0。
fn estimate_impact(snapshot: &PlanSnapshot, event: &TriggerEvent) -> f64 {
    if let Some(v) = event
        .payload
        .as_ref()
        .and_then(|p| p.get("impact_ratio"))
        .and_then(|v| v.as_f64())
    {
        return v.clamp(0.0, 1.0);
    }
    match event.kind {
        EV_OVERDUE_ACCUMULATED => {
            let today = Local::now().format("%Y-%m-%d").to_string();
            let overdue = snapshot
                .active_tasks
                .iter()
                .filter(|t| {
                    t.plan_date
                        .as_deref()
                        .map(|d| d < today.as_str())
                        .unwrap_or(false)
                })
                .count();
            if snapshot.active_tasks.is_empty() {
                0.0
            } else {
                overdue as f64 / snapshot.active_tasks.len() as f64
            }
        }
        _ => 1.0,
    }
}

/// 决策引擎（§4.13.2 伪代码，五层闸门）
pub async fn should_replan(
    pool: &SqlitePool,
    event: &TriggerEvent,
    policy: &TriggerPolicy,
) -> AppResult<GateResult> {
    let empty = || GateResult {
        decision: Decision::Suppress("Unknown".into()),
        snapshot_hash: String::new(),
        impact_ratio: None,
    };

    // 闸门 1：Info 级事件不可操作（提速建议等）
    if event.severity == "info" {
        return Ok(GateResult {
            decision: Decision::Suppress("NotActionable".into()),
            ..empty()
        });
    }

    // 闸门 2：目标存在性 + 自动规划开关
    let goal: Goal = sqlx::query_as("SELECT * FROM goals WHERE id = ?")
        .bind(&event.goal_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("目标不存在".into()))?;
    if goal.auto_replan == 0 && event.kind != EV_MANUAL_REQUEST {
        return Ok(GateResult {
            decision: Decision::Suppress("AutoReplanDisabled".into()),
            ..empty()
        });
    }

    // 闸门 3：冷却期（ManualRequest 豁免）
    let state = load_trigger_state(pool, &event.goal_id).await?;
    if event.kind != EV_MANUAL_REQUEST {
        if let Some(last) = state.as_ref().and_then(|s| s.last_auto_replan_at.clone()) {
            if let Some(elapsed_h) = hours_since(&last) {
                if elapsed_h < policy.cooldown_hours {
                    return Ok(GateResult {
                        decision: Decision::Suppress(format!(
                            "Cooldown({:.1}h)",
                            policy.cooldown_hours - elapsed_h
                        )),
                        ..empty()
                    });
                }
            }
        }
    }

    // 闸门 4：L1 触发幂等（snapshot 未变化）
    let snapshot = load_snapshot_for_goal(pool, &event.goal_id).await?;
    let h = snapshot_hash(&snapshot);
    if let Some(last_h) = state.as_ref().and_then(|s| s.last_snapshot_hash.clone()) {
        if last_h == h {
            return Ok(GateResult {
                decision: Decision::Suppress("NoMaterialChange".into()),
                snapshot_hash: h,
                impact_ratio: None,
            });
        }
    }

    // 闸门 5：影响面评估
    let impact = estimate_impact(&snapshot, event);
    if impact < policy.min_impact_ratio {
        return Ok(GateResult {
            decision: Decision::Suppress("BelowImpactThreshold".into()),
            snapshot_hash: h,
            impact_ratio: Some(impact),
        });
    }

    // 闸门 6：自动应用（事件 ∈ auto_apply_events 且目标允许）
    if policy.auto_apply_events.contains(event.kind) && goal.auto_apply == 1 {
        return Ok(GateResult {
            decision: Decision::AutoApply,
            snapshot_hash: h,
            impact_ratio: Some(impact),
        });
    }
    Ok(GateResult {
        decision: Decision::Proceed,
        snapshot_hash: h,
        impact_ratio: Some(impact),
    })
}

// ============================================================
// 落库（pending / applied / suppressed + 状态更新）
// ============================================================

/// 投递结果（check_triggers 报告用）
#[derive(Debug, Clone)]
pub struct DispatchOutcome {
    /// proceed | auto_apply | suppressed
    pub decision: String,
    /// 写入的 replan_events id（pending / applied）
    pub event_id: Option<String>,
    pub impact_ratio: Option<f64>,
    pub suppress_reason: Option<String>,
}

async fn write_pending_event(
    pool: &SqlitePool,
    event: &TriggerEvent,
    gate: &GateResult,
) -> AppResult<String> {
    let id = new_uuid();
    let metrics = json!({
        "impact_ratio": gate.impact_ratio,
        "trigger": event.reason_text,
    })
    .to_string();
    sqlx::query(
        "INSERT INTO replan_events
         (id, goal_id, trigger_type, trigger_source, severity, status, plan_hash,
          snapshot_hash, before_summary, after_summary, metrics, error_message,
          created_at, applied_at)
         VALUES (?, ?, ?, 'system', ?, 'pending', NULL, ?, NULL, NULL, ?, NULL, ?, NULL)",
    )
    .bind(&id)
    .bind(&event.goal_id)
    .bind(event.kind)
    .bind(&event.severity)
    .bind(&gate.snapshot_hash)
    .bind(&metrics)
    .bind(now_local_ts())
    .execute(pool)
    .await?;
    Ok(id)
}

async fn write_suppressed_event(
    pool: &SqlitePool,
    event: &TriggerEvent,
    reason: &str,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO replan_events
         (id, goal_id, trigger_type, trigger_source, severity, status, plan_hash,
          snapshot_hash, before_summary, after_summary, metrics, error_message,
          created_at, applied_at)
         VALUES (?, ?, ?, 'system', ?, 'suppressed', NULL, NULL, NULL, NULL, NULL, ?, ?, NULL)",
    )
    .bind(new_uuid())
    .bind(&event.goal_id)
    .bind(event.kind)
    .bind(&event.severity)
    .bind(reason)
    .bind(now_local_ts())
    .execute(pool)
    .await?;
    Ok(())
}

/// 按闸门结果落库（Suppress / Proceed / AutoApply 三分支）
///
/// 用于命令钩子等离散事件：抑制时保留 suppressed 事件埋点（AC-10）。
pub async fn settle_event(
    pool: &SqlitePool,
    event: &TriggerEvent,
    gate: &GateResult,
) -> AppResult<DispatchOutcome> {
    settle_event_impl(pool, event, gate, true).await
}

/// 轮询巡检专用：抑制时不落库、不累计 suppressed_count。
///
/// `check_triggers` 会在每次进入目标树/手动刷新时被调用，若抑制也写事件，
/// 会产生大量重复的 suppressed 噪声；轮询不是离散用户事件，抑制只返回原因即可。
async fn settle_event_polling(
    pool: &SqlitePool,
    event: &TriggerEvent,
    gate: &GateResult,
) -> AppResult<DispatchOutcome> {
    settle_event_impl(pool, event, gate, false).await
}

async fn settle_event_impl(
    pool: &SqlitePool,
    event: &TriggerEvent,
    gate: &GateResult,
    record_suppressed: bool,
) -> AppResult<DispatchOutcome> {
    match &gate.decision {
        Decision::Suppress(reason) => {
            if record_suppressed {
                // suppressed_count + 1（阈值不重置，冷却由 last_auto_replan_at 控制）
                let mut state = load_trigger_state(pool, &event.goal_id)
                    .await?
                    .unwrap_or(TriggerState {
                        last_snapshot_hash: None,
                        last_auto_replan_at: None,
                        suppressed_count: 0,
                        updated_at: now_local_ts(),
                    });
                state.suppressed_count += 1;
                state.updated_at = now_local_ts();
                save_trigger_state(pool, &event.goal_id, &state).await?;
                write_suppressed_event(pool, event, reason).await?;
            }
            Ok(DispatchOutcome {
                decision: "suppressed".into(),
                event_id: None,
                impact_ratio: gate.impact_ratio,
                suppress_reason: Some(reason.clone()),
            })
        }
        Decision::Proceed => {
            let event_id = write_pending_event(pool, event, gate).await?;
            // 该快照已生成建议 → 标记，防止重复建议
            let mut state = load_trigger_state(pool, &event.goal_id)
                .await?
                .unwrap_or(TriggerState {
                    last_snapshot_hash: None,
                    last_auto_replan_at: None,
                    suppressed_count: 0,
                    updated_at: now_local_ts(),
                });
            state.last_snapshot_hash = Some(gate.snapshot_hash.clone());
            state.updated_at = now_local_ts();
            save_trigger_state(pool, &event.goal_id, &state).await?;
            Ok(DispatchOutcome {
                decision: "proceed".into(),
                event_id: Some(event_id),
                impact_ratio: gate.impact_ratio,
                suppress_reason: None,
            })
        }
        Decision::AutoApply => {
            // 求解 + 应用（预览与提交同源：apply 内部重新求解校验 plan_hash）。
            // 仅当可行时自动应用；不可行降级为 pending 建议（交用户裁决）。
            let solve_req = ReplanSolveInput {
                goal_id: event.goal_id.clone(),
                scope: TRIGGER_SCOPE.to_string(),
                goal_ids: None,
                dry_run: true,
                overrides: None,
                expected_plan_hash: None,
            };
            let plan = solve(pool, &solve_req).await?;
            let mut state = load_trigger_state(pool, &event.goal_id)
                .await?
                .unwrap_or(TriggerState {
                    last_snapshot_hash: None,
                    last_auto_replan_at: None,
                    suppressed_count: 0,
                    updated_at: now_local_ts(),
                });
            if plan.feasibility.feasible {
                let apply_req = ReplanApplyInput {
                    goal_id: event.goal_id.clone(),
                    plan_hash: plan.plan_hash.clone(),
                    scope: TRIGGER_SCOPE.to_string(),
                    goal_ids: None,
                    event_id: None,
                    overrides: None,
                    excluded_task_ids: None,
                    accept_risk: false,
                };
                let result = crate::services::replan::apply(pool, &apply_req).await?;
                // apply 以 'manual' 落库 → 修正为系统触发来源
                sqlx::query(
                    "UPDATE replan_events
                     SET trigger_type = ?, trigger_source = 'system', severity = ?
                     WHERE id = ?",
                )
                .bind(event.kind)
                .bind(&event.severity)
                .bind(&result.event_id)
                .execute(pool)
                .await?;
                state.last_auto_replan_at = Some(now_local_ts());
                state.last_snapshot_hash = Some(gate.snapshot_hash.clone());
                state.updated_at = now_local_ts();
                save_trigger_state(pool, &event.goal_id, &state).await?;
                Ok(DispatchOutcome {
                    decision: "auto_apply".into(),
                    event_id: Some(result.event_id),
                    impact_ratio: gate.impact_ratio,
                    suppress_reason: None,
                })
            } else {
                // 不可行：不自动应用，降级 pending（冷却不重置）
                let event_id = write_pending_event(pool, event, gate).await?;
                state.last_snapshot_hash = Some(gate.snapshot_hash.clone());
                state.updated_at = now_local_ts();
                save_trigger_state(pool, &event.goal_id, &state).await?;
                Ok(DispatchOutcome {
                    decision: "proceed".into(),
                    event_id: Some(event_id),
                    impact_ratio: gate.impact_ratio,
                    suppress_reason: None,
                })
            }
        }
    }
}

/// 事件投递入口（命令钩子调用）：策略装配 → 闸门 → 落库。
pub async fn dispatch_event(pool: &SqlitePool, event: &TriggerEvent) -> AppResult<DispatchOutcome> {
    let policy = load_policy(pool).await?;
    let gate = should_replan(pool, event, &policy).await?;
    settle_event(pool, event, &gate).await
}

// ============================================================
// 命令钩子辅助（fire-and-forget，不阻塞主命令）
// ============================================================

/// 计算单任务事件的冲击比（任务剩余 / 目标剩余，须在任务状态变更前调用）
pub async fn task_impact_ratio(
    pool: &SqlitePool,
    goal_id: &str,
    task_remaining: f64,
) -> AppResult<f64> {
    let w: f64 = sqlx::query_scalar(
        "SELECT CAST(COALESCE(SUM(MAX(plan_qty - actual_qty, 0)), 0) AS REAL)
         FROM tasks WHERE goal_id = ? AND status NOT IN ('done','skipped')",
    )
    .bind(goal_id)
    .fetch_one(pool)
    .await?;
    if w > 0.0 {
        Ok((task_remaining / w).clamp(0.0, 1.0))
    } else {
        Ok(1.0)
    }
}

/// 异步投递（fire-and-forget，失败仅记日志，不阻塞主命令）
pub fn spawn_dispatch(pool: SqlitePool, event: TriggerEvent) {
    tokio::spawn(async move {
        if let Err(e) = dispatch_event(&pool, &event).await {
            tracing::warn!("触发投递失败 goal={} kind={}: {}", event.goal_id, event.kind, e);
        }
    });
}

/// 异步投递（单任务事件）：spawn 后先计算冲击比再走闸门
pub fn spawn_dispatch_with_impact(
    pool: SqlitePool,
    mut event: TriggerEvent,
    task_remaining: f64,
) {
    tokio::spawn(async move {
        let impact = task_impact_ratio(&pool, &event.goal_id, task_remaining)
            .await
            .unwrap_or(1.0);
        if let Some(p) = event.payload.as_mut() {
            p["impact_ratio"] = json!(impact);
        }
        if let Err(e) = dispatch_event(&pool, &event).await {
            tracing::warn!("触发投递失败 goal={} kind={}: {}", event.goal_id, event.kind, e);
        }
    });
}

// ============================================================
// 批量检查（前端轮询 replan_check_triggers）
// ============================================================

/// 读取目标级触发阈值（goal_planning_policy，缺省 3 / 7）
async fn load_goal_thresholds(pool: &SqlitePool, goal_id: &str) -> AppResult<(f64, f64)> {
    let row: Option<(Option<f64>, Option<f64>)> = sqlx::query_as(
        "SELECT overdue_threshold, load_threshold FROM goal_planning_policy WHERE goal_id = ?",
    )
    .bind(goal_id)
    .fetch_optional(pool)
    .await?;
    let (o, l) = row.unwrap_or((None, None));
    Ok((o.unwrap_or(3.0), l.unwrap_or(7.0)))
}

/// 构造单目标的候选触发事件（按优先级：逾期 > 过载 > 周期复评）
async fn collect_candidates(
    pool: &SqlitePool,
    goal: &Goal,
) -> AppResult<Vec<TriggerEvent>> {
    let today = Local::now().format("%Y-%m-%d").to_string();
    let today_plus_7 = (Local::now().date_naive() + Duration::days(7))
        .format("%Y-%m-%d")
        .to_string();
    let (overdue_threshold, load_threshold) = load_goal_thresholds(pool, &goal.id).await?;
    let mut events = Vec::new();

    // 1. OverdueAccumulated：逾期任务数 ≥ 阈值
    let overdue: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tasks
         WHERE goal_id = ? AND status IN ('pending','partial') AND plan_date < ?",
    )
    .bind(&goal.id)
    .bind(&today)
    .fetch_one(pool)
    .await?;
    if overdue as f64 >= overdue_threshold {
        events.push(TriggerEvent::new(
            goal.id.clone(),
            EV_OVERDUE_ACCUMULATED,
            format!("{} 个任务已逾期", overdue),
        ));
    }

    // 2. LoadOverload：近 7 天某日任务数 ≥ 阈值
    let loads: Vec<i64> = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tasks
         WHERE goal_id = ? AND plan_date IS NOT NULL
           AND plan_date >= ? AND plan_date <= ? AND status != 'skipped'
         GROUP BY plan_date",
    )
    .bind(&goal.id)
    .bind(&today)
    .bind(&today_plus_7)
    .fetch_all(pool)
    .await?;
    if loads.iter().any(|c| *c as f64 >= load_threshold) {
        events.push(TriggerEvent::new(
            goal.id.clone(),
            EV_LOAD_OVERLOAD,
            format!("存在单日负载 ≥ {} 个任务的日期", load_threshold as i64),
        ));
    }

    // 3. PeriodicReview：距上次自动规划 ≥ 24h
    let state = load_trigger_state(pool, &goal.id).await?;
    let needs_review = match state.as_ref().and_then(|s| s.last_auto_replan_at.as_ref()) {
        None => true,
        Some(last) => match parse_ts(last) {
            Some(dt) => {
                (Local::now().naive_local() - dt).num_hours() >= PERIODIC_REVIEW_HOURS
            }
            None => false,
        },
    };
    if needs_review {
        events.push(TriggerEvent::new(
            goal.id.clone(),
            EV_PERIODIC_REVIEW,
            "距上次自动规划已超过 24 小时".to_string(),
        ));
    }
    Ok(events)
}

/// 批量评估 deadline 90 天内活跃目标，产出建议/抑制报告。
pub async fn check_triggers(pool: &SqlitePool) -> AppResult<TriggerReport> {
    let today = Local::now().date_naive();
    let horizon_end = today + Duration::days(COORDINATE_DEADLINE_DAYS);
    let goals: Vec<Goal> = sqlx::query_as(
        "SELECT DISTINCT g.* FROM goals g
         WHERE g.deadline IS NOT NULL AND g.deadline <= ?
           AND EXISTS (
                SELECT 1 FROM tasks t
                WHERE t.goal_id = g.id AND t.status NOT IN ('done','skipped'))
         ORDER BY g.deadline",
    )
    .bind(horizon_end.format("%Y-%m-%d").to_string())
    .fetch_all(pool)
    .await?;

    let policy = load_policy(pool).await?;
    let mut results: Vec<TriggerGoalResult> = Vec::new();
    let mut pending_count = 0usize;

    for goal in goals {
        let candidates = collect_candidates(pool, &goal).await?;
        if candidates.is_empty() {
            continue;
        }
        // 按优先级取第一个非抑制决策；全部抑制时取第一个（最高优先级，仅用于返回原因）
        let mut first_suppressed: Option<(TriggerEvent, GateResult)> = None;
        let mut chosen: Option<(TriggerEvent, GateResult)> = None;
        for ev in &candidates {
            let gate = should_replan(pool, ev, &policy).await?;
            if matches!(gate.decision, Decision::Suppress(_)) {
                if first_suppressed.is_none() {
                    first_suppressed = Some((ev.clone(), gate));
                }
                continue;
            }
            chosen = Some((ev.clone(), gate));
            break;
        }
        let Some((ev, gate)) = chosen.or(first_suppressed) else {
            continue;
        };
        let outcome = settle_event_polling(pool, &ev, &gate).await?;
        let decision = outcome.decision.clone();
        if decision == "proceed" {
            pending_count += 1;
        }
        let suppress_reason = match &gate.decision {
            Decision::Suppress(r) => Some(r.clone()),
            _ => outcome.suppress_reason,
        };
        results.push(TriggerGoalResult {
            goal_id: goal.id.clone(),
            goal_name: goal.name.clone(),
            decision,
            trigger_type: ev.kind.to_string(),
            severity: ev.severity.clone(),
            reason: Some(ev.reason_text),
            suppress_reason,
            impact_ratio: outcome.impact_ratio,
            event_id: outcome.event_id,
        });
    }

    Ok(TriggerReport {
        goals: results,
        pending_count,
    })
}

// ============================================================
// 单元测试（should_replan 五层闸门 + 冷却豁免）
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    fn test_policy() -> TriggerPolicy {
        TriggerPolicy {
            cooldown_hours: 6.0,
            min_impact_ratio: 0.10,
            auto_apply_events: HashSet::from([EV_TASK_COMPLETED.to_string()]),
        }
    }

    fn is_suppress(gate: &GateResult, prefix: &str) -> bool {
        matches!(&gate.decision, Decision::Suppress(r) if r.starts_with(prefix))
    }

    /// 临时库（应用全部迁移），插入一个带任务的目标后返回连接池
    /// 每个测试独立库文件，避免全量测试并行时共享文件冲突
    async fn setup_db() -> SqlitePool {
        static CNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let idx = CNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("selfpilot_test_trigger_{}", idx));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("trigger.db");
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

    async fn insert_goal(
        pool: &SqlitePool,
        id: &str,
        auto_replan: i64,
        auto_apply: i64,
    ) {
        let deadline = (Local::now().date_naive() + Duration::days(20))
            .format("%Y-%m-%d")
            .to_string();
        let path = format!("/{}", id);
        sqlx::query(
            "INSERT INTO goals
             (id, name, parent_id, path, deadline, total_qty, unit, sort_order,
              created_at, priority, auto_replan, auto_apply)
             VALUES (?, ?, NULL, ?, ?, 20, '', 0, '2026-01-01T00:00:00', 1.0, ?, ?)",
        )
        .bind(id)
        .bind("测试目标")
        .bind(&path)
        .bind(&deadline)
        .bind(auto_replan)
        .bind(auto_apply)
        .execute(pool)
        .await
        .unwrap();
        // 2 个活跃任务（快照装配需要）
        let plan1 = (Local::now().date_naive() + Duration::days(1))
            .format("%Y-%m-%d")
            .to_string();
        let plan2 = (Local::now().date_naive() + Duration::days(2))
            .format("%Y-%m-%d")
            .to_string();
        for (i, plan) in [&plan1, &plan2].iter().enumerate() {
            let tid = format!("{}-t{}", id, i + 1);
            let path = format!("/{}/{}", id, tid);
            sqlx::query(
                "INSERT INTO tasks
                 (id, goal_id, stage_id, parent_id, path, name, plan_date, plan_qty,
                  actual_qty, unit, status, is_manual, source, sort_order, created_at)
                 VALUES (?, ?, NULL, NULL, ?, '任务', ?, 10, 0, '', 'pending', 0, 'auto', 0, '2026-01-01T00:00:00')",
            )
            .bind(&tid)
            .bind(id)
            .bind(&path)
            .bind(plan)
            .execute(pool)
            .await
            .unwrap();
        }
    }

    fn completed_event(goal_id: &str) -> TriggerEvent {
        TriggerEvent::new(goal_id.to_string(), EV_TASK_COMPLETED, "任务超额完成".to_string())
    }

    #[tokio::test]
    async fn gate1_info_suppressed() {
        let pool = setup_db().await;
        insert_goal(&pool, "g1", 1, 0).await;
        // ProgressLag 严重度=info → 闸门 1 直接抑制
        let ev = TriggerEvent::new("g1".into(), EV_PROGRESS_LAG, "进度滞后".into());
        let gate = should_replan(&pool, &ev, &test_policy()).await.unwrap();
        assert!(is_suppress(&gate, "NotActionable"), "{:?}", gate.decision);
    }

    #[tokio::test]
    async fn gate2_auto_replan_disabled_suppressed() {
        let pool = setup_db().await;
        insert_goal(&pool, "g1", 0, 0).await; // auto_replan=0
        let gate = should_replan(&pool, &completed_event("g1"), &test_policy())
            .await
            .unwrap();
        assert!(is_suppress(&gate, "AutoReplanDisabled"), "{:?}", gate.decision);
    }

    #[tokio::test]
    async fn gate3_cooldown_suppressed() {
        let pool = setup_db().await;
        insert_goal(&pool, "g1", 1, 0).await;
        // 插入刚自动规划过的状态 → 进入冷却
        sqlx::query(
            "INSERT INTO replan_trigger_state
             (goal_id, last_snapshot_hash, last_auto_replan_at, suppressed_count, updated_at)
             VALUES ('g1', NULL, ?, 0, ?)",
        )
        .bind(now_local_ts())
        .bind(now_local_ts())
        .execute(&pool)
        .await
        .unwrap();
        let gate = should_replan(&pool, &completed_event("g1"), &test_policy())
            .await
            .unwrap();
        assert!(is_suppress(&gate, "Cooldown("), "{:?}", gate.decision);
    }

    #[tokio::test]
    async fn gate3_manual_bypass_cooldown() {
        let pool = setup_db().await;
        insert_goal(&pool, "g1", 1, 0).await;
        sqlx::query(
            "INSERT INTO replan_trigger_state
             (goal_id, last_snapshot_hash, last_auto_replan_at, suppressed_count, updated_at)
             VALUES ('g1', NULL, ?, 0, ?)",
        )
        .bind(now_local_ts())
        .bind(now_local_ts())
        .execute(&pool)
        .await
        .unwrap();
        // ManualRequest 豁免冷却 → Proceed
        let ev = TriggerEvent::new("g1".into(), EV_MANUAL_REQUEST, "用户手动请求".into());
        let gate = should_replan(&pool, &ev, &test_policy()).await.unwrap();
        assert!(matches!(gate.decision, Decision::Proceed), "{:?}", gate.decision);
    }

    #[tokio::test]
    async fn gate4_idempotent_suppressed() {
        let pool = setup_db().await;
        insert_goal(&pool, "g1", 1, 0).await;
        // 第一次：Proceed，拿到快照哈希
        let gate1 = should_replan(&pool, &completed_event("g1"), &test_policy())
            .await
            .unwrap();
        assert!(matches!(gate1.decision, Decision::Proceed), "{:?}", gate1.decision);
        assert!(!gate1.snapshot_hash.is_empty());
        // 模拟已就同一快照生成过建议 → 第二次 NoMaterialChange
        sqlx::query(
            "INSERT INTO replan_trigger_state
             (goal_id, last_snapshot_hash, last_auto_replan_at, suppressed_count, updated_at)
             VALUES ('g1', ?, NULL, 0, ?)",
        )
        .bind(&gate1.snapshot_hash)
        .bind(now_local_ts())
        .execute(&pool)
        .await
        .unwrap();
        let gate2 = should_replan(&pool, &completed_event("g1"), &test_policy())
            .await
            .unwrap();
        assert!(is_suppress(&gate2, "NoMaterialChange"), "{:?}", gate2.decision);
    }

    #[tokio::test]
    async fn gate5_below_impact_suppressed() {
        let pool = setup_db().await;
        insert_goal(&pool, "g1", 1, 0).await;
        let mut ev = completed_event("g1");
        ev.payload = Some(json!({"impact_ratio": 0.01}));
        let gate = should_replan(&pool, &ev, &test_policy()).await.unwrap();
        assert!(
            is_suppress(&gate, "BelowImpactThreshold"),
            "{:?}",
            gate.decision
        );
    }

    #[tokio::test]
    async fn gate6_auto_apply_when_enabled() {
        let pool = setup_db().await;
        insert_goal(&pool, "g1", 1, 1).await; // auto_apply=1
        let gate = should_replan(&pool, &completed_event("g1"), &test_policy())
            .await
            .unwrap();
        assert!(matches!(gate.decision, Decision::AutoApply), "{:?}", gate.decision);
    }

    #[tokio::test]
    async fn proceeds_when_no_obstacle() {
        let pool = setup_db().await;
        insert_goal(&pool, "g1", 1, 0).await;
        let gate = should_replan(&pool, &completed_event("g1"), &test_policy())
            .await
            .unwrap();
        assert!(matches!(gate.decision, Decision::Proceed), "{:?}", gate.decision);
    }

    #[tokio::test]
    async fn snapshot_hash_deterministic() {
        // 同一快照内容两次哈希一致（纯函数校验：排序稳定性）
        let pool = setup_db().await;
        insert_goal(&pool, "g1", 1, 0).await;
        let snap = load_snapshot_for_goal(&pool, "g1").await.unwrap();
        let h1 = snapshot_hash(&snap);
        let h2 = snapshot_hash(&snap);
        assert_eq!(h1, h2);
        assert!(h1.starts_with("sha256:"));
    }

    #[tokio::test]
    async fn check_triggers_overdue_candidate_proceeds() {
        let pool = setup_db().await;
        insert_goal(&pool, "g1", 1, 0).await;
        // 追加 3 个昨日到期的 pending 任务 → 达到默认 overdue_threshold=3
        let yesterday = (Local::now().date_naive() - Duration::days(1))
            .format("%Y-%m-%d")
            .to_string();
        for i in 0..3 {
            let tid = format!("g1-od{}", i);
            let path = format!("/g1/{}", tid);
            sqlx::query(
                "INSERT INTO tasks
                 (id, goal_id, stage_id, parent_id, path, name, plan_date, plan_qty,
                  actual_qty, unit, status, is_manual, source, sort_order, created_at)
                 VALUES (?, 'g1', NULL, NULL, ?, '逾期任务', ?, 5, 0, '', 'pending', 0, 'auto', 0, '2026-01-01T00:00:00')",
            )
            .bind(&tid)
            .bind(&path)
            .bind(&yesterday)
            .execute(&pool)
            .await
            .unwrap();
        }

        let report = check_triggers(&pool).await.unwrap();
        assert_eq!(report.pending_count, 1, "{:?}", report.goals);
        let result = report
            .goals
            .iter()
            .find(|g| g.goal_id == "g1")
            .expect("g1 应出现在触发报告中");
        assert_eq!(result.decision, "proceed");
        assert_eq!(result.trigger_type, EV_OVERDUE_ACCUMULATED);

        // pending 事件已落库，供前端建议条消费
        let pending: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM replan_events
             WHERE goal_id = 'g1' AND status = 'pending'
               AND trigger_type = 'overdue_accumulated'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(pending, 1);

        // 再次轮询：同快照被幂等抑制，且不写入 suppressed 事件（噪声修复）
        let report2 = check_triggers(&pool).await.unwrap();
        assert_eq!(report2.pending_count, 0);
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM replan_events WHERE goal_id = 'g1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(total, 1, "轮询抑制不应新增事件");
    }

    #[tokio::test]
    async fn check_triggers_polling_suppression_writes_nothing() {
        let pool = setup_db().await;
        insert_goal(&pool, "g1", 0, 0).await; // auto_replan=0 → 每轮均抑制

        let r1 = check_triggers(&pool).await.unwrap();
        assert_eq!(r1.pending_count, 0);
        let g1 = r1.goals.iter().find(|g| g.goal_id == "g1").unwrap();
        assert_eq!(g1.decision, "suppressed");
        assert!(
            g1.suppress_reason
                .as_deref()
                .unwrap_or("")
                .starts_with("AutoReplanDisabled"),
            "{:?}",
            g1.suppress_reason
        );

        let r2 = check_triggers(&pool).await.unwrap();
        assert_eq!(r2.pending_count, 0);
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM replan_events WHERE goal_id = 'g1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(total, 0, "轮询抑制不应落库");
        // 轮询也不累计 suppressed_count
        let count: Option<i64> = sqlx::query_scalar(
            "SELECT suppressed_count FROM replan_trigger_state WHERE goal_id = 'g1'",
        )
        .fetch_optional(&pool)
        .await
        .unwrap();
        assert!(count.unwrap_or(0) == 0);
    }

    #[tokio::test]
    async fn hook_suppressed_event_still_recorded() {
        let pool = setup_db().await;
        insert_goal(&pool, "g1", 0, 0).await;

        // 离散命令钩子仍保留 suppressed 埋点（AC-10）
        let outcome = dispatch_event(&pool, &completed_event("g1")).await.unwrap();
        assert_eq!(outcome.decision, "suppressed");
        let suppressed: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM replan_events
             WHERE goal_id = 'g1' AND status = 'suppressed'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(suppressed, 1);
        let count: i64 = sqlx::query_scalar(
            "SELECT suppressed_count FROM replan_trigger_state WHERE goal_id = 'g1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn apply_refreshes_trigger_cooldown() {
        let pool = setup_db().await;
        insert_goal(&pool, "g1", 1, 0).await;

        let r1 = check_triggers(&pool).await.unwrap();
        assert_eq!(r1.pending_count, 1, "{:?}", r1.goals);

        // 手动应用一次规划
        let plan = crate::services::replan::planner::solve(
            &pool,
            &ReplanSolveInput {
                goal_id: "g1".into(),
                scope: "subtree".into(),
                goal_ids: None,
                dry_run: true,
                overrides: None,
                expected_plan_hash: None,
            },
        )
        .await
        .unwrap();
        crate::services::replan::apply(
            &pool,
            &ReplanApplyInput {
                goal_id: "g1".into(),
                plan_hash: plan.plan_hash.clone(),
                scope: "subtree".into(),
                goal_ids: None,
                event_id: None,
                overrides: None,
                excluded_task_ids: None,
                accept_risk: false,
            },
        )
        .await
        .unwrap();

        // 应用后进入冷却：不会立即再次建议，也不再写 suppressed
        let r2 = check_triggers(&pool).await.unwrap();
        assert_eq!(r2.pending_count, 0);
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM replan_events WHERE goal_id = 'g1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(total, 2, "应为 pending + applied 两条，无新增噪声");
        let state_last: Option<String> = sqlx::query_scalar(
            "SELECT last_auto_replan_at FROM replan_trigger_state WHERE goal_id = 'g1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(state_last.is_some(), "应用后应刷新冷却时间");
    }
}

