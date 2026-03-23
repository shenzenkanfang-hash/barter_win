#![forbid(unsafe_code)]

pub mod types;

pub mod pine_indicator_full;

pub mod min;
pub mod day;
pub mod processor;

pub use pine_indicator_full::{PineColorDetector as PineColorDetectorV5, colors, EMA, RSI};
pub use types::*;
pub use processor::SignalProcessor;
