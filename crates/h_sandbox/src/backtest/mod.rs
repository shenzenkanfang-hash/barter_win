//! 回测模块 - 异步回测引擎
//!
//! 基于 TickGenerator + DataFeeder 的异步回测框架

// mod engine;  // TODO: 后续实现
// mod loader;   // TODO: parquet API 兼容性问题待修复

mod strategy;

pub use strategy::{BacktestStrategy, BacktestTick, MaCrossStrategy, Signal, BacktestOrder, BacktestFill};
