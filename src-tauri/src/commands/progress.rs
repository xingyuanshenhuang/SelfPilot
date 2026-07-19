use tauri::State;

use crate::db::models::ProgressInfo;
use crate::db::DbPool;
use crate::error::AppResult;
use crate::services::progress_service;

/// 获取单个目标的进度
#[tauri::command]
pub async fn get_goal_progress(
    goal_id: String,
    state: State<'_, DbPool>,
) -> AppResult<ProgressInfo> {
    progress_service::calc_goal_progress(&state.0, &goal_id).await
}

/// 获取所有目标的进度
#[tauri::command]
pub async fn get_all_goals_progress(state: State<'_, DbPool>) -> AppResult<Vec<ProgressInfo>> {
    progress_service::calc_all_goals_progress(&state.0).await
}

/// 获取目标及其所有祖先的进度（P2-3：局部更新专用）
///
/// 用于写操作后只重算受影响的祖先链，而非全量重算所有目标进度。
/// 返回顺序：[自身, 父目标, 祖父目标, ...]
#[tauri::command]
pub async fn get_goal_ancestors_progress(
    goal_id: String,
    state: State<'_, DbPool>,
) -> AppResult<Vec<ProgressInfo>> {
    progress_service::calc_goal_ancestors_progress(&state.0, &goal_id).await
}
