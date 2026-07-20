use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("数据库错误: {0}")]
    Db(#[from] sqlx::Error),

    #[error("迁移错误: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),

    #[error("参数错误: {0}")]
    Param(String),

    #[error("未找到: {0}")]
    NotFound(String),

    #[error("业务逻辑错误: {0}")]
    Business(String),

    #[error("内部错误: {0}")]
    Internal(String),

    #[error("JSON 序列化错误: {0}")]
    Json(#[from] serde_json::Error),
}

pub type AppResult<T> = Result<T, AppError>;

/// 让 AppError 可序列化，以便通过 Tauri IPC 返回前端
impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.to_string().as_ref())
    }
}
