mod commands;
mod db;
mod error;
mod services;
mod util;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::str::FromStr;
use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            // 获取应用数据目录并创建
            let app_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_dir)?;

            // S-04 (SEC-M-04)：初始化 tracing 日志，按天滚动写入应用数据目录 logs/
            // 内部错误（含 SQL/路径等敏感细节）仅记录到日志，前端只接收脱敏提示
            let log_dir = app_dir.join("logs");
            std::fs::create_dir_all(&log_dir)?;
            let file_appender = tracing_appender::rolling::daily(&log_dir, "selfpilot.log");
            tracing_subscriber::fmt()
                .with_ansi(false)
                .with_writer(file_appender)
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| "info".into()),
                )
                .init();

            // 构建 SQLite 数据库连接
            let db_path = app_dir.join("selfpilot.db");
            let db_url = format!("sqlite://{}?mode=rwc", db_path.display());

            // 初始化连接池并执行迁移
            let pool = tauri::async_runtime::block_on(async {
                // S-03 (SEC-H-03): 启用 SQLite 外键约束，确保级联删除等数据完整性
                let options = SqliteConnectOptions::from_str(&db_url)
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?
                    .foreign_keys(true);

                let pool = SqlitePoolOptions::new()
                    .max_connections(5)
                    .connect_with(options)
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
        // R-10：命令注册按模块拆分，由 commands::build_handler() 统一合并
        .invoke_handler(commands::build_handler())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
