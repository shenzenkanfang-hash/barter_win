//! 配置模块
//!
//! 包含:
//! - Platform: 平台检测 (Windows/Linux)
//! - Paths: 路径配置 (内存盘、磁盘、SQLite)
//! - VolatilityConfig: 波动率阈值配置

pub mod platform;
pub mod volatility;

pub use platform::{Platform, Paths};
pub use volatility::{VolatilityConfig, VOLATILITY_CONFIG};
