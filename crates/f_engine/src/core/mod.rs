//! Core 模块 - 引擎核心组件
//!
//! # 子模块
//! - `engine_v2`: TradingEngine v2（V1.4 完整实现）
//! - `engine_state`: 引擎全局状态管理（生产级）
//! - `strategy_pool`: 策略资金池
//! - `state`: 品种状态和交易锁
//! - `business_types`: 业务数据类型（V1.4 文档定义）
//!
//! # 架构说明
//! 使用 `engine_v2` 的 `TradingEngineV2`，它遵循 V1.4 文档：
//! - 并行触发器 → CheckTables → StrategyQuery → 两级风控 → 抢锁 → 执行 → 状态对齐

#![forbid(unsafe_code)]

pub mod engine_state;
pub mod state;
pub mod strategy_pool;
pub mod engine_v2;  // TradingEngineV2 - V1.4 完整实现
pub mod business_types;  // 业务数据类型
pub mod triggers;  // 触发器模块
pub mod execution;  // 执行流程模块
pub mod fund_pool;  // 资金池管理
pub mod risk_manager;  // 风控管理
pub mod monitoring;  // 监控与超时
pub mod rollback;  // 回滚管理

#[cfg(test)]
mod tests;  // 测试模块

pub use d_checktable::h_15m::pipeline_form::PipelineForm;
pub use strategy_pool::{StrategyAllocation, StrategyPool};
pub use state::{SymbolState, SymbolMetrics, TradeLock, CheckConfig, StartupState};
pub use crate::types::{StrategyId, TradingDecision, OrderRequest, Side, OrderType};

// engine_v2 导出（TradingEngineV2 是唯一的主引擎）
pub use engine_v2::{TradingEngineV2, TradingEngineConfig};

// engine_state 导出
pub use engine_state::{
    EngineState, EngineStateHandle, EngineStatus, EngineMode, Environment,
    EngineMetricsSnapshot, HealthStatus, CircuitBreaker, CircuitBreakerConfig,
    CircuitBreakerAction, EngineStateError, Result as EngineStateResult,
};

// business_types 导出（V1.4 文档定义）
pub use business_types::{
    // 枚举类型
    PositionSide, VolatilityTier, RiskState, ChannelType, OrderLifecycle,
    // 结构体
    StrategyQuery, StrategyResponse, RiskCheckResult, PriceControlOutput,
    OrderInfo, FundPool,
    // 错误码
    EngineErrorCode,
};
