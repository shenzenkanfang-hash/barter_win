//! 低频1天策略检查
//!
//! 检查顺序: a_exit → b_close → d_add → e_open
//! 优先检查退出/关仓，降低风险

#![forbid(unsafe_code)]

pub mod market_status_generator;  // 通用市场状态生成器
pub mod signal;                   // 信号生成器模块
pub mod check;                    // 检查: a_exit, b_close, d_add, e_open, check_chain

// 导出检查链类型
pub use check::check_chain::{CheckSignal, TriggerEvent, run_check_chain};

// 导出市场状态生成器
pub use market_status_generator::DayMarketStatusGenerator;
