#![forbid(unsafe_code)]
#![allow(dead_code)]

//! b_data_mock - 模拟数据层
//!
//! 与 b_data_source 完全对齐的模拟实现，用于沙盒测试。
//!
//! # 架构设计
//!
//! | b_data_source (实盘) | b_data_mock (模拟) |
//! |---------------------|---------------------|
//! | ws/kline_1m/       | ws/kline_1m/        |
//! | ws/kline_1d/       | ws/kline_1d/        |
//! | ws/order_books/     | ws/order_books/     |
//! | store/             | store/              |
//!
//! # 使用方式
//!
//! main.rs 中通过 feature flag 切换:
//! - `cargo run --features mock` → 使用 b_data_mock
//! - `cargo run` → 使用 b_data_source

// ============================================================================
// 核心子模块
// ============================================================================

pub mod models;
pub mod store;
pub mod ws;
pub mod api;
pub mod replay_source;
pub mod trader_pool;
pub mod symbol_rules;
pub mod history;
pub mod recovery;

// ============================================================================
// 公开导出 - 与 b_data_source 完全一致
// ============================================================================

// a_common 复用
pub use a_common::MarketError;
pub use a_common::config::{Platform, Paths};
pub use a_common::logs::{CheckpointLogger, CompositeCheckpointLogger, ConsoleCheckpointLogger, Stage, StageResult, TracingCheckpointLogger};

// Models
pub use models::{MarketStream, MockMarketStream};
pub use models::{KLine, Period, Tick};

// Store
pub use store::{MarketDataStore, MarketDataStoreImpl, OrderBookData, VolatilityData};

// WS
pub use ws::kline_1m::{Kline1mStream, KLineSynthesizer, KlineData, StoreRef};
pub use ws::kline_1d::Kline1dStream;
pub use ws::order_books::{OrderBook, DepthStream, DepthData};
pub use ws::kline_generator::{KlineStreamGenerator, SimulatedKline};

// API
pub use api::symbol_registry::SymbolRegistry;
pub use api::trade_settings::{TradeSettings, PositionMode};
pub use api::DataFeeder;
pub use api::mock_account::{Account, Side};
pub use api::mock_gateway::MockApiGateway;
pub use api::mock_config::{MockConfig, MockExecutionConfig};

// Recovery
pub use recovery::{CheckpointData, CheckpointManager, MockRecovery};

// SymbolRules
pub use symbol_rules::{SymbolRuleService, ParsedSymbolRules};

// TraderPool
pub use trader_pool::{SymbolMeta, TradingStatus, TraderPool};

// Replay
pub use replay_source::{KLineSource, ReplayError, ReplaySource};

// History
pub use history::{HistoryDataManager, HistoryDataProvider};
pub use history::{DataIssue, DataSource, HistoryError, HistoryRequest, HistoryResponse, KlineMetadata};

// Interceptor - 心跳延迟监控拦截器
pub mod interceptor;
pub use interceptor::{TickInterceptor, OrderInterceptor};
pub use interceptor::order_interceptor::{OrderInterceptorConfig, OrderStats};

// TradingEventTracker - 策略交易事件追踪器
pub mod trading_event_tracker;
pub use trading_event_tracker::{
    StrategyEventTracker, SimpleMatchEngine,
    TradingEvent, EventStats, PnlDataPoint,
    ReplayReport, MaxProfitMoment, MaxDrawdownMoment, InvalidPeriod,
};
