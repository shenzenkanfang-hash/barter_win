//! trading - 交易数据类型

pub mod rules;
pub mod order;
pub mod futures;
pub mod signal;

pub use rules::{SymbolRulesData, ParsedSymbolRules};
pub use order::{OrderRejectReason, OrderResult, OrderRecord};
pub use futures::{FuturesPosition, FuturesAccount};
pub use signal::{StrategySignal, TradeCommand, StrategyId, StrategyType, StrategyLevel, PositionRef};
