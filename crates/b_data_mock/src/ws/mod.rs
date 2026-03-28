//! WebSocket 数据接口层
//!
//! 与 b_data_source::ws 对齐，但使用模拟数据源

pub mod kline_1m;
pub mod kline_1d;
pub mod order_books;
pub mod kline_generator;
pub mod noise;
pub mod volatility;

pub use kline_1m::{Kline1mStream, KLineSynthesizer, KlineData};
pub use kline_1d::Kline1dStream;
pub use order_books::{OrderBook, DepthStream, DepthData};
pub use volatility::{VolatilityManager, SymbolVolatility};
pub use a_common::volatility::VolatilityEntry;
