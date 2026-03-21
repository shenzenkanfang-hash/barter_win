#![forbid(unsafe_code)]

pub mod risk;
pub mod position;
pub mod persistence;
pub mod shared;

// Re-exports
pub use risk::{risk::{RiskPreChecker, VolatilityMode}, risk_rechecker::RiskReChecker, order_check::{OrderCheck, OrderCheckResult, OrderReservation}, thresholds::ThresholdConstants, minute_risk::{calculate_hour_open_notional, calculate_minute_open_notional, calculate_open_qty_from_notional, MinuteOpenResult}};
pub use position::{position_manager::{Direction, LocalPosition, LocalPositionManager, PositionStats}, position_exclusion::{PositionDirection, PositionExclusionChecker, PositionInfo}};
pub use persistence::{memory_backup::{AccountSnapshot as MemoryAccountSnapshot, DepthData, DepthEntry, IndicatorsData, KlineEntry, MemoryBackup, PositionSnapshot as MemoryPositionSnapshot, SymbolRulesData}, persistence::{KLineCache, KLineData, PersistenceConfig, PersistenceService, PersistenceStats, PositionSnapshot, TradeRecord}, sqlite_persistence::{AccountSnapshotRecord, ChannelEventRecord, EventRecorder, ExchangePositionRecord, IndicatorCsvWriter, IndicatorComparisonRow, IndicatorEventRecord, LocalPositionRecord, NoOpEventRecorder, RiskEventRecord, SqliteEventRecorder, SqliteRecordService, format_decimal, OrderRecord, SyncLogRecord}, disaster_recovery::{AccountSnapshot, ApiPositionSnapshot, DisasterRecovery, LocalPositionSnapshot, OrderSnapshot, RecoveryData, SyncLogEntry, VerificationResult}};
pub use shared::{account_pool::{AccountInfo, AccountMargin, AccountPool, CircuitBreakerState}, margin_config::{GlobalMarginConfig, MarginPoolConfig, MinuteOpenConfig, StrategyLevel}, market_status::{MarketStatus, MarketStatusDetector, PinIntensity, PinDetection}, pnl_manager::PnlManager, round_guard::{RoundGuard, RoundGuardScope}};
