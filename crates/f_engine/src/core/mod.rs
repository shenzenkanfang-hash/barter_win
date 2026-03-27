//! Core 模块 - 引擎核心组件
//!
//! # 架构
//! - `strategy_loop`: 策略自循环协程管理（已废弃，推荐使用 engine）
//! - `engine`: 事件驱动引擎（新架构，推荐使用）

#![forbid(unsafe_code)]

pub mod strategy_loop;
pub mod engine;

pub use strategy_loop::{StrategyLoop, StrategyLoopConfig, MAX_RESTART_COUNT, HEARTBEAT_TIMEOUT_MS};
pub use engine::EventDrivenEngine;
