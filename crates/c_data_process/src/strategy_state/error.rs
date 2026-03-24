//! 策略状态管理错误类型

use thiserror::Error;

#[derive(Debug, Clone, Eq, PartialEq, Error)]
pub enum StrategyStateError {
    #[error("数据库错误: {0}")]
    Database(String),

    #[error("序列化错误: {0}")]
    Serialization(String),

    #[error("反序列化错误: {0}")]
    Deserialization(String),

    #[error("策略状态不存在: {0}")]
    NotFound(String),

    #[error("无效操作: {0}")]
    InvalidOperation(String),

    #[error("并发冲突: {0}")]
    Conflict(String),
}

impl From<rusqlite::Error> for StrategyStateError {
    fn from(e: rusqlite::Error) -> Self {
        StrategyStateError::Database(e.to_string())
    }
}

impl From<serde_json::Error> for StrategyStateError {
    fn from(e: serde_json::Error) -> Self {
        StrategyStateError::Serialization(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, StrategyStateError>;
