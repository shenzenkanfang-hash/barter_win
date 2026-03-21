#![forbid(unsafe_code)]

// Re-exports from a_common (API and WS gateways)
pub use a_common::api::{BinanceApiGateway, RateLimiter, SymbolRulesFetcher, SymbolRulesData};
pub use a_common::api::{BinanceExchangeInfo, BinanceSymbol, PositionRisk, LeverageBracket};
pub use a_common::api::{BinanceAccountInfo, BinancePositionRisk, BinanceLeverageBracket};
pub use a_common::ws::{BinanceTradeStream, BinanceWsConnector, BinanceTradeMsg, BinanceKlineMsg, BinanceDepthMsg};
pub use a_common::ws::{MarketConnector, MockMarketConnector};
pub use a_common::error::MarketError;

// WebSocket market stream (depends on Tick)
pub mod ws;
pub use ws::{MarketStream, MockMarketStream};

// Data processing modules
pub mod data_feeder;
pub mod kline_1m;
pub mod kline_1d;
pub mod order_books;
pub mod symbol_rules;

pub mod error;
pub mod volatility;
pub mod recovery;
pub mod types;

// Re-exports
pub use data_feeder::{DataFeeder, DataMessage, MarketDataFeeder};
pub use kline_1m::{KLineSynthesizer, KlinePersistence};
pub use symbol_rules::SymbolRegistry;
pub use order_books::OrderBook;
pub use volatility::VolatilityDetector;
pub use recovery::{CheckpointData, CheckpointManager, RedisRecovery};
pub use types::{KLine, Period, Tick, VolatilityStats};
