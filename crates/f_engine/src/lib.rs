#![forbid(unsafe_code)]
#![allow(dead_code)]

//! f_engine - 交易引擎核心
//!
//! # 架构（精简后）
//! - `core/strategy_loop`: 协程管理（心跳监控 + 指数退避重启）
//! - `strategy/trader_manager`: 多品种 Trader 生命周期管理
//! - `types.rs`: 核心类型（StrategyId, TradingDecision, OrderRequest）
//!
//! # 与 h_15m 的关系
//! - h_15m Trader 自循环，Engine 通过 strategy_loop 协程管理

pub mod core;
pub mod interfaces;
pub mod strategy;
pub mod types;

// Re-exports - 策略协程管理
pub use core::strategy_loop::{StrategyLoop, StrategyLoopConfig, MAX_RESTART_COUNT, HEARTBEAT_TIMEOUT_MS};
pub use strategy::{TraderManager, StrategyType, TraderError};

// Re-exports - 核心类型
pub use types::{StrategyId, TradingDecision, OrderRequest, Side, OrderType, TradingAction};

// Re-exports - h_15m Trader（核心依赖）
pub use d_checktable::h_15m::Trader;
