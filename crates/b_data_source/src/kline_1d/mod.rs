#![forbid(unsafe_code)]

//! 1天K线 WebSocket 订阅模块

pub mod ws;

pub use ws::Kline1dStream;
