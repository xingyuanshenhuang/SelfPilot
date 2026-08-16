//! 数据库公共操作 helpers
//!
//! R-03：提取原先散落在 commands/*.rs 中的两类样板：
//! 1. not_found：统一 NotFound 错误构造，文案格式 "实体 id 不存在"
//! 2. insert_task_row：统一 tasks 表 16 列 INSERT，消除 15/16 列不一致
//!
//! 现有分布：
//! - NotFound 显式样板 25 处（goal 12、task 10、encouragement 3）
//! - INSERT tasks 样板 7 处（5 处 16 列、2 处 15 列缺 estimated_hours）
//!
//! 说明：
//! fetch_one_or_not_found 的泛型封装因 sqlx::QueryAs 类型系统复杂度未实现，
//! 调用点改用 `fetch_optional(...).await?.ok_or_else(|| not_found("目标", id))` 模式，
//! 相比原写法仍减少 `AppError::NotFound(format!(...))` 的重复。

use sqlx::SqlitePool;

use crate::db::models::Task;
use crate::error::{AppError, AppResult};

// ============================================================
// NotFound helpers
// ============================================================

/// 构造 NotFound 错误，统一文案格式："实体 id 不存在"
///
/// 替代原 25 处 `AppError::NotFound(format!("xxx {} 不存在", id))` 样板。
pub fn not_found(entity: &str, id: &str) -> AppError {
    AppError::NotFound(format!("{} {} 不存在", entity, id))
}

// ============================================================
// INSERT task helper
// ============================================================

/// tasks 表 16 列 INSERT 语句
///
/// 统一原先 5 处 16 列 + 2 处 15 列（缺 estimated_hours）的实现。
/// overdue_date 不包含在此 INSERT 中：该字段由逾期检测异步写入，
/// 新建任务时数据库列默认为 NULL。
pub const INSERT_TASK_SQL: &str = "INSERT INTO tasks \
(id, goal_id, stage_id, parent_id, path, name, plan_date, plan_qty, actual_qty, \
unit, status, is_manual, source, sort_order, created_at, estimated_hours) \
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";

/// 执行一次任务 INSERT（16 列）
///
/// 统一原先 7 处 INSERT INTO tasks 样板，并修复 backup.rs 中 15 列缺
/// estimated_hours 的不一致问题。
pub async fn insert_task_row(pool: &SqlitePool, t: &Task) -> AppResult<()> {
    sqlx::query(INSERT_TASK_SQL)
        .bind(&t.id)
        .bind(&t.goal_id)
        .bind(&t.stage_id)
        .bind(&t.parent_id)
        .bind(&t.path)
        .bind(&t.name)
        .bind(&t.plan_date)
        .bind(t.plan_qty)
        .bind(t.actual_qty)
        .bind(&t.unit)
        .bind(&t.status)
        .bind(t.is_manual)
        .bind(&t.source)
        .bind(t.sort_order)
        .bind(&t.created_at)
        .bind(t.estimated_hours)
        .execute(pool)
        .await?;
    Ok(())
}
