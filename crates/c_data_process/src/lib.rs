#![forbid(unsafe_code)]

pub mod types;

pub mod pine_indicator_full;

pub mod min;
pub mod day;

pub mod trading_trigger;

pub mod volatility_rank;

pub mod volatility;

pub use volatility_rank::{VolatilityEntry, VolatilityRank};
pub use pine_indicator_full::{PineColorDetector as PineColorDetectorV5, colors, EMA, RSI};
pub use volatility::VolatilityStats;
pub use types::*;
pub use trading_trigger::TradingTrigger;

// Re-export generators
pub use min::{
    MinMarketStatusGenerator,
    MinSignalGenerator,
    MinPriceControlGenerator,
};
pub use day::{
    DayMarketStatusGenerator,
    DaySignalGenerator,
    DayPriceControlGenerator,
    BigCycleCalculator,
    BigCycleIndicators,
};
