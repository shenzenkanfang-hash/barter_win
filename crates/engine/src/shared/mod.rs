pub mod account_pool;
pub mod check_table;
pub mod checkpoint;
pub mod checkpoint_integration;
pub mod error;
pub mod margin_config;
pub mod market_status;
pub mod platform;
pub mod pnl_manager;
pub mod round_guard;
pub mod symbol_rules;
pub mod symbol_rules_fetcher;
pub mod telegram_notifier;

// Re-export commonly used types
pub use account_pool::{AccountInfo, AccountMargin, AccountPool, CircuitBreakerState};
pub use margin_config::{GlobalMarginConfig, MarginPoolConfig, MinuteOpenConfig, StrategyLevel};
pub use checkpoint::{CheckpointLogger, CompositeCheckpointLogger, ConsoleCheckpointLogger, Stage, StageResult, TracingCheckpointLogger};
pub use check_table::{CheckEntry, CheckTable};
pub use error::EngineError;
pub use market_status::{MarketStatus, MarketStatusDetector, PinIntensity, PinDetection};
pub use platform::{Platform, Paths};
pub use pnl_manager::PnlManager;
pub use round_guard::{RoundGuard, RoundGuardScope};
pub use symbol_rules::SymbolRules;
pub use symbol_rules_fetcher::SymbolRulesFetcher;
pub use telegram_notifier::{TelegramConfig, TelegramNotifier};
