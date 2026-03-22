//! Exchange Gateway Types - 交易所网关数据类型
//!
//! 提供交易所网关使用的纯数据类型，不包含业务逻辑。

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// 交易所账户信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeAccount {
    pub account_id: String,
    pub total_equity: Decimal,
    pub available: Decimal,
    pub frozen_margin: Decimal,
    pub unrealized_pnl: Decimal,
    pub update_ts: i64,
}

impl ExchangeAccount {
    pub fn new(account_id: String, initial_balance: Decimal) -> Self {
        Self {
            account_id,
            total_equity: initial_balance,
            available: initial_balance,
            frozen_margin: Decimal::ZERO,
            unrealized_pnl: Decimal::ZERO,
            update_ts: 0,
        }
    }

    pub fn margin_ratio(&self) -> Decimal {
        if self.total_equity.is_zero() {
            return Decimal::ZERO;
        }
        self.frozen_margin / self.total_equity
    }
}

/// 交易所持仓方向
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PositionDirection {
    Long,
    Short,
    Both,
    None,
}

/// 交易所持仓信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangePosition {
    pub symbol: String,
    pub long_qty: Decimal,
    pub long_avg_price: Decimal,
    pub short_qty: Decimal,
    pub short_avg_price: Decimal,
    pub unrealized_pnl: Decimal,
    pub margin_used: Decimal,
}

impl ExchangePosition {
    pub fn new(symbol: String) -> Self {
        Self {
            symbol,
            long_qty: Decimal::ZERO,
            long_avg_price: Decimal::ZERO,
            short_qty: Decimal::ZERO,
            short_avg_price: Decimal::ZERO,
            unrealized_pnl: Decimal::ZERO,
            margin_used: Decimal::ZERO,
        }
    }

    pub fn total_qty(&self) -> Decimal {
        self.long_qty + self.short_qty
    }

    pub fn net_direction(&self) -> PositionDirection {
        let has_long = self.long_qty > Decimal::ZERO;
        let has_short = self.short_qty > Decimal::ZERO;

        match (has_long, has_short) {
            (true, false) => PositionDirection::Long,
            (false, true) => PositionDirection::Short,
            (true, true) => PositionDirection::Both,
            (false, false) => PositionDirection::None,
        }
    }
}

/// 订单状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderStatus {
    Pending,
    Filled,
    Cancelled,
    Rejected,
}

/// 订单拒绝原因
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum RejectReason {
    InsufficientBalance,
    PositionLimitExceeded,
    MarginInsufficient,
    PriceDeviationExceeded,
    SymbolNotTradable,
    OrderFrequencyExceeded,
    SystemError,
}

impl std::fmt::Display for RejectReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RejectReason::InsufficientBalance => write!(f, "INSUFFICIENT_BALANCE"),
            RejectReason::PositionLimitExceeded => write!(f, "POSITION_LIMIT_EXCEEDED"),
            RejectReason::MarginInsufficient => write!(f, "MARGIN_INSUFFICIENT"),
            RejectReason::PriceDeviationExceeded => write!(f, "PRICE_DEVIATION_EXCEEDED"),
            RejectReason::SymbolNotTradable => write!(f, "SYMBOL_NOT_TRADABLE"),
            RejectReason::OrderFrequencyExceeded => write!(f, "ORDER_FREQUENCY_EXCEEDED"),
            RejectReason::SystemError => write!(f, "SYSTEM_ERROR"),
        }
    }
}

/// 订单结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderResult {
    pub order_id: String,
    pub status: OrderStatus,
    pub filled_qty: Decimal,
    pub filled_price: Decimal,
    pub commission: Decimal,
    pub reject_reason: Option<RejectReason>,
    pub message: String,
}
