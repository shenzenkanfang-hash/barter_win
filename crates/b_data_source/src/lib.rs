#![forbid(unsafe_code)]

pub mod api;
pub mod ws;
pub mod kline;
pub mod registry;

pub mod error;
pub mod orderbook;
pub mod volatility;
pub mod recovery;
pub mod types;

pub use api::{BinanceApiGateway, RateLimiter, SymbolRulesFetcher, SymbolRulesData};
pub use api::{BinanceExchangeInfo, BinanceSymbol, PositionRisk, LeverageBracket};
pub use api::{BinanceAccountInfo, BinancePositionRisk, BinanceLeverageBracket};

pub use ws::{BinanceTradeStream, BinanceWsConnector};
pub use ws::{MarketConnector, MarketStream, MockMarketConnector, MockMarketStream};
pub use ws::{DataFeeder, DataMessage, MarketDataFeeder};

pub use kline::{KLineSynthesizer, KlinePersistence};

pub use registry::SymbolRegistry;

pub use orderbook::OrderBook;
pub use volatility::VolatilityDetector;
pub use recovery::{CheckpointData, CheckpointManager, RedisRecovery};
pub use types::{KLine, Period, Tick, VolatilityStats};
pub use error::MarketError;
