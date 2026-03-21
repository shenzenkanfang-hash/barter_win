#![forbid(unsafe_code)]

pub mod error;
pub mod pin_strategy;
pub mod traits;
pub mod trend_strategy;
pub mod types;

pub use error::StrategyError;
pub use pin_strategy::{PinSignal, PinState, PinStrategy, PinStrategyConfig};
pub use traits::Strategy;
pub use trend_strategy::{TrendSignal, TrendState, TrendStrategy, TrendStrategyConfig};
pub use types::{OrderRequest, OrderType, Side, Signal, StrategyId, TradingMode, TradingAction, TradingDecision};
