pub mod sqlite_persistence;
pub mod disaster_recovery;
pub mod persistence;
pub mod startup_recovery;

pub use persistence::{KLineCache, KLineData, PersistenceConfig, PersistenceService, PersistenceStats, PositionSnapshot, TradeRecord};
pub use sqlite_persistence::{
    AccountSnapshotRecord, ChannelEventRecord, EventRecorder, ExchangePositionRecord,
    IndicatorCsvWriter, IndicatorComparisonRow, IndicatorEventRecord,
    LocalPositionRecord, NoOpEventRecorder, RiskEventRecord, SqliteEventRecorder,
    SqliteRecordService, format_decimal, OrderRecord, SyncLogRecord,
};
pub use disaster_recovery::{
    AccountSnapshot, ApiPositionSnapshot, DisasterRecovery, LocalPositionSnapshot,
    OrderSnapshot, RecoveryData, SyncLogEntry, VerificationResult,
};
pub use startup_recovery::{
    RecoveryPriority, RecoveryStatus, RecoverySource, StartupRecoveryManager,
    UnifiedPositionSnapshot, UnifiedAccountSnapshot, RecoveryCheckpoint,
    VerificationResult as StartupVerificationResult, Discrepancy, DiscrepancySeverity,
    ResolvedDiscrepancy, Resolution, RecoveryResult as StartupRecoveryResult,
    SqliteRecoverySource, MemoryDiskRecoverySource, HardDiskRecoverySource,
};
