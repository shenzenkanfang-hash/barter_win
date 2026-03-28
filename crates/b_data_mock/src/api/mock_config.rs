//! MockApiGateway 配置模块

use rust_decimal::Decimal;
use rust_decimal_macros::dec;

/// MockApiGateway 配置
#[derive(Debug, Clone)]
pub struct MockConfig {
    pub initial_balance: Decimal,
    pub fee_rate: Decimal,
    pub slippage_rate: Decimal,
    pub maintenance_margin_rate: Decimal,
    pub max_position_ratio: Decimal,
    pub min_reserve_ratio: Decimal,
}

impl Default for MockConfig {
    fn default() -> Self {
        Self {
            initial_balance: dec!(100000.0),
            fee_rate: dec!(0.0004),
            slippage_rate: dec!(0.0),
            maintenance_margin_rate: dec!(0.005),
            max_position_ratio: dec!(0.95),
            min_reserve_ratio: dec!(0.05),
        }
    }
}

impl MockConfig {
    pub fn new(initial_balance: Decimal) -> Self {
        Self {
            initial_balance,
            ..Default::default()
        }
    }

    pub fn with_fee_rate(mut self, fee_rate: Decimal) -> Self {
        self.fee_rate = fee_rate;
        self
    }

    pub fn with_slippage(mut self, slippage_rate: Decimal) -> Self {
        self.slippage_rate = slippage_rate;
        self
    }
}
