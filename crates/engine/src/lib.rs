#![forbid(unsafe_code)]

pub mod core;
pub mod risk;
pub mod order;
pub mod position;
pub mod persistence;
pub mod channel;
pub mod shared;

// Re-exports from submodules
pub use core::{engine::TradingEngine, pipeline::{Pipeline, Processor, MockIndicatorProcessor, MockStrategyProcessor, MockRiskProcessor}, pipeline_form::PipelineForm, strategy_pool::{StrategyAllocation, StrategyPool}};
pub use shared::{account_pool::{AccountInfo, AccountMargin, AccountPool, CircuitBreakerState}, margin_config::{GlobalMarginConfig, MarginPoolConfig, MinuteOpenConfig, StrategyLevel}, checkpoint::{CheckpointLogger, CompositeCheckpointLogger, ConsoleCheckpointLogger, Stage, StageResult, TracingCheckpointLogger}, check_table::{CheckEntry, CheckTable}, error::EngineError, market_status::{MarketStatus, MarketStatusDetector, PinIntensity, PinDetection}, platform::{Platform, Paths}, pnl_manager::PnlManager, round_guard::{RoundGuard, RoundGuardScope}, symbol_rules::SymbolRules, symbol_rules_fetcher::SymbolRulesFetcher, telegram_notifier::{TelegramConfig, TelegramNotifier}};
pub use risk::{risk::{RiskPreChecker, VolatilityMode}, risk_rechecker::RiskReChecker, order_check::{OrderCheck, OrderCheckResult, OrderReservation}, thresholds::ThresholdConstants, minute_risk::{calculate_hour_open_notional, calculate_minute_open_notional, calculate_open_qty_from_notional, MinuteOpenResult}};
pub use order::{gateway::ExchangeGateway, order::OrderExecutor, mock_binance_gateway::{ChannelState, GatewayChannelType as MockChannelType, ExitSignal, MockAccount, MockBinanceGateway, MockOrder, MockPosition, MockTrade, OrderResult, OrderStatus, PineColorState, RejectReason, RiskConfig, SignalSynthesisLayer, TriggerLogEntry}};
pub use position::{position_manager::{Direction, LocalPosition, LocalPositionManager, PositionStats}, position_exclusion::{PositionDirection, PositionExclusionChecker, PositionInfo}};
pub use persistence::{memory_backup::{AccountSnapshot as MemoryAccountSnapshot, DepthData, DepthEntry, IndicatorsData, KxianCache, KxianData, KlineEntry, MemoryBackup, OrderSnapshot as MemoryOrderSnapshot, PositionSnapshot as MemoryPositionSnapshot, RealtimeTradeEntry, RealtimeTradesData, SymbolRulesData, TradeSnapshot as MemoryTradeSnapshot}, persistence::{KLineCache, KLineData, PersistenceConfig, PersistenceService, PersistenceStats, PositionSnapshot, TradeRecord}, sqlite_persistence::{AccountSnapshotRecord, ChannelEventRecord, EventRecorder, ExchangePositionRecord, IndicatorCsvWriter, IndicatorComparisonRow, IndicatorEventRecord, LocalPositionRecord, NoOpEventRecorder, RiskEventRecord, SqliteEventRecorder, SqliteRecordService, format_decimal, OrderRecord, SyncLogRecord}, disaster_recovery::{AccountSnapshot, ApiPositionSnapshot, DisasterRecovery, LocalPositionSnapshot, OrderSnapshot, RecoveryData, SyncLogEntry, VerificationResult}};
pub use channel::{channel::{ChannelCheckpointCallback, ChannelType, VolatilityChannel}, mode::ModeSwitcher};
