#![forbid(unsafe_code)]

//! 订单簿模块 - 20档深度 + WebSocket 订阅

pub mod orderbook;
pub mod ws;

pub use orderbook::OrderBook;
pub use ws::{DepthStream, DepthData};
