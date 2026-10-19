//! 重新规划（阶段 3/4）命令门面
//!
//! 在 `services/replan`（求解 + 落库 + 撤销）之上提供 7 个 Tauri 命令：
//! - `replan_solve`：只读求解预览（永不落库，what-if 天然成立）；
//! - `replan_apply`：提交应用（内部重新求解校验 plan_hash，防 stale）；
//! - `replan_revert`：撤销最近一次已应用的规划；
//! - `replan_list_events`：事件时间线（created_at DESC 分页）；
//! - `replan_event_detail`：事件 + 差异明细；
//! - `replan_record_feedback`：画像学习反馈（阶段 4，参数名 `req`）；
//! - `replan_check_triggers`：批量触发检查（阶段 4，无参 → TriggerReport）。
//!
//! 错误信息统一经 `AppError` 脱敏（内部错误序列化时由 error.rs 挡为笼统提示）。

use tauri::State;
use validator::Validate;

use crate::db::models::{
    FeedbackInput, ReplanApplyInput, ReplanApplyResult, ReplanEvent, ReplanEventDetail,
    ReplanPlan, ReplanSolveInput, TriggerReport,
};
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::services::replan::{apply, check_triggers, record_feedback, revert, solve};

/// 重新规划 · 预览（只读求解，永不落库）
#[tauri::command]
pub async fn replan_solve(
    req: ReplanSolveInput,
    state: State<'_, DbPool>,
) -> AppResult<ReplanPlan> {
    // S-05：入参校验（solve 内部还会防御性复验）
    req.validate()?;
    solve(&state.0, &req).await
}

/// 重新规划 · 提交应用
#[tauri::command]
pub async fn replan_apply(
    req: ReplanApplyInput,
    state: State<'_, DbPool>,
) -> AppResult<ReplanApplyResult> {
    apply(&state.0, &req).await
}

/// 重新规划 · 撤销最近一次已应用的规划
#[tauri::command]
pub async fn replan_revert(
    event_id: String,
    state: State<'_, DbPool>,
) -> AppResult<ReplanApplyResult> {
    revert(&state.0, event_id).await
}

/// 重新规划 · 事件时间线（created_at DESC，分页）
#[tauri::command]
pub async fn replan_list_events(
    goal_id: String,
    state: State<'_, DbPool>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> AppResult<Vec<ReplanEvent>> {
    let limit = limit.unwrap_or(50).clamp(1, 200);
    let offset = offset.unwrap_or(0).max(0);
    let events: Vec<ReplanEvent> = sqlx::query_as(
        "SELECT id, goal_id, trigger_type, trigger_source, severity, status, plan_hash,
                snapshot_hash, before_summary, after_summary, metrics, error_message,
                created_at, applied_at
         FROM replan_events WHERE goal_id = ?
         ORDER BY created_at DESC LIMIT ? OFFSET ?",
    )
    .bind(&goal_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.0)
    .await?;
    Ok(events)
}

/// 重新规划 · 事件详情（含差异明细）
#[tauri::command]
pub async fn replan_event_detail(
    event_id: String,
    state: State<'_, DbPool>,
) -> AppResult<ReplanEventDetail> {
    let event: ReplanEvent = sqlx::query_as(
        "SELECT id, goal_id, trigger_type, trigger_source, severity, status, plan_hash,
                snapshot_hash, before_summary, after_summary, metrics, error_message,
                created_at, applied_at
         FROM replan_events WHERE id = ?",
    )
    .bind(&event_id)
    .fetch_optional(&state.0)
    .await?
    .ok_or_else(|| AppError::NotFound("重新规划事件不存在".into()))?;
    let diffs: Vec<crate::db::models::ReplanEventDiff> = sqlx::query_as(
        "SELECT id, event_id, task_id, change_type, old_plan_date, new_plan_date,
                old_plan_qty, new_plan_qty, old_unit, new_unit, reason
         FROM replan_event_diffs WHERE event_id = ? ORDER BY id",
    )
    .bind(&event_id)
    .fetch_all(&state.0)
    .await?;
    Ok(ReplanEventDetail { event, diffs })
}

/// 重新规划 · 画像学习反馈（阶段 4 §4.6）
///
/// 写 `split_feedback` 行 + EWMA 更新 `settings.planning_profile`。
/// 参数名 `req`（Tauri v2 严格按参数名匹配）。
#[tauri::command]
pub async fn replan_record_feedback(
    req: FeedbackInput,
    state: State<'_, DbPool>,
) -> AppResult<()> {
    req.validate()?;
    record_feedback(&state.0, &req).await
}

/// 重新规划 · 批量触发检查（阶段 4 §4.13）
///
/// 评估 deadline 90 天内活跃目标，产出建议条报告（proceed/auto_apply/suppressed）。
/// 无参；供前端进入目标树 / 手动刷新时轮询。
#[tauri::command]
pub async fn replan_check_triggers(state: State<'_, DbPool>) -> AppResult<TriggerReport> {
    check_triggers(&state.0).await
}
