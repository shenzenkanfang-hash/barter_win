//! trading - 交易数据类型

pub mod rules;
pub mod order;
pub mod futures;

pub use rules::{SymbolRulesData, ParsedSymbolRules};
pub use order::{OrderRejectReason, OrderResult, OrderRecord};
pub use futures::{FuturesPosition, FuturesAccount};
