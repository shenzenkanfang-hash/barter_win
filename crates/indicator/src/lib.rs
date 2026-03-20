#![forbid(unsafe_code)]

pub mod ema;
pub mod error;
pub mod pine_color;
pub mod price_position;
pub mod rsi;
pub mod tr_ratio;
pub mod velocity;
pub mod z_score;

pub use ema::EMA;
pub use error::IndicatorError;
pub use pine_color::{PineColor, PineColorDetector};
pub use price_position::PricePosition;
pub use rsi::RSI;
pub use tr_ratio::{TRRatio, TRRatioSignal, analyze_tr_ratio, windows as tr_windows};
pub use velocity::{Momentum, PriceDeviation, VelocityPercentile};
pub use z_score::{ZScore, ZScoreSignal, ZScoreThreshold, analyze_zscore};
