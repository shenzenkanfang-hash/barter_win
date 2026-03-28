#![forbid(unsafe_code)]

//! 1分钟K线 WebSocket 模拟模块
//!
//! 与 b_data_source::ws::kline_1m 对齐，但使用模拟数据源

pub mod kline;
pub mod ws;

pub use kline::KLineSynthesizer;
pub use ws::{Kline1mStream, KlineData};
