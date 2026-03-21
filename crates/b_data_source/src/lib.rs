#![forbid(unsafe_code)]

pub mod api;
pub mod ws;
pub mod data_feeder;
pub mod kline_1m;
pub mod kline_1d;
pub mod order_books;
pub mod symbol_rules;

pub mod error;
pub mod volatility;
pub mod recovery;
pub mod types;

pub use api::{BinanceApiGateway, RateLimiter, SymbolRulesFetcher, SymbolRulesData};
pub use api::{BinanceExchangeInfo, BinanceSymbol, PositionRisk, LeverageBracket};
pub use api::{BinanceAccountInfo, BinancePositionRisk, BinanceLeverageBracket};

pub use ws::{BinanceTradeStream, BinanceWsConnector};
pub use ws::{MarketConnector, MarketStream, MockMarketConnector, MockMarketStream};

pub use data_feeder::{DataFeeder, DataMessage, MarketDataFeeder};

pub use kline_1m::{KLineSynthesizer, KlinePersistence};

pub use symbol_rules::SymbolRegistry;

pub use order_books::OrderBook;
pub use volatility::VolatilityDetector;
pub use recovery::{CheckpointData, CheckpointManager, RedisRecovery};
pub use types::{KLine, Period, Tick, VolatilityStats};
pub use error::MarketError;
