#![forbid(unsafe_code)]

//! b_data_source - 业务数据层
//!
//! 提供市场数据处理功能：数据订阅、K线合成、订单簿、波动率检测等。
//!
//! 分层架构：
//! - ws/     - WebSocket 数据接口（K线、深度）
//! - api/    - REST API 数据接口（账户、持仓）
//! - 其他模块 - 内部实现

// Re-exports from a_common (仅基础设施错误和配置)
pub use a_common::MarketError;
pub use a_common::config::{Platform, Paths};
pub use a_common::logs::{CheckpointLogger, CompositeCheckpointLogger, ConsoleCheckpointLogger, Stage, StageResult, TracingCheckpointLogger};

// Sub-modules
pub mod kline_1m;
pub mod kline_1d;
pub mod order_books;
pub mod symbol_rules;
pub mod recovery;
pub mod misc;
pub mod volatility;
pub mod futures;
pub mod models;

// 统一数据接口层
pub mod ws;      // WebSocket 数据接口
pub mod api;     // REST API 数据接口

// Re-exports - Models (业务数据类型)
pub use models::{MarketStream, MockMarketStream};
pub use models::{KLine, Period, Tick};

// Re-exports - Data processing
pub use kline_1m::{KLineSynthesizer, KlinePersistence};
pub use symbol_rules::SymbolRegistry;
pub use order_books::OrderBook;
pub use recovery::{CheckpointData, CheckpointManager, RedisRecovery};

// Re-exports - Trade settings
pub use misc::{TradeSettings, PositionMode};

// Re-exports - Volatility
pub use volatility::{VolatilityManager, SymbolVolatility};

#[cfg(test)]
pub mod tests;
