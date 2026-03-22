//! misc - 交易辅助模块
//!
//! 提供交易设置功能：设置杠杆、持仓模式、获取手续费率等。

pub mod trade_settings;

pub use trade_settings::{TradeSettings, PositionMode};
