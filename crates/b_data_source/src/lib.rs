#![forbid(unsafe_code)]

//! b_data_source - 业务数据层
//!
//! 提供市场数据处理功能：数据订阅、K线合成、订单簿、波动率检测等。

// Re-exports from a_common (API and WS gateways)
pub use a_common::api::{BinanceApiGateway, RateLimiter, SymbolRulesFetcher, SymbolRulesData};
pub use a_common::api::{BinanceExchangeInfo, BinanceSymbol, PositionRisk, LeverageBracket};
pub use a_common::api::{BinanceAccountInfo, BinancePositionRisk, BinanceLeverageBracket};
pub use a_common::api::{FuturesAccountResponse, FuturesAsset, FuturesPositionResponse};
pub use a_common::ws::{BinanceTradeStream, BinanceCombinedStream, BinanceWsConnector, BinanceTradeMsg, BinanceKlineMsg, BinanceDepthMsg};
pub use a_common::ws::{MarketConnector, MockMarketConnector};
pub use a_common::MarketError;
pub use a_common::config::{Platform, Paths};
pub use a_common::logs::{CheckpointLogger, CompositeCheckpointLogger, ConsoleCheckpointLogger, Stage, StageResult, TracingCheckpointLogger};

// Sub-modules
pub mod kline_1m;
pub mod kline_1d;
pub mod order_books;
pub mod symbol_rules;
pub mod recovery;

// Futures data modules
pub mod futures;

// Organized modules
pub mod models;

// Re-exports - Models (业务数据类型)
pub use models::{MarketStream, MockMarketStream};
pub use models::{KLine, Period, Tick};

// Re-exports - Data processing
pub use kline_1m::{KLineSynthesizer, KlinePersistence, Kline1mStream};
pub use kline_1d::Kline1dStream;
pub use symbol_rules::SymbolRegistry;
pub use order_books::{OrderBook, DepthStream};
pub use recovery::{CheckpointData, CheckpointManager, RedisRecovery};

// Re-exports - Futures data
pub use futures::{FuturesAccount, FuturesAccountData, FuturesPosition, FuturesPositionData};

#[cfg(test)]
pub mod tests;
