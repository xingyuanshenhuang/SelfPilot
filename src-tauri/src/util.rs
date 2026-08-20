// ============================================================
// R-07 (DUP-B-05)：公共工具函数
// 统一时间戳/UUID/日期解析样板，消除多种时间格式并存。
// ============================================================

use chrono::Local;
use uuid::Uuid;

use crate::error::{AppError, AppResult};

/// 当前本地时间戳，统一格式 `yyyy-MM-ddTHH:mm:ss`
pub fn now_local_ts() -> String {
    Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

/// 生成新 UUID 字符串
pub fn new_uuid() -> String {
    Uuid::new_v4().to_string()
}

/// 解析日期字符串（`yyyy-MM-dd`）为 `NaiveDate`
///
/// 非法格式返回参数错误（用户级提示，含字段标签）。
pub fn parse_date_or_param(s: &str, label: &str) -> AppResult<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|_| AppError::Param(format!("{}日期格式必须为 yyyy-MM-dd", label)))
}