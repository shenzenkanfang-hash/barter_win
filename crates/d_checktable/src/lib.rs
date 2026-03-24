//! d_checktable - 检查层
//!
//! 按周期组织的策略检查：高频15分钟、高频1分钟、低频1天
//! 检查层异步并发执行，由引擎层统一调度

#![forbid(unsafe_code)]
#![allow(dead_code)]

pub mod check_table;
pub mod types;

// 周期策略模块
pub mod h_15m;     // 高频15分钟策略检查
pub mod l_1d;      // 低频1天策略检查

pub use check_table::{CheckTable, CheckEntry};
