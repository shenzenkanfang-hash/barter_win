#![forbid(unsafe_code)]

pub mod api;
pub mod error;
pub mod platform;
pub mod checkpoint;
// pub mod checkpoint_integration; // TODO: 移到 e_strategy (依赖 channel 类型)
pub mod telegram_notifier;
pub mod types;
pub mod ws;

// Re-exports
pub use api::{BinanceApiGateway, RateLimiter, SymbolRulesFetcher, SymbolRulesData};
pub use api::{BinanceExchangeInfo, BinanceSymbol, PositionRisk, LeverageBracket};
pub use api::{BinanceAccountInfo, BinancePositionRisk, BinanceLeverageBracket};

pub use error::EngineError;
pub use platform::{Platform, Paths};
pub use checkpoint::{CheckpointLogger, CompositeCheckpointLogger, ConsoleCheckpointLogger, Stage, StageResult, TracingCheckpointLogger};
// pub use checkpoint_integration::CheckpointIntegration; // TODO: 移到 e_strategy
pub use telegram_notifier::{TelegramConfig, TelegramNotifier};
pub use types::*;

pub use ws::{BinanceTradeStream, BinanceWsConnector};
pub use ws::{MarketConnector, MockMarketConnector};
