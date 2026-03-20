#![forbid(unsafe_code)]

pub mod binance_ws;
pub mod error;
pub mod kline;
pub mod orderbook;
pub mod types;
pub mod websocket;

pub use binance_ws::{BinanceTradeStream, BinanceWsConnector};
pub use error::MarketError;
pub use kline::KLineSynthesizer;
pub use orderbook::OrderBook;
pub use types::{KLine, Period, Tick};
pub use websocket::{MarketConnector, MarketStream, MockMarketConnector, MockMarketStream};
