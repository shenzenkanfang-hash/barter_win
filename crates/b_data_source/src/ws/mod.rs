//! WebSocket 数据接口层
//!
//! 封装所有 WebSocket 数据获取接口
//! 其他模块只能通过这里获取 WS 数据，不能直接访问 a_common::ws

pub use crate::kline_1m::Kline1mStream;
pub use crate::kline_1d::Kline1dStream;
pub use crate::order_books::DepthStream;
