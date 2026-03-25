//! 订单数据类型

#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

// ============================================================================
// OrderRejectReason
// ============================================================================

/// 订单拒绝原因
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderRejectReason {
    /// 资金不足
    InsufficientBalance,
    /// 超出持仓限制
    PositionLimitExceeded,
    /// 保证金不足
    MarginInsufficient,
    /// 价格偏差过大
    PriceDeviationExceeded,
    /// 交易对不可交易
    SymbolNotTradable,
    /// 订单频率超限
    OrderFrequencyExceeded,
    /// 系统错误
    SystemError,
    /// 未知错误
    Unknown,
}

// ============================================================================
// OrderResult
// ============================================================================

/// 订单结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderResult {
    /// 订单ID
    pub order_id: String,
    /// 订单状态
    pub status: String,
    /// 成交数量
    pub filled_qty: Decimal,
    /// 成交价格
    pub filled_price: Decimal,
    /// 手续费
    pub commission: Decimal,
    /// 拒绝原因
    pub reject_reason: Option<OrderRejectReason>,
    /// 消息
    pub message: String,
}

// ============================================================================
// OrderRecord
// ============================================================================

/// 订单记录（用于持久化和恢复）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderRecord {
    /// 订单ID
    pub order_id: String,
    /// 交易品种
    pub symbol: String,
    /// 订单方向
    pub side: String,
    /// 订单数量
    pub qty: String,
    /// 订单价格
    pub price: String,
    /// 订单状态
    pub status: String,
    /// 创建时间
    pub created_at: String,
    /// 成交时间
    pub filled_at: Option<String>,
}
