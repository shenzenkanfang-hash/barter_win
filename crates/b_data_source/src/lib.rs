#![forbid(unsafe_code)]
//! b_data_source - 业务数据层
//!
//! 提供市场数据处理功能：数据订阅、K线合成、订单簿、波动率检测等。
//!
//! 分层架构：
//! - ws/     - WebSocket 数据接口（K线、深度）
//! - api/    - REST API 数据接口（账户、持仓、交易设置）
//! - 其他模块 - 内部实现（recovery, volatility等）

// Re-exports from a_common (仅基础设施错误和配置)
pub use a_common::MarketError;
pub use a_common::config::{Platform, Paths};
pub use a_common::logs::{CheckpointLogger, CompositeCheckpointLogger, ConsoleCheckpointLogger, Stage, StageResult, TracingCheckpointLogger};

// Sub-modules
pub mod recovery;
pub mod models;
pub mod trader_pool;     // 品种池
pub mod replay_source;    // 历史数据回放

// 统一数据接口层
pub mod ws;      // WebSocket 数据接口
pub mod api;     // REST API 数据接口
pub mod symbol_rules; // 交易对规则服务
pub mod history; // 历史数据管理层

// Re-exports - Models (业务数据类型)
pub use models::{MarketStream, MockMarketStream};
pub use models::{KLine, Period, Tick};

// Re-exports - Data processing
pub use api::symbol_registry::SymbolRegistry;
pub use recovery::{CheckpointData, CheckpointManager, RedisRecovery};

// Re-exports - Trade settings
pub use api::trade_settings::{TradeSettings, PositionMode};

// Re-exports - Volatility
pub use ws::{VolatilityManager, SymbolVolatility};

// Re-exports - DataFeeder (统一数据接口)
pub use api::DataFeeder;

// Re-exports - SymbolRules (交易对规则服务)
pub use symbol_rules::{SymbolRuleService, ParsedSymbolRules};

// Re-exports - TraderPool (品种池)
pub use trader_pool::{SymbolMeta, TradingStatus, TraderPool};

// Re-exports - ReplaySource (历史数据回放)
pub use replay_source::{KLineSource, ReplayError, ReplaySource};

// Re-exports - HistoryDataManager (历史数据管理)
pub use history::{HistoryDataManager, HistoryDataProvider};
pub use history::{DataIssue, DataSource, HistoryError, HistoryRequest, HistoryResponse, KlineMetadata};

// Re-exports - MarketDataStore (统一存储接口)
pub mod store;
pub use store::{MarketDataStore, MarketDataStoreImpl, OrderBookData, VolatilityData};

// ============================================================================
// 默认存储实例（全局单例，供真实 WS 使用）
// 模拟器/回测请创建独立实例
// ============================================================================

use std::sync::Arc;
use once_cell::sync::OnceCell;

static DEFAULT_STORE: OnceCell<Arc<MarketDataStoreImpl>> = OnceCell::new();

/// 获取默认存储实例
///
/// 真实 WS 使用此实例写入数据
/// 策略通过此实例读取数据
pub fn default_store() -> &'static Arc<MarketDataStoreImpl> {
    DEFAULT_STORE.get_or_init(|| Arc::new(MarketDataStoreImpl::new()))
}

/// 便捷宏：写入K线
#[macro_export]
macro_rules! store_write_kline {
    ($symbol:expr, $kline:expr, $closed:expr) => {
        $crate::default_store().write_kline($symbol, $kline, $closed)
    };
}

/// 便捷宏：读取当前K线
#[macro_export]
macro_rules! store_get_kline {
    ($symbol:expr) => {
        $crate::default_store().get_current_kline($symbol)
    };
}

/// 便捷宏：读取波动率
#[macro_export]
macro_rules! store_get_volatility {
    ($symbol:expr) => {
        $crate::default_store().get_volatility($symbol)
    };
}
