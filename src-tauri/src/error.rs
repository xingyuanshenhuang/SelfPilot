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
}

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    /// S-04 (SEC-M-04)：是否为内部实现类错误
    ///
    /// 内部错误（数据库/迁移/内部逻辑）可能泄露 SQL 语句、表名、文件路径等
    /// 实现细节，序列化时对前端脱敏，详细信息仅写入 tracing 日志。
    fn is_internal(&self) -> bool {
        matches!(
            self,
            AppError::Db(_) | AppError::Migrate(_) | AppError::Internal(_)
        )
    }
}

/// 让 AppError 可序列化，以便通过 Tauri IPC 返回前端
///
/// S-04 (SEC-M-04)：错误信息脱敏
/// - 用户级错误（Param/NotFound/Business）原样返回，供前端展示与流程判断
/// - 内部错误（Db/Migrate/Internal）返回笼统提示，详细信息记录到 tracing 日志
impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if self.is_internal() {
            tracing::error!(error = %self, "命令执行失败（返回前端的错误已脱敏）");
            serializer.serialize_str("内部错误，请稍后重试")
        } else {
            tracing::debug!(error = %self, "命令返回用户级错误");
            serializer.serialize_str(self.to_string().as_ref())
        }
    }
}

/// S-05 (SEC-M-05)：validator 校验失败 → Param 错误
impl From<validator::ValidationErrors> for AppError {
    fn from(errors: validator::ValidationErrors) -> Self {
        // 拼接各字段校验失败的中文提示（自定义校验函数均返回中文 message）
        let msg = errors
            .errors()
            .values()
            .filter_map(|kind| match kind {
                validator::ValidationErrorsKind::Field(errs) => errs.first(),
                _ => None,
            })
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("；");
        AppError::Param(if msg.is_empty() {
            "参数校验失败".to_string()
        } else {
            msg
        })
    }
}
