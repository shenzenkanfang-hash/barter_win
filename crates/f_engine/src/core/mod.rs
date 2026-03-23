//! Core 模块 - 引擎核心组件
//!
//! # 子模块
//! - `engine`: TradingEngine 主引擎
//! - `pipeline`: 交易流程编排器 (Legacy)
//! - `strategy_pool`: 策略资金池
//! - `state`: 品种状态和交易锁

#![forbid(unsafe_code)]

pub mod engine;
pub mod pipeline;
pub mod state;
pub mod strategy_pool;

pub use engine::TradingEngine;
pub use pipeline::{Pipeline, Processor, MockIndicatorProcessor, MockStrategyProcessor, MockRiskProcessor};
pub use d_checktable::h_15m::pipeline_form::PipelineForm;
pub use strategy_pool::{StrategyAllocation, StrategyPool};
pub use state::{SymbolState, TradeLock, CheckConfig, StartupState};
pub use crate::types::{StrategyId, TradingDecision, OrderRequest, Side, OrderType};
// ModeSwitcher 和 Mode 从 channel 模块重导出 via types.rs
