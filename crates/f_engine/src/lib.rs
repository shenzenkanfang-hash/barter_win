#![forbid(unsafe_code)]
#![allow(dead_code)]

//! f_engine - 交易引擎核心
//!
//! # 架构
//! - `core/engine`: 事件驱动引擎（推荐使用）
//! - `core/strategy_loop`: 协程管理（已废弃）
//! - `strategy/trader_manager`: 多品种管理（已废弃）
//! - `types.rs`: 核心类型
//!
//! # 推荐使用
//! ```ignore
//! use f_engine::core::EventDrivenEngine;
//! 
//! let mut engine = EventDrivenEngine::new(symbol, executor, risk_checker);
//! engine.run(tick_rx).await;
//! ```

pub mod core;
pub mod interfaces;
pub mod strategy;
pub mod types;

// Re-exports - 事件驱动引擎
pub use core::EventDrivenEngine;

// Re-exports - 策略协程管理（已废弃）
pub use core::strategy_loop::{StrategyLoop, StrategyLoopConfig, MAX_RESTART_COUNT, HEARTBEAT_TIMEOUT_MS};
pub use strategy::{TraderManager, StrategyType, TraderError};

// Re-exports - 核心类型
pub use types::{StrategyId, TradingDecision, OrderRequest, Side, OrderType, TradingAction};

// Re-exports - h_15m Trader
pub use d_checktable::h_15m::Trader;
