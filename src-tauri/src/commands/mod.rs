pub mod backup;
pub mod encouragement;
pub mod goal;
pub mod progress;
pub mod settings;
pub mod stats;
pub mod task;

/// 统一 Tauri invoke handler（R-10 命令注册重构）
///
/// 命令注册清单从 lib.rs 迁移至本模块，并按领域分组便于维护；
/// `generate_handler!` 在此版本仅接受命令函数路径（不支持嵌套子 handler），
/// 故采用单一生成器收敛各模块命令，让 lib.rs 无需维护 90 行手动注册表。
///
/// 返回闭包同时实现 `Fn(Invoke<Wry>) -> bool`，可直接传给 `Builder::invoke_handler`。
pub fn build_handler() -> impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        // 目标
        goal::create_goal,
        goal::list_goals,
        goal::list_goal_tree,
        goal::get_goal,
        goal::update_goal,
        goal::delete_goal,
        goal::auto_split,
        goal::split_by_capacity,
        goal::smart_split,
        goal::repeat_split,
        goal::replan_preview,
        goal::replan_goal,
        goal::move_goal,
        // 任务与依赖
        task::create_task,
        task::complete_task,
        task::skip_task,
        task::backfill_task,
        task::move_task,
        task::update_task_plan_qty,
        task::update_task,
        task::delete_task,
        task::delete_tasks_batch,
        task::list_today_tasks,
        task::list_overdue_tasks,
        task::list_tasks_by_goal,
        task::list_tasks_by_date_range,
        task::set_task_dependency,
        task::list_task_dependencies,
        task::list_task_dependents,
        task::remove_task_dependency,
        task::validate_dependency_chain,
        task::list_task_dependency_records,
        // 进度
        progress::get_goal_progress,
        progress::get_all_goals_progress,
        progress::get_goal_ancestors_progress,
        // 统计
        stats::get_completion_trend,
        stats::get_goal_completion_stats,
        stats::get_heatmap,
        stats::get_completion_predictions,
        stats::get_daily_load,
        stats::get_celebration_achievement,
        // 鼓励语
        encouragement::list_encouragements,
        encouragement::add_encouragement,
        encouragement::update_encouragement,
        encouragement::delete_encouragement,
        encouragement::random_encouragement,
        encouragement::random_encouragement_by_streak,
        encouragement::random_celebration_encouragement,
        encouragement::get_streak,
        encouragement::get_setback_situation,
        encouragement::batch_delete_encouragements,
        encouragement::batch_update_encouragement_level,
        encouragement::hide_preset_encouragement,
        encouragement::reset_hidden_presets,
        encouragement::toggle_favorite,
        encouragement::get_favorites,
        encouragement::is_favorite,
        encouragement::log_encouragement_close,
        encouragement::get_encouragement_stats,
        encouragement::check_longest_streak_milestone,
        encouragement::random_longest_streak_encouragement,
        encouragement::update_encouragement_order,
        // 设置
        settings::get_all_settings,
        settings::get_setting,
        settings::set_setting,
        settings::get_encouragement_settings,
        settings::update_encouragement_settings,
        // 备份
        backup::export_data,
        backup::export_data_to_file,
        backup::import_data,
        backup::backup_database,
        backup::restore_database,
    ]
}