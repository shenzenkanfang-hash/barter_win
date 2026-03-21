#![forbid(unsafe_code)]

pub mod error;
pub mod platform;
pub mod checkpoint;
pub mod symbol_rules_fetcher;
// pub mod checkpoint_integration; // TODO: 移到 e_strategy (依赖 channel 类型)
pub mod telegram_notifier;
pub mod types;

// Re-exports
pub use error::EngineError;
pub use platform::{Platform, Paths};
pub use checkpoint::{CheckpointLogger, CompositeCheckpointLogger, ConsoleCheckpointLogger, Stage, StageResult, TracingCheckpointLogger};
pub use symbol_rules_fetcher::{SymbolRulesFetcher, SymbolRulesData, BinanceExchangeInfo, BinanceSymbol, PositionRisk, LeverageBracket};
// pub use checkpoint_integration::CheckpointIntegration; // TODO: 移到 e_strategy
pub use telegram_notifier::{TelegramConfig, TelegramNotifier};
pub use types::*;
