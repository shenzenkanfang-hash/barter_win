#![forbid(unsafe_code)]

pub mod error;
pub mod platform;
pub mod check_table;
pub mod checkpoint;
pub mod checkpoint_integration;
pub mod telegram_notifier;
pub mod mock_binance_gateway;
pub mod types;

// Re-exports
pub use error::EngineError;
pub use platform::{Platform, Paths};
pub use check_table::{CheckEntry, CheckTable};
pub use checkpoint::{CheckpointLogger, CompositeCheckpointLogger, ConsoleCheckpointLogger, Stage, StageResult, TracingCheckpointLogger};
pub use checkpoint_integration::CheckpointIntegration;
pub use telegram_notifier::{TelegramConfig, TelegramNotifier};
pub use mock_binance_gateway::{MockBinanceGateway, MockAccount, MockPosition, MockOrder, MockTrade, ChannelState, GatewayChannelType, ExitSignal, OrderResult, OrderStatus, PineColorState, RejectReason, RiskConfig, SignalSynthesisLayer, TriggerLogEntry};
pub use types::*;
