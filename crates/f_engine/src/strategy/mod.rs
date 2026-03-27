//! Strategy 模块 - 策略协程管理
//!
//! # 精简后保留
//! - `TraderManager`: 多品种 Trader 生命周期管理（启动/停止/健康检查）

pub mod trader_manager;

pub use trader_manager::{TraderManager, StrategyType, TraderError};
