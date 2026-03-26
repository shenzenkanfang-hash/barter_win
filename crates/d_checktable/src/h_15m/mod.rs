//! h_15m - 交易容器框架
//!
//! 纯框架，无业务逻辑

#![forbid(unsafe_code)]

pub mod trader;

pub use trader::{Trader, Status, Config, TraderHealth, run_loop, DataFn, OrderFn};
