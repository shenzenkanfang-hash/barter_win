//! 回测模块 - 异步回测引擎
//!
//! 基于 TickGenerator + DataFeeder 的异步回测框架
//! 支持从 parquet 文件读取历史 K线数据，生成模拟 Tick 流进行回测

mod engine;
mod loader;
mod strategy;

pub use engine::{BacktestEngine, BacktestConfig, BacktestResult};
pub use loader::ParquetLoader;
pub use strategy::{BacktestStrategy, Signal};
