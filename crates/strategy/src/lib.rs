#![forbid(unsafe_code)]

pub mod error;
pub mod traits;
pub mod types;

pub use error::StrategyError;
pub use traits::Strategy;
pub use types::{OrderRequest, OrderType, Side, Signal, StrategyId, TradingMode};
