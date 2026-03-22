use crate::strategy::types::TradingMode;

/// 交易模式切换器
///
/// 根据市场波动率状态自动切换交易模式:
/// - Low: 正常模式
/// - Medium: 警戒模式
/// - High: 高频交易模式
pub struct ModeSwitcher {
    current_mode: TradingMode,
}

impl ModeSwitcher {
    pub fn new() -> Self {
        Self {
            current_mode: TradingMode::Low,
        }
    }

    pub fn mode(&self) -> TradingMode {
        self.current_mode
    }

    pub fn switch(&mut self, new_mode: TradingMode) {
        if self.current_mode != new_mode {
            self.current_mode = new_mode;
        }
    }
}

impl Default for ModeSwitcher {
    fn default() -> Self {
        Self::new()
    }
}
