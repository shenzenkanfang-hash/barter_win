//! backup - 高速内存盘备份模块
//!
//! 将实时交易数据保存到高速内存盘 (E:/shm/backup)，定期同步到磁盘。
//! 用于快速读写高频交易数据，同时保证数据持久性。
//!
//! # 目录结构
//!
//! ```ignore
//! E:/shm/backup/
//! ├── account.json           # 账户信息
//! ├── positions.json         # 持仓（统一管理）
//! ├── trading_pairs.json     # 交易品种列表
//! ```

pub mod memory_backup;

pub use crate::api::SymbolRulesData;

pub use memory_backup::{
    AccountSnapshot, DepthData, DepthEntry, IndicatorsData, KlineData, KlineEntry,
    MemoryBackup, PositionSnapshot, Positions, SymbolMutexStatus,
    SystemConfig, TaskInfo, TaskPool, TradingPairInfo, TradingPairs, ChannelData,
    ACCOUNT_FILE, DEPTH_DIR, INDICATORS_1D_HISTORY_DIR, INDICATORS_1D_REALTIME_DIR,
    INDICATORS_1M_HISTORY_DIR, INDICATORS_1M_REALTIME_DIR, KLINE_1D_HISTORY_DIR,
    KLINE_1D_REALTIME_DIR, KLINE_1M_HISTORY_DIR, KLINE_1M_REALTIME_DIR,
    MAX_CSV_FILE_SIZE, MAX_DEPTH_ENTRIES, MAX_INDICATORS_ENTRIES, MAX_KLINE_ENTRIES,
    MAX_TASKS_ENTRIES, MAX_TRADES_ENTRIES, MUTEX_DIR, MUTEX_HOUR_DIR, MUTEX_MINUTE_DIR,
    POSITIONS_FILE, RULES_DIR, SYSTEM_CONFIG_FILE, TASKS_DAILY_DIR, TASKS_DIR,
    TASKS_MINUTE_DIR, TRADES_DIR, memory_backup_dir,
};
