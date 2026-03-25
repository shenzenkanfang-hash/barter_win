#![forbid(unsafe_code)]
#![allow(dead_code)]

//! f_engine - 交易引擎核心
//!
//! 提供量化交易引擎的核心组件。

pub mod channel;
pub mod core;
pub mod order;
pub mod strategy;
pub mod types;

/// 接口层 - 跨模块交互的唯一入口
pub mod interfaces;

// Re-exports - Interfaces
pub use interfaces::{
    // 市场数据接口
    MarketDataProvider, MarketKLine, MarketTick, VolatilityInfo,
    // 策略接口
    StrategyExecutor, StrategyInstance, TradingSignal, SignalDirection, SignalType,
    StrategyState, SignalAggregator, StrategyFactory,
    // 风控接口
    RiskChecker, RiskLevel, PositionInfo, ExecutedOrder, RiskWarning, RiskThresholds,
    // 执行接口
    ExchangeGateway,
    // CheckTable 接口
    CheckTableProvider, CheckTable, CheckTableResult, CheckTableConfig,
};

// Re-exports - Types
pub use crate::types::OrderRequest;
pub use a_common::models::types::OrderStatus;
pub use a_common::models::market_data::{OrderBookLevel, OrderBookSnapshot};

pub use core::engine_v2::{TradingEngineV2, TradingEngineConfig};

pub use core::engine_state::{
    EngineState, EngineStateHandle, EngineStatus, EngineMode, Environment,
    EngineMetricsSnapshot, HealthStatus, CircuitBreaker, CircuitBreakerConfig,
    CircuitBreakerAction, EngineStateError, Result as EngineStateResult,
};

pub use core::business_types::{
    PositionSide, VolatilityTier, RiskState, ChannelType, OrderLifecycle,
    TradingAction, StrategyQuery, StrategyResponse, RiskCheckResult,
    PriceControlOutput, OrderInfo, FundPool, EngineErrorCode,
};

// Re-exports - Strategy Signal (for strategy-engine communication)
pub use x_data::trading::signal::{
    StrategySignal, TradeCommand, StrategyId, StrategyType, StrategyLevel, PositionRef,
};

// Re-exports - CheckTable (dual-cycle)
pub use d_checktable::h_15m::{MinQuantityCalculator, MinQuantityConfig};
pub use d_checktable::l_1d::{DayQuantityCalculator, DayQuantityConfig};
pub use d_checktable::h_15m::check::CheckChainContext as MinCheckChainContext;
pub use d_checktable::l_1d::check::CheckChainContext as DayCheckChainContext;
