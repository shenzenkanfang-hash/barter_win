#![forbid(unsafe_code)]

pub mod binance_ws;
pub mod data_feeder;
pub mod error;
pub mod kline;
pub mod kline_persistence;
pub mod orderbook;
pub mod recovery;
pub mod symbol_registry;
pub mod types;
pub mod volatility;
pub mod websocket;

pub use binance_ws::{BinanceTradeStream, BinanceWsConnector};
pub use data_feeder::{DataFeeder, DataMessage, MarketDataFeeder};
pub use error::MarketError;
pub use kline::KLineSynthesizer;
pub use kline_persistence::KlinePersistence;
pub use orderbook::OrderBook;
pub use recovery::{CheckpointData, CheckpointManager, RedisRecovery};
pub use symbol_registry::SymbolRegistry;
pub use types::{KLine, Period, Tick, VolatilityStats};
pub use volatility::VolatilityDetector;
pub use websocket::{MarketConnector, MarketStream, MockMarketConnector, MockMarketStream};
