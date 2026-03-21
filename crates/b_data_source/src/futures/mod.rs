//! futures - USDT 合约数据获取模块
//!
//! 纯数据获取层，只做字段解析，不涉及业务逻辑判断。

pub mod account;
pub mod position;

pub use account::{FuturesAccount, FuturesAccountData};
pub use position::{FuturesPosition, FuturesPositionData};
