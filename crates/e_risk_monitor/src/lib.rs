#![forbid(unsafe_code)]

pub mod risk;
pub mod position;
pub mod persistence;
pub mod shared;

// Re-exports - 从 a_common::backup 获取内存备份类型
pub use a_common::backup::{MemoryBackup, memory_backup_dir};

// Re-exports from risk module (common and minute_risk submodules)
pub use risk::common::{RiskPreChecker, VolatilityMode, RiskReChecker, OrderCheck, OrderCheckResult, OrderReservation, ThresholdConstants};
pub use risk::minute_risk::{calculate_hour_open_notional, calculate_minute_open_notional, calculate_open_qty_from_notional, MinuteOpenResult};
pub use position::{position_manager::{Direction, LocalPosition, LocalPositionManager, PositionStats}, position_exclusion::{PositionDirection, PositionExclusionChecker, PositionInfo}};
pub use persistence::{persistence::{KLineCache, KLineData, PersistenceConfig, PersistenceService, PersistenceStats, PositionSnapshot, TradeRecord}, sqlite_persistence::{AccountSnapshotRecord, ChannelEventRecord, EventRecorder, ExchangePositionRecord, IndicatorCsvWriter, IndicatorComparisonRow, IndicatorEventRecord, LocalPositionRecord, NoOpEventRecorder, RiskEventRecord, SqliteEventRecorder, SqliteRecordService, format_decimal, OrderRecord, SyncLogRecord}, disaster_recovery::{AccountSnapshot, ApiPositionSnapshot, DisasterRecovery, LocalPositionSnapshot, OrderSnapshot, RecoveryData, SyncLogEntry, VerificationResult}};
pub use shared::{account_pool::{AccountInfo, AccountMargin, AccountPool, CircuitBreakerState}, margin_config::{GlobalMarginConfig, MarginPoolConfig, MinuteOpenConfig, StrategyLevel}, market_status::{MarketStatus, MarketStatusDetector, PinIntensity, PinDetection}, pnl_manager::PnlManager, round_guard::{RoundGuard, RoundGuardScope}};
