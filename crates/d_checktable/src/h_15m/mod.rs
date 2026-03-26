//! h_15m - 分钟级策略
//!
//! 交易容器框架，对标 Python 版本的 singleAssetTrader。

#![forbid(unsafe_code)]

pub mod trader;

pub use trader::{Trader, Status, Config, HealthCheck, RuntimeInfo};
