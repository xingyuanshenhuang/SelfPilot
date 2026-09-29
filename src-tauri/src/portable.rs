// ============================================================
// 路径解析（数据目录优先级）
//
// 1. 便携模式：exe 同级目录存在 portable.flag 时，数据统一写入
//    exe 同级 data/（数据库、日志、WebView2 缓存均随目录迁移，
//    不写 %APPDATA%/%LOCALAPPDATA%）。
// 2. 安装版自定义目录：安装程序（NSIS 向导页 / MSI DATADIR 属性）
//    将用户选择的目录写入注册表 HKCU\Software\{identifier}\DataDir，
//    运行时读取并校验（见 registry_data_dir）。
// 3. 安装版默认目录：应用安装路径（exe 同级），见 install_default_data_dir。
// 4. 兜底：应用数据目录（app.path().app_data_dir()）。
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

/// 解析应用数据根目录（便携模式 > 安装版自定义目录 > 安装路径 > 兜底）
///
/// - 便携模式：返回 `<exe 同级>/data`
/// - 安装版自定义：返回注册表 DataDir（安装时用户选择，见 registry_data_dir），
///   无自定义值或值无效时继落到安装路径
/// - 安装版默认：返回应用安装路径（exe 同级，见 install_default_data_dir）
/// - 兜底：`app.path().app_data_dir()`
pub fn resolve_app_dir(app: &tauri::AppHandle) -> AppResult<PathBuf> {
    if let Some(dir) = portable_data_dir() {
        return Ok(dir);
    }
    if let Some(dir) = registry_data_dir(&app.config().identifier) {
        return Ok(dir);
    }
    if let Some(dir) = install_default_data_dir() {
        return Ok(dir);
    }
    app.path()
        .app_data_dir()
        .map_err(|e| AppError::Internal(format!("获取应用数据目录失败: {}", e)))
}

/// 安装版默认数据目录 = 应用安装路径（exe 同级）；便携模式返回 None
pub fn install_default_data_dir() -> Option<PathBuf> {
    if is_portable() {
        None
    } else {
        exe_dir()
    }
}

/// 平滑迁移：将旧默认位置（%APPDATA% 应用数据目录）的数据库复制到安装路径。
///
/// 仅在以下条件同时满足时执行（保证幂等与安全，绝不覆盖既有数据）：
/// - 旧位置存在 `selfpilot.db`，且与目标目录不同；
/// - 目标目录尚不存在 `selfpilot.db`。
///
/// 采用复制而非移动，原文件保留作为安全网。返回是否实际执行了迁移。
pub fn migrate_legacy_database(app: &tauri::AppHandle, target_dir: &std::path::Path) -> AppResult<bool> {
    let legacy = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Internal(format!("获取应用数据目录失败: {}", e)))?;

    // 目标即旧位置，或目标已有库，或旧库不存在：无需迁移
    if legacy == target_dir || target_dir.join("selfpilot.db").exists() || !legacy.join("selfpilot.db").exists() {
        return Ok(false);
    }

    std::fs::create_dir_all(target_dir)
        .map_err(|e| AppError::Internal(format!("创建数据目录失败: {}", e)))?;

    // 复制主库及可能存在的 WAL 副文件；原文件保留
    for name in ["selfpilot.db", "selfpilot.db-wal", "selfpilot.db-shm"] {
        let src = legacy.join(name);
        if src.exists() {
            std::fs::copy(&src, target_dir.join(name)).map_err(|e| {
                AppError::Internal(format!("迁移数据库文件 {} 失败: {}", name, e))
            })?;
        }
    }

    tracing::info!(
        "已迁移旧数据库 {} -> {}",
        legacy.display(),
        target_dir.display()
    );
    Ok(true)
}

/// 读取安装程序写入的自定义数据目录（HKCU\Software\{identifier}\DataDir）
///
/// 键不存在、值非法或目录不可创建时返回 None（属正常回退，不视为错误）。
#[cfg(windows)]
fn registry_data_dir(identifier: &str) -> Option<PathBuf> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(format!(r"Software\{identifier}"))
        .ok()?;
    let raw: String = key.get_value("DataDir").ok()?;
    if !validate_data_dir(&raw) {
        tracing::warn!("注册表 DataDir 值非法，回退默认数据目录");
        return None;
    }
    let dir = PathBuf::from(raw);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::warn!("自定义数据目录不可创建（{}），回退默认数据目录", e);
        return None;
    }
    Some(dir)
}

/// 非 Windows 平台无安装时路径选择，恒回退默认目录
#[cfg(not(windows))]
fn registry_data_dir(_identifier: &str) -> Option<PathBuf> {
    None
}

/// 校验注册表中的自定义数据目录（目录导向的宽松校验，
/// 与 backup.rs 面向单文件的 validate_path_scope 职责不同，不强行复用）
#[cfg(windows)]
fn validate_data_dir(raw: &str) -> bool {
    if raw.is_empty() {
        return false;
    }
    // 禁止 NUL 与 Windows 文件名非法字符
    if raw
        .chars()
        .any(|c| matches!(c, '\0' | '<' | '>' | '"' | '|' | '?' | '*'))
    {
        return false;
    }
    let path = PathBuf::from(raw);
    if !path.is_absolute() {
        return false;
    }
    for comp in path.components() {
        match comp {
            std::path::Component::ParentDir => return false,
            std::path::Component::Normal(name) => {
                if is_reserved_device_name(&name.to_string_lossy()) {
                    return false;
                }
            }
            _ => {}
        }
    }
    true
}

/// 判断是否为 Windows 保留设备名（CON、PRN、AUX、NUL、COM1-9、LPT1-9）
#[cfg(windows)]
fn is_reserved_device_name(name: &str) -> bool {
    let stem = name.to_uppercase();
    let stem = stem.split('.').next().unwrap_or("");
    if matches!(stem, "CON" | "PRN" | "AUX" | "NUL") {
        return true;
    }
    if stem.len() == 4 {
        let (prefix, digit) = stem.split_at(3);
        if (prefix == "COM" || prefix == "LPT") && digit.chars().all(|c| c.is_ascii_digit()) {
            return true;
        }
    }
    false
}