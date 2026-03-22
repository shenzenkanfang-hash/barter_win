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
pub mod backup;

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
pub use ws::{BinanceTradeStream, BinanceCombinedStream, BinanceWsConnector};
pub use ws::{MarketConnector, MockMarketConnector};

// Re-exports - Backup
pub use backup::{
    AccountSnapshot, ChannelData, DepthData, DepthEntry, IndicatorsData, KlineData, KlineEntry,
    MemoryBackup, PositionSnapshot, Positions, SymbolMutexStatus, SymbolRulesData, TaskInfo, TaskPool,
    TradingPairInfo, TradingPairs, ACCOUNT_FILE, DEPTH_DIR, INDICATORS_1D_HISTORY_DIR,
    INDICATORS_1D_REALTIME_DIR, INDICATORS_1M_HISTORY_DIR, INDICATORS_1M_REALTIME_DIR,
    KLINE_1D_HISTORY_DIR, KLINE_1D_REALTIME_DIR, KLINE_1M_HISTORY_DIR, KLINE_1M_REALTIME_DIR,
    MAX_CSV_FILE_SIZE, MAX_DEPTH_ENTRIES, MAX_INDICATORS_ENTRIES, MAX_KLINE_ENTRIES,
    MAX_TASKS_ENTRIES, MAX_TRADES_ENTRIES, memory_backup_dir, MUTEX_DIR, MUTEX_HOUR_DIR,
    MUTEX_MINUTE_DIR, POSITIONS_FILE, RULES_DIR, TASKS_DAILY_DIR, TASKS_DIR, TASKS_MINUTE_DIR,
    TRADES_DIR,
};
