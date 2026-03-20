#![forbid(unsafe_code)]

pub mod big_cycle;
pub mod ema;
pub mod error;
pub mod pine_color;
pub mod pine_indicator_full;
pub mod price_position;
pub mod rsi;
pub mod tr_ratio;
pub mod velocity;
pub mod z_score;

pub use big_cycle::{BigCycleCalculator, BigCycleConfig, BigCycleIndicators, PineColorBig, TRRatioSignal};
pub use ema::EMA;
pub use error::IndicatorError;
pub use pine_color::{PineColor, PineColorDetector};
pub use pine_indicator_full::{PineColorDetector as PineColorDetectorV5, colors};
pub use price_position::PricePosition;
pub use rsi::RSI;
pub use tr_ratio::{TRRatio, TRRatioSignal as TRRatioSignalSmall, analyze_tr_ratio, windows as tr_windows};
pub use velocity::{Momentum, PriceDeviation, VelocityPercentile};
pub use z_score::{ZScore, ZScoreSignal, ZScoreThreshold, analyze_zscore};
