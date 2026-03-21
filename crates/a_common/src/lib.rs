#![forbid(unsafe_code)]

//! a_common - 基础设施层
//!
//! 提供 API/WS 网关、配置、通用错误、数据模型等基础设施组件。

// Core modules
pub mod api;
pub mod ws;
pub mod config;
pub mod logs;
pub mod models;
pub mod claint;
pub mod util;

// Re-exports - API
pub use api::{BinanceApiGateway, RateLimiter, SymbolRulesFetcher, SymbolRulesData};
pub use api::{BinanceExchangeInfo, BinanceSymbol, PositionRisk, LeverageBracket};
pub use api::{BinanceAccountInfo, BinancePositionRisk, BinanceLeverageBracket};
pub use api::{FuturesAccountResponse, FuturesAsset, FuturesPositionResponse};

// Re-exports - Config
pub use config::{Platform, Paths};

// Re-exports - Logs
pub use logs::{CheckpointLogger, CompositeCheckpointLogger, ConsoleCheckpointLogger, Stage, StageResult, TracingCheckpointLogger};

// Re-exports - Models
pub use models::*;

// Re-exports - Claint (errors)
pub use claint::{EngineError, MarketError};

// Re-exports - Util
pub use util::{TelegramConfig, TelegramNotifier};

// Re-exports - WS
pub use ws::{BinanceTradeStream, BinanceWsConnector};
pub use ws::{MarketConnector, MockMarketConnector};
