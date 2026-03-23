//! 低频1天策略检查
//!
//! 检查顺序: a_exit → b_close → d_add → e_open
//! 优先检查退出/关仓，降低风险

#![forbid(unsafe_code)]

pub mod check;      // 检查: a_exit, b_close, d_add, e_open, check_chain, TriggerEvent
