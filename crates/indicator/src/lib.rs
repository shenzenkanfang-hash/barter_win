#![forbid(unsafe_code)]

pub mod types;

pub mod indicator_1d;
pub mod indicator_1m;
pub mod pine_indicator_full;

pub mod min;
pub mod day;

pub mod trading_trigger;

pub use indicator_1d::{BigCycleCalculator, BigCycleConfig, BigCycleIndicators, PineColorBig, TRRatioSignal};
pub use indicator_1m::{Indicator1m, Indicator1mOutput};
pub use pine_indicator_full::{PineColorDetector as PineColorDetectorV5, colors, DominantCycleRSI, EMA, RMA};
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
};
