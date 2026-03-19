use strategy::TradingMode;

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
