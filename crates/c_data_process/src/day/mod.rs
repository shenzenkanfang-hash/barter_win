#![forbid(unsafe_code)]

// 趋势指标（历史趋势指标，日线级触发）
pub mod trend;

pub use trend::{BigCycleCalculator, BigCycleIndicators, PineColorBig, TRRatioSignal};
