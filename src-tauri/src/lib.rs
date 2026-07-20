mod commands;
mod db;
mod error;
mod services;

use sqlx::sqlite::SqlitePoolOptions;
use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            // 获取应用数据目录并创建
            let app_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_dir)?;

            // 构建 SQLite 数据库连接
            let db_path = app_dir.join("selfpilot.db");
            let db_url = format!("sqlite://{}?mode=rwc", db_path.display());

            // 初始化连接池并执行迁移
            let pool = tauri::async_runtime::block_on(async {
                let pool = SqlitePoolOptions::new()
                    .max_connections(5)
                    .connect(&db_url)
                    .await
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

                sqlx::migrate!("./migrations")
                    .run(&pool)
                    .await
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

                Ok::<_, Box<dyn std::error::Error>>(pool)
            })?;

            // 将连接池注入 Tauri State
            app.manage(db::DbPool(pool));

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // 目标相关
            commands::goal::create_goal,
            commands::goal::list_goals,
            commands::goal::list_goal_tree,
            commands::goal::get_goal,
            commands::goal::update_goal,
            commands::goal::delete_goal,
            commands::goal::auto_split,
            commands::goal::split_by_capacity,
            commands::goal::smart_split,
            commands::goal::repeat_split,
            commands::goal::replan_preview,
            commands::goal::replan_goal,
            commands::goal::move_goal,
            // 任务相关
            commands::task::create_task,
            commands::task::complete_task,
            commands::task::skip_task,
            commands::task::backfill_task,
            commands::task::move_task,
            commands::task::update_task_plan_qty,
            commands::task::update_task,
            commands::task::delete_task,
            commands::task::delete_tasks_batch,
            commands::task::list_today_tasks,
            commands::task::list_overdue_tasks,
            commands::task::list_tasks_by_goal,
            commands::task::list_tasks_by_date_range,
            // 任务依赖（P1-1）
            commands::task::set_task_dependency,
            commands::task::list_task_dependencies,
            commands::task::list_task_dependents,
            commands::task::remove_task_dependency,
            commands::task::validate_dependency_chain,
            commands::task::list_task_dependency_records,
            // 进度相关
            commands::progress::get_goal_progress,
            commands::progress::get_all_goals_progress,
            commands::progress::get_goal_ancestors_progress,
            // 统计相关
            commands::stats::get_completion_trend,
            commands::stats::get_goal_completion_stats,
            commands::stats::get_heatmap,
            commands::stats::get_completion_predictions,
            commands::stats::get_daily_load,
            // 鼓励语相关
            commands::encouragement::list_encouragements,
            commands::encouragement::add_encouragement,
            commands::encouragement::update_encouragement,
            commands::encouragement::delete_encouragement,
            commands::encouragement::random_encouragement,
            commands::encouragement::random_encouragement_by_streak,
            commands::encouragement::random_celebration_encouragement,
            commands::encouragement::get_streak,
            // 设置相关
            commands::settings::get_all_settings,
            commands::settings::get_setting,
            commands::settings::set_setting,
            // P1-4: 鼓励语偏好设置
            commands::settings::get_encouragement_settings,
            commands::settings::update_encouragement_settings,
            // P1-2: 挫折场景检测
            commands::encouragement::get_setback_situation,
            // P1-3: 庆祝成就数据
            commands::stats::get_celebration_achievement,
            // P1-5: 批量操作
            commands::encouragement::batch_delete_encouragements,
            commands::encouragement::batch_update_encouragement_level,
            // P2-5: 预设文案管理
            commands::encouragement::hide_preset_encouragement,
            commands::encouragement::reset_hidden_presets,
            // P3-2: 用户收藏机制
            commands::encouragement::add_favorite,
            commands::encouragement::remove_favorite,
            commands::encouragement::list_favorites,
            // P3-3: 展示反馈学习
            commands::encouragement::record_feedback,
            commands::encouragement::get_feedback_stats,
            // P3-5: 拖拽排序
            commands::encouragement::update_encouragement_order,
            commands::encouragement::batch_update_encouragement_order,
            // P3-6: 独立导入导出
            commands::encouragement::export_encouragements,
            commands::encouragement::import_encouragements,
            // 备份相关
            commands::backup::export_data,
            commands::backup::export_data_to_file,
            commands::backup::import_data,
            commands::backup::backup_database,
            commands::backup::restore_database,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
