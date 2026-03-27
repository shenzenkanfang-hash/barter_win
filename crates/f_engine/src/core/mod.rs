//! Core 模块 - 引擎核心组件
//!
//! # 精简后保留
//! - `strategy_loop`: 策略自循环协程管理（spawn/stop/心跳监控/指数退避重启）

#![forbid(unsafe_code)]

pub mod strategy_loop;

pub use strategy_loop::{StrategyLoop, StrategyLoopConfig, MAX_RESTART_COUNT, HEARTBEAT_TIMEOUT_MS};
