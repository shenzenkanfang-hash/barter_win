//! 信号生成模块
//!
//! 提供日线级的信号生成

#![forbid(unsafe_code)]

pub mod signal_generator;
pub mod price_control_generator;

pub use signal_generator::DaySignalGenerator;
pub use price_control_generator::DayPriceControlGenerator;
