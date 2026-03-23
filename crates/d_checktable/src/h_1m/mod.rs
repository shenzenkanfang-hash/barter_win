//! 高频1分钟策略检查
//!
//! 检查顺序: a_exit → b_close → c_risk → d_add → e_open → trigger(最终触发)
//! 优先检查退出/关仓，降低风险

#![forbid(unsafe_code)]

pub mod 信号;           // 信号: 市场状态、信号、价格控制生成器
pub mod trigger;       // trigger: 最终触发器状态机
pub mod a_exit;        // a_exit: 退出检查
pub mod b_close;       // b_close: 关仓检查
pub mod c_risk;        // c_risk: 风控检查
pub mod d_add;         // d_add: 加仓检查
pub mod e_open;        // e_open: 开仓检查
