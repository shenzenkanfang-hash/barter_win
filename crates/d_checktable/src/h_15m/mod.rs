//! 高频15分钟策略检查
//!
//! 检查顺序: a_exit → b_close → d_add → e_open
//! 优先检查退出/关仓，降低风险

#![forbid(unsafe_code)]

pub mod market_status_generator;
pub mod pipeline_form;
pub mod signal_generator;
pub mod price_control_generator;
pub mod quantity_calculator;
pub mod check;

// 导出数量计算器
pub use quantity_calculator::{MinQuantityCalculator, MinQuantityConfig};

// 导出检查链类型
pub use check::check_chain::{CheckSignal, CheckChainContext, run_check_chain};

// 导出市场状态生成器
pub use market_status_generator::MinMarketStatusGenerator;
