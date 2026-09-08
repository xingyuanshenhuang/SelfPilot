// ============================================================
// 便携模式（绿色版）路径解析
//
// 规则：exe 同级目录存在 portable.flag 时，数据统一写入 exe 同级 data/
// （数据库、日志、WebView2 缓存均随目录迁移，不写 %APPDATA%/%LOCALAPPDATA%）；
// 无 flag 时回退到安装版行为（应用数据目录）。
//
// lib.rs 与 backup.rs 必须统一通过本模块获取数据目录，禁止再直接调用
// app.path().app_data_dir()。
// ============================================================

use std::path::PathBuf;

use tauri::Manager;

use crate::error::{AppError, AppResult};

/// 返回 exe 同级目录
fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()?
        .parent()
        .map(|p| p.to_path_buf())
}

/// 是否为便携模式（exe 同级存在 portable.flag）
pub fn is_portable() -> bool {
    exe_dir()
        .map(|dir| dir.join("portable.flag").exists())
        .unwrap_or(false)
}

/// 便携模式数据根目录 `exe同级/data`（非便携返回 None）
pub fn portable_data_dir() -> Option<PathBuf> {
    if is_portable() {
        exe_dir().map(|dir| dir.join("data"))
    } else {
        None
    }
}

/// 便携模式 WebView2 用户数据目录 `exe同级/data/webview`
pub fn portable_webview_dir() -> Option<PathBuf> {
    portable_data_dir().map(|dir| dir.join("webview"))
}

/// 解析应用数据根目录（便携模式 or 安装版回退）
///
/// - 便携模式：返回 `<exe 同级>/data`
/// - 安装版：返回 `app.path().app_data_dir()`
pub fn resolve_app_dir(app: &tauri::AppHandle) -> AppResult<PathBuf> {
    if let Some(dir) = portable_data_dir() {
        return Ok(dir);
    }
    app.path()
        .app_data_dir()
        .map_err(|e| AppError::Internal(format!("获取应用数据目录失败: {}", e)))
}