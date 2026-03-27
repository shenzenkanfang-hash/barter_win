#![forbid(unsafe_code)]
#![allow(dead_code)]

//! f_engine - 交易引擎核心
//!
//! # 架构
//! - `event/`: 事件驱动引擎（推荐使用）
//! - `core/engine`: 基础引擎
//! - `core/strategy_loop`: 协程管理（已废弃）
//! - `strategy/trader_manager`: 多品种管理（已废弃）
//! - `types.rs`: 核心类型
//!
//! # 推荐使用（事件驱动）
//! ```ignore
//! use f_engine::event::{EventEngine, EngineConfig, EventBus};
//!
//! let (bus, handle) = EventBus::default();
//! let mut engine = EventEngine::new(config, risk_checker, strategy, gateway);
//! engine.run(bus.tick_rx()).await;
//! ```

pub mod core;
pub mod event;          // 新增：事件驱动模块
pub mod interfaces;
pub mod strategy;
pub mod types;

// Re-exports - 事件驱动引擎（推荐）
pub use event::{EventEngine, EventBus, EventBusHandle, EngineConfig, EngineState};
pub use event::event_bus::DEFAULT_CHANNEL_BUFFER;

// Re-exports - 核心引擎
pub use core::EventDrivenEngine;

// Re-exports - 策略协程管理（已废弃）
pub use core::strategy_loop::{StrategyLoop, StrategyLoopConfig, MAX_RESTART_COUNT, HEARTBEAT_TIMEOUT_MS};
pub use strategy::{TraderManager, StrategyType, TraderError};

// Re-exports - 核心类型
pub use types::{StrategyId, TradingDecision, OrderRequest, Side, OrderType, TradingAction};

// Re-exports - h_15m Trader
pub use d_checktable::h_15m::Trader;
