//! 配置模块
//!
//! 包含:
//! - Platform: 平台检测 (Windows/Linux)
//! - Paths: 路径配置 (内存盘、磁盘、SQLite)

pub mod platform;

pub use platform::{Platform, Paths};
