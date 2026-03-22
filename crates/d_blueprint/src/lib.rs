//! d_blueprint - 策略蓝图层
//!
//! 统一记录各流水线判断结果，供引擎层执行闭环

#![forbid(unsafe_code)]

pub mod check_table;

pub use check_table::{CheckTable, CheckEntry};
