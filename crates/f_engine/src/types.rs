//! f_engine 核心类型定义
//!
//! # 模块划分
//! - `StrategyId` - 策略标识符
//! - `TradingDecision` - 交易决策
//! - `OrderRequest` - 订单请求
//! - `Side`, `OrderType` - 来自 a_common 的类型重导出
//! - `Mode`, `ModeSwitcher` - 移至 `channel` 模块

#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use c_data_process::types::TradingAction;

// ============================================================================
// 策略标识符
// ============================================================================

/// 策略 ID
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StrategyId(pub String);

impl StrategyId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl Default for StrategyId {
    fn default() -> Self {
        Self("main".to_string())
    }
}

impl std::fmt::Display for StrategyId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ============================================================================
// 交易决策
// ============================================================================

/// 交易决策 (来自 c_data_process::types::TradingDecision)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingDecision {
    pub action: TradingAction,
    pub reason: String,
    pub confidence: u8,
    pub symbol: String,
    pub qty: Decimal,
    pub price: Decimal,
}

impl TradingDecision {
    pub fn new(
        action: TradingAction,
        reason: impl Into<String>,
        confidence: u8,
        symbol: String,
        qty: Decimal,
        price: Decimal,
    ) -> Self {
        Self {
            action,
            reason: reason.into(),
            confidence,
            symbol,
            qty,
            price,
        }
    }

    pub fn is_exit(&self) -> bool {
        matches!(self.action, TradingAction::Flat)
    }

    pub fn is_entry(&self) -> bool {
        matches!(self.action, TradingAction::Long | TradingAction::Short)
    }
}

// ============================================================================
// 订单相关类型 (来自 a_common)
// ============================================================================

/// Side 用于订单方向
pub use a_common::models::types::Side;

/// OrderType 订单类型
pub use a_common::models::types::OrderType;

/// OrderRequest 订单请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderRequest {
    pub symbol: String,
    pub side: Side,
    pub order_type: OrderType,
    pub qty: Decimal,
    pub price: Option<Decimal>,
}

impl OrderRequest {
    pub fn new_market(symbol: String, side: Side, qty: Decimal) -> Self {
        Self {
            symbol,
            side,
            order_type: OrderType::Market,
            qty,
            price: None,
        }
    }

    pub fn new_limit(symbol: String, side: Side, qty: Decimal, price: Decimal) -> Self {
        Self {
            symbol,
            side,
            order_type: OrderType::Limit,
            qty,
            price: Some(price),
        }
    }
}

// ============================================================================
// 通道模块类型重导出
// ============================================================================

/// 交易模式
pub use crate::channel::Mode;

/// 模式切换器
pub use crate::channel::ModeSwitcher;
