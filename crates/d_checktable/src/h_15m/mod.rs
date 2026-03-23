//! 高频15分钟策略检查
//!
//! 检查顺序: a_exit → b_close → d_add → e_open
//! 优先检查退出/关仓，降低风险

#![forbid(unsafe_code)]

pub mod 信号;           // 信号: 市场状态、信号、价格控制生成器
pub mod check;        // 检查: a_exit, b_close, d_add, e_open, check_chain, TriggerEvent
