//! 高频1分钟策略检查
//!
//! 检查顺序: 退出 → 风控 → 加仓 → 开仓 → 触发
//! 优先检查关仓，降低风险

#![forbid(unsafe_code)]

pub mod a_trigger;
pub mod b_exit;
pub mod c_close;
pub mod d_risk;
pub mod e_add;
pub mod f_open;
