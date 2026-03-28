#![forbid(unsafe_code)]
#![allow(dead_code)]

//! b_data_mock - 模拟数据层
//!
//! 与 b_data_source 对齐的模拟实现，用于沙盒测试。
//!
//! # 架构设计
//!
//! | b_data_source (实盘) | b_data_mock (模拟) |
//! |---------------------|---------------------|
//! | ws/kline_1m/       | ws/kline_1m/ (StreamTickGenerator) |
//! | ws/kline_1d/       | ws/kline_1d/ (1m聚合) |
//! | ws/order_books/     | ws/order_books/ (模拟) |
//! | api/account.rs     | api/account.rs (MockAccount) |
//! | api/position.rs    | api/position.rs (MockAccount) |
//! | api/data_sync.rs   | api/data_sync.rs (MockAccount) |
//! | store/             | store/ (复用) |
//!
//! # 使用方式
//!
//! main.rs 中通过 feature flag 切换:
//! - `cargo run --features mock` → 使用 b_data_mock
//! - `cargo run` → 使用 b_data_source

// ============================================================================
// 核心子模块
// ============================================================================

pub mod models;       // 数据类型：KLine, Tick, Period
pub mod store;        // 统一存储：MarketDataStore
pub mod ws;           // 模拟 WebSocket 数据源
pub mod api;          // 模拟 REST API 数据源

// ============================================================================
// 辅助子模块
// ============================================================================

pub mod replay_source;    // 历史数据回放
pub mod trader_pool;     // 交易品种池
pub mod symbol_rules;     // 交易对规则
pub mod history;         // 历史数据管理
pub mod recovery;        // 灾备恢复

// ============================================================================
// 公开导出
// ============================================================================

// Models
pub use models::{KLine, Period, Tick, MarketStream, MockMarketStream};

// Store
pub use store::{MarketDataStore, MarketDataStoreImpl, OrderBookData, VolatilityData};

// WS - K线
pub use ws::{Kline1mStream, KLineSynthesizer, KlineData};
pub use ws::kline_1d::Kline1dStream;

// WS - 订单簿
pub use ws::{OrderBook, DepthStream, DepthData};

// API - 模拟网关（核心）
pub use api::{MockApiGateway, MockConfig, Account};

// API - 账户/持仓
pub use api::{FuturesAccount, FuturesAccountData};
pub use api::{FuturesPosition, FuturesPositionData};
pub use api::{FuturesDataSyncer, FuturesSyncResult};

// API - 其他
pub use api::{SymbolRegistry, TradeSettings, PositionMode, DataFeeder};

// Tick 生成器（独立文件）
pub use ws::tick_generator::{StreamTickGenerator, SimulatedTick};
pub use ws::noise::GaussianNoise;

// Replay
pub use replay_source::{ReplaySource, ReplayError, KLineSource};

// TraderPool
pub use trader_pool::{TraderPool, SymbolMeta, TradingStatus};

// SymbolRules
pub use symbol_rules::{SymbolRuleService, ParsedSymbolRules};

// History
pub use history::{KLine as HistoryKLine, HistoryRequest, HistoryResponse, HistoryError};

// Recovery
pub use recovery::{CheckpointData, CheckpointManager, MockRecovery};

// ============================================================================
// 便捷构造函数
// ============================================================================

use rust_decimal::Decimal;

/// 创建模拟 K线流（从历史数据）
pub fn create_kline_stream(
    symbol: String,
    klines: Vec<KLine>,
) -> Kline1mStream {
    let source = ReplaySource::from_data(klines);
    Kline1mStream::from_klines(symbol, Box::new(source))
}

/// 创建默认模拟网关
pub fn create_mock_gateway(initial_balance: Decimal) -> MockApiGateway {
    MockApiGateway::new(initial_balance, MockConfig::default())
}

/// 创建模拟数据提供者
pub fn create_data_feeder() -> DataFeeder {
    DataFeeder::new()
}

/// 创建模拟交易品种池
pub fn create_trader_pool(symbols: Vec<&str>) -> TraderPool {
    let pool = TraderPool::new();
    for symbol in symbols {
        pool.register(SymbolMeta::new(symbol).with_status(TradingStatus::Active));
    }
    pool
}
