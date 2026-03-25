//! 统一错误类型
//!
//! 提供 x_data 层级的统一错误类型，避免与 a_common 的循环依赖。

#![forbid(unsafe_code)]

use thiserror::Error;

// ============================================================================
// XDataError
// ============================================================================

/// x_data 层统一错误类型
#[derive(Debug, Clone, Error)]
pub enum XDataError {
    // === 数据错误 ===
    #[error("[Data] 序列化错误: {0}")]
    SerializeError(String),
    #[error("[Data] 解析错误: {0}")]
    ParseError(String),
    #[error("[Data] 类型转换错误: {0}")]
    TypeCastError(String),

    // === 状态错误 ===
    #[error("[State] 持仓不存在: {0}")]
    PositionNotFound(String),
    #[error("[State] 账户不存在: {0}")]
    AccountNotFound(String),
    #[error("[State] 锁获取失败: {0}")]
    LockFailed(String),

    // === 业务错误 ===
    #[error("[Business] 数据验证失败: {0}")]
    ValidationFailed(String),
    #[error("[Business] 状态不一致: {0}")]
    StateInconsistent(String),

    // === 其他 ===
    #[error("[Other] {0}")]
    Other(String),
}

impl XDataError {
    pub fn serialize_error(msg: impl Into<String>) -> Self {
        XDataError::SerializeError(msg.into())
    }

    pub fn parse_error(msg: impl Into<String>) -> Self {
        XDataError::ParseError(msg.into())
    }

    pub fn position_not_found(symbol: impl Into<String>) -> Self {
        XDataError::PositionNotFound(symbol.into())
    }

    pub fn other(msg: impl Into<String>) -> Self {
        XDataError::Other(msg.into())
    }
}
