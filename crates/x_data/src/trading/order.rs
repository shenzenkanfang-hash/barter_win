//! 订单数据类型

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

// ============================================================================
// OrderRejectReason
// ============================================================================

/// 订单拒绝原因
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderRejectReason {
    /// 资金不足
    InsufficientMargin,
    /// 超出持仓限制
    PositionLimitExceeded,
    /// 价格超出限制
    PriceOutOfRange,
    /// 数量超出限制
    QtyOutOfRange,
    /// 风控拒绝
    RiskRejected,
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
    /// 交易品种
    pub symbol: String,
    /// 订单方向
    pub side: String,
    /// 订单价格
    pub price: String,
    /// 订单数量
    pub qty: String,
    /// 订单状态
    pub status: String,
    /// 成交数量
    pub filled_qty: String,
    /// 拒绝原因
    pub reject_reason: Option<String>,
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
