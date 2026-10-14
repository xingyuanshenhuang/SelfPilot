//! 重新规划（阶段 3）命令门面
//!
//! 在 `services/replan`（求解 + 落库 + 撤销）之上提供 5 个 Tauri 命令：
//! - `replan_solve`：只读求解预览（永不落库，what-if 天然成立）；
//! - `replan_apply`：提交应用（内部重新求解校验 plan_hash，防 stale）；
//! - `replan_revert`：撤销最近一次已应用的规划；
//! - `replan_list_events`：事件时间线（created_at DESC 分页）；
//! - `replan_event_detail`：事件 + 差异明细。
//!
//! 错误信息统一经 `AppError` 脱敏（内部错误序列化时由 error.rs 挡为笼统提示）。

use tauri::State;
use validator::Validate;

use crate::db::models::{
    ReplanApplyInput, ReplanApplyResult, ReplanEvent, ReplanEventDetail, ReplanPlan,
    ReplanSolveInput,
};
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::services::replan::{apply, revert, solve};

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
