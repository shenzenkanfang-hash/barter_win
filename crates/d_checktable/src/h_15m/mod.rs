//! h_15m - 分钟级策略
//!
//! 两个核心文件：
//! - indicator.rs  指标计算 + 信号生成
//! - trader.rs     主交易逻辑

#![forbid(unsafe_code)]

pub mod indicator;
pub mod trader;

pub use indicator::{Indicator, Signal, MarketData, PositionData, MarketStatus, config};
pub use trader::{Trader, Status, TraderHealth};
