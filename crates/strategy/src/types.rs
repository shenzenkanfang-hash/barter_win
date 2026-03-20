use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradingMode {
    Low,
    Medium,
    High,
}

/// 策略信号 (简单枚举，用于内部传递)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Signal {
    LongEntry,
    ShortEntry,
    LongHedge,
    ShortHedge,
    LongExit,
    ShortExit,
    ExitHighVol,
}

/// 交易动作 (用于 TradingDecision)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradingAction {
    OpenLong,
    OpenShort,
    CloseLong,
    CloseShort,
    NoAction,
}

/// 交易决策 (包含完整交易信息)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingDecision {
    pub action: TradingAction,
    pub symbol: String,
    pub price: Decimal,
    pub qty: Decimal,
    pub reason: String,
}

impl TradingDecision {
    /// 创建做多决策
    pub fn open_long(symbol: String, price: Decimal, qty: Decimal, reason: String) -> Self {
        Self {
            action: TradingAction::OpenLong,
            symbol,
            price,
            qty,
            reason,
        }
    }

    /// 创建做空决策
    pub fn open_short(symbol: String, price: Decimal, qty: Decimal, reason: String) -> Self {
        Self {
            action: TradingAction::OpenShort,
            symbol,
            price,
            qty,
            reason,
        }
    }

    /// 创建平多决策
    pub fn close_long(symbol: String, price: Decimal, reason: String) -> Self {
        Self {
            action: TradingAction::CloseLong,
            symbol,
            price,
            qty: Decimal::ZERO,
            reason,
        }
    }

    /// 创建平空决策
    pub fn close_short(symbol: String, price: Decimal, reason: String) -> Self {
        Self {
            action: TradingAction::CloseShort,
            symbol,
            price,
            qty: Decimal::ZERO,
            reason,
        }
    }

    /// 创建无操作决策
    pub fn no_action(symbol: String, reason: String) -> Self {
        Self {
            action: TradingAction::NoAction,
            symbol,
            price: Decimal::ZERO,
            qty: Decimal::ZERO,
            reason,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    Long,
    Short,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderType {
    Market,
    Limit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderRequest {
    pub symbol: String,
    pub side: Side,
    pub order_type: OrderType,
    pub qty: Decimal,
    pub price: Option<Decimal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyId(pub String);
