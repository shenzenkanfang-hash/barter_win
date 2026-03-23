//! Channel 模块 - 交易通道和模式控制
//!
//! 负责交易模式管理和通道状态控制。

#![forbid(unsafe_code)]

pub mod mode_switcher;

pub use mode_switcher::{Mode, ModeSwitcher};
