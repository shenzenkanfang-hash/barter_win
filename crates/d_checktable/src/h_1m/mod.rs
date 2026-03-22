//! 高频1分钟策略检查
//!
//! 结构:
//! - a_trigger: 触发器
//! - b_open: 开仓
//! - c_add: 加仓
//! - d_risk: 风控
//! - e_exit: 退出

#![forbid(unsafe_code)]

pub mod a_trigger;
pub mod b_open;
pub mod c_add;
pub mod d_risk;
pub mod e_exit;
