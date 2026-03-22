#![forbid(unsafe_code)]

pub mod error;
pub mod traits;
pub mod types;
pub mod pin_strategy;
pub mod trend_strategy;

pub use error::StrategyError;
pub use traits::{MinuteIndicators, Strategy};
pub use types::{OrderRequest, Side, Signal};
