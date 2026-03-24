#![forbid(unsafe_code)]
#![allow(dead_code)]

//! f_engine - 交易引擎核心
//!
//! 提供量化交易引擎的核心组件。
//!
//! # 架构原则
//! 1. **模块隔离**：禁止直接访问其他模块内部
//! 2. **接口强制**：所有跨模块调用通过 Trait 接口
//! 3. **依赖注入**：核心组件通过构造函数注入
//!
//! # 模块结构
//! - `interfaces/` - 统一接口层（核心）
//! - `core/` - 核心引擎实现
//! - `order/` - 订单执行模块
//! - `channel/` - 通道切换模块
//! - `strategy/` - 策略定义

pub mod channel;
pub mod core;
pub mod order;
pub mod strategy;
pub mod types;

/// 接口层 - 跨模块交互的唯一入口
///
/// 所有模块间调用必须通过这里定义的 Trait 接口。
pub mod interfaces;

// Re-exports - Strategy
pub use strategy::{
    Direction, MarketStatus, MarketStatusType, SignalType, SignalAggregator, Strategy, 
    StrategyExecutor, StrategyFactory, StrategyKLine, StrategyState, StrategyStatus, 
    TradingSignal, VolatilityLevel,
};

// Re-exports - Interfaces
pub use interfaces::{
    // 市场数据接口
    MarketDataProvider, MarketKLine, MarketTick, VolatilityInfo, VolatilityLevel as InterfaceVolatilityLevel,
    // 策略接口
    StrategyExecutor as StrategyExecutorTrait, StrategyInstance, TradingSignal as InterfaceTradingSignal,
    SignalDirection, SignalType as InterfaceSignalType, StrategyState as InterfaceStrategyState,
    SignalAggregator as SignalAggregatorTrait, StrategyFactory as StrategyFactoryTrait,
    // 风控接口
    RiskChecker, RiskCheckResult, RiskLevel, OrderRequest as RiskOrderRequest,
    AccountInfo, PositionInfo, OrderSide, OrderType as RiskOrderType,
    // 执行接口
    ExchangeGateway, OrderResult, OrderStatus,
};

pub use core::engine_v2::{
    TradingEngine, TradingMode, EngineState as EngineStateV2, EngineError,
};

// Re-exports - Engine State (生产级)
pub use core::engine_state::{
    EngineState, EngineStateHandle, EngineStatus, EngineMode, Environment,
    EngineMetricsSnapshot, HealthStatus, CircuitBreaker, CircuitBreakerConfig,
    CircuitBreakerAction, EngineStateError, Result as EngineStateResult,
};

// Re-exports - Business Types (V1.4 文档定义)
pub use core::business_types::{
    // 枚举类型
    PositionSide, VolatilityTier, RiskState, ChannelType, OrderLifecycle,
    TradingAction,
    // 结构体
    StrategyQuery, StrategyResponse, RiskCheckResult as BusinessRiskCheckResult,
    PriceControlOutput,
    OrderInfo, FundPool,
    // 错误码
    EngineErrorCode,
};
