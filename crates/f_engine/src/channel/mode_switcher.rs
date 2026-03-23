//! 交易模式和通道切换器
//!
//! 管理交易引擎的运行模式（正常/回测/仿真/维护）。

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// 交易模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    /// 正常交易模式
    Normal,
    /// 回测模式
    Backtest,
    /// 仿真模式
    Paper,
    /// 维护模式
    Maintenance,
}

impl Default for Mode {
    fn default() -> Self {
        Mode::Normal
    }
}

/// 模式切换器
#[derive(Debug, Clone)]
pub struct ModeSwitcher {
    current_mode: Mode,
}

impl ModeSwitcher {
    pub fn new() -> Self {
        Self {
            current_mode: Mode::Normal,
        }
    }

    pub fn mode(&self) -> Mode {
        self.current_mode
    }

    pub fn set_mode(&mut self, mode: Mode) {
        self.current_mode = mode;
    }

    pub fn is_trading_allowed(&self) -> bool {
        self.current_mode == Mode::Normal || self.current_mode == Mode::Paper
    }
}

impl Default for ModeSwitcher {
    fn default() -> Self {
        Self::new()
    }
}
