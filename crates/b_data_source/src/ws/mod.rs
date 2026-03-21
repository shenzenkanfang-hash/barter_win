#![forbid(unsafe_code)]

pub mod binance_ws;
pub mod websocket;
pub mod data_feeder;

pub use binance_ws::{BinanceTradeStream, BinanceWsConnector};
pub use websocket::{MarketConnector, MarketStream, MockMarketConnector, MockMarketStream};
pub use data_feeder::{DataFeeder, DataMessage, MarketDataFeeder};
