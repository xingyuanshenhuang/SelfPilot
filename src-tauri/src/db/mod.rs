pub mod helpers;
pub mod models;

use sqlx::sqlite::SqlitePool;

/// 包装 SqlitePool 以便通过 Tauri State 共享
pub struct DbPool(pub SqlitePool);

impl std::ops::Deref for DbPool {
    type Target = SqlitePool;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
