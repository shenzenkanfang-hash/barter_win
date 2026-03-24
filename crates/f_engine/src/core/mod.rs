//! Core 模块 - 引擎核心组件
//!
//! # 子模块
//! - `engine`: TradingEngine 主引擎（原始版本）
//! - `engine_v2`: TradingEngine v2（基于接口的解耦版本）
//! - `engine_state`: 引擎全局状态管理（生产级）
//! - `strategy_pool`: 策略资金池
//! - `state`: 品种状态和交易锁
//!
//! # 架构说明
//! 推荐使用 `engine_v2`，它遵循：
//! - 接口强制规范
//! - 模块隔离原则
//! - 依赖注入模式

#![forbid(unsafe_code)]

pub mod engine;
pub mod engine_state;
pub mod state;
pub mod strategy_pool;
pub mod engine_v2;  // 基于接口的解耦架构

pub use engine::TradingEngine;
pub use d_checktable::h_15m::pipeline_form::PipelineForm;
pub use strategy_pool::{StrategyAllocation, StrategyPool};
pub use state::{SymbolState, SymbolMetrics, TradeLock, CheckConfig, StartupState};
pub use crate::types::{StrategyId, TradingDecision, OrderRequest, Side, OrderType};

// engine_state 导出
pub use engine_state::{
    EngineState, EngineStateHandle, EngineStatus, EngineMode, EngineMetricsSnapshot,
    HealthStatus, CircuitBreaker, CircuitBreakerConfig, CircuitBreakerAction,
    EngineStateError, Result as EngineStateResult,
};
// ModeSwitcher 和 Mode 从 channel 模块重导出 via types.rs
