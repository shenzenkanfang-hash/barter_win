//! WebSocket 数据接口层
//!
//! 封装所有 WebSocket 数据获取接口
//! 其他模块只能通过这里获取 WS 数据

pub mod kline_1m;
pub mod kline_1d;
pub mod order_books;
pub mod volatility;

pub use kline_1m::{Kline1mStream, KLineSynthesizer, KlinePersistence};
pub use kline_1d::Kline1dStream;
pub use order_books::{OrderBook, DepthStream};
pub use volatility::{VolatilityManager, SymbolVolatility};
// Re-export from a_common
pub use a_common::volatility::VolatilityEntry;
