//! 订单簿数据类型

#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

// ============================================================================
// DepthData
// ============================================================================

/// 深度数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepthData {
    /// 交易品种
    pub symbol: String,
    /// 更新时间戳
    pub update_id: i64,
    /// 买方深度（价格 -> 数量）
    pub bids: Vec<(Decimal, Decimal)>,
    /// 卖方深度（价格 -> 数量）
    pub asks: Vec<(Decimal, Decimal)>,
}

// ============================================================================
// OrderBook
// ============================================================================

/// 订单簿
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBook {
    /// 交易品种
    pub symbol: String,
    /// 最新深度
    pub depth: DepthData,
}

/// 订单簿层级
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookLevel {
    /// 价格
    pub price: Decimal,
    /// 数量
    pub qty: Decimal,
}

/// 订单簿快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookSnapshot {
    /// 交易品种
    pub symbol: String,
    /// 买方深度
    pub bids: Vec<OrderBookLevel>,
    /// 卖方深度
    pub asks: Vec<OrderBookLevel>,
    /// 快照时间
    pub ts: i64,
}
