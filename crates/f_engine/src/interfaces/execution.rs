//! 订单执行接口
//!
//! 定义订单执行的统一接口。
//! 确保交易所网关封装，其他模块不能直接访问网关内部。

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use a_common::models::types::OrderStatus;
use a_common::{OrderResult as CommonOrderResult, EngineError};

// Re-export ExchangeGateway from order module to keep interface consistent
pub use crate::order::ExchangeGateway;

/// 订单执行结果
#[derive(Debug, Clone)]
pub struct OrderResult {
    pub order_id: String,
    pub status: OrderStatus,
    pub executed_quantity: Decimal,
    pub executed_price: Decimal,
    pub commission: Decimal,
    pub message: String,
    pub reject_reason: Option<String>,
    pub timestamp: DateTime<Utc>,
}

impl From<CommonOrderResult> for OrderResult {
    fn from(common: CommonOrderResult) -> Self {
        Self {
            order_id: common.order_id,
            status: common.status,
            executed_quantity: common.filled_qty,
            executed_price: common.filled_price,
            commission: common.commission,
            message: common.message,
            reject_reason: common.reject_reason.map(|r| r.to_string()),
            timestamp: Utc::now(),
        }
    }
}

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

impl From<EngineError> for ExecutionError {
    fn from(e: EngineError) -> Self {
        ExecutionError::Gateway(e.to_string())
    }
}

/// 订单簿提供者接口
///
/// 用于获取订单簿数据（深度、流动性等）。
pub trait MarketDepthProvider: Send + Sync {
    /// 获取指定品种的买一/卖一
    fn best_bid_ask(&self, symbol: &str) -> Option<(Decimal, Decimal)>;

    /// 获取流动性（指定价格范围内的挂单总量）
    fn liquidity(&self, symbol: &str, depth: Decimal) -> (Decimal, Decimal);
}
