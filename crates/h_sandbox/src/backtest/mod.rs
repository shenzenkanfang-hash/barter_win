//! 回测模块 - 异步回测引擎
//!
//! 基于 TickGenerator + DataFeeder 的异步回测框架
//! 数据源支持: CSV replay (b_data_source/replay_source.rs)

mod strategy;

pub use strategy::{BacktestStrategy, BacktestTick, MaCrossStrategy, Signal, BacktestOrder, BacktestFill};
