//! 订单执行接口
//!
//! 定义订单执行的统一接口。
//! 确保交易所网关封装，其他模块不能直接访问网关内部。

use rust_decimal::Decimal;

// Re-export ExchangeGateway from order module
pub use crate::order::ExchangeGateway;

/// 执行错误类型
#[derive(Debug, Clone, thiserror::Error)]
pub enum ExecutionError {
    #[error("Network error: {0}")]
    Network(String),

    #[error("API error: {0}")]
    Api(String),

    #[error("Insufficient balance: {0}")]
    InsufficientBalance(String),

    #[error("Position limit exceeded: {0}")]
    PositionLimitExceeded(String),

    #[error("Order rejected: {0}")]
    OrderRejected(String),

    #[error("Invalid order: {0}")]
    InvalidOrder(String),

    #[error("Gateway error: {0}")]
    Gateway(String),
}

/// 订单簿提供者接口
pub trait MarketDepthProvider: Send + Sync {
    /// 获取指定品种的买一/卖一
    fn best_bid_ask(&self, symbol: &str) -> Option<(Decimal, Decimal)>;

    /// 获取流动性
    fn liquidity(&self, symbol: &str, depth: Decimal) -> (Decimal, Decimal);
}
