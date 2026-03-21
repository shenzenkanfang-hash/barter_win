#![forbid(unsafe_code)]

pub mod market_status_generator;
pub mod signal_generator;
pub mod price_control_generator;
pub mod indicator_1d;

pub use market_status_generator::DayMarketStatusGenerator;
pub use signal_generator::DaySignalGenerator;
pub use price_control_generator::DayPriceControlGenerator;
pub use indicator_1d::{BigCycleCalculator, BigCycleIndicators, PineColorBig, TRRatioSignal};