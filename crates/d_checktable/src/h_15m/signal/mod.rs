//! 信号生成模块
//!
//! 提供分钟级和日线级的市场状态、信号、价格控制生成

#![forbid(unsafe_code)]

pub mod signal_generator;
pub mod price_control_generator;

pub use signal_generator::MinSignalGenerator;
pub use price_control_generator::MinPriceControlGenerator;
