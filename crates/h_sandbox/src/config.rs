//! ShadowGateway 配置模块
//!
//! 劫持模式网关的配置文件

use rust_decimal::Decimal;
use rust_decimal_macros::dec;

/// ShadowBinanceGateway 配置
#[derive(Debug, Clone)]
pub struct ShadowConfig {
    /// 初始余额
    pub initial_balance: Decimal,
    /// 手续费率 (Taker)
    pub fee_rate: Decimal,
    /// 滑点率
    pub slippage_rate: Decimal,
    /// 维持保证金率 (USDT永续默认 0.5%)
    pub maintenance_margin_rate: Decimal,
    /// 最大持仓比例
    pub max_position_ratio: Decimal,
    /// 最小保留比例
    pub min_reserve_ratio: Decimal,
}

impl Default for ShadowConfig {
    fn default() -> Self {
        Self {
            initial_balance: dec!(100000.0),
            fee_rate: dec!(0.0004),            // 0.04%
            slippage_rate: dec!(0.0),           // 默认无滑点
            maintenance_margin_rate: dec!(0.005), // 0.5%
            max_position_ratio: dec!(0.95),    // 最大95%仓位
            min_reserve_ratio: dec!(0.05),     // 最低5%保留
        }
    }
}

impl ShadowConfig {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ShadowConfig::default();
        assert_eq!(config.initial_balance, dec!(100000.0));
        assert_eq!(config.fee_rate, dec!(0.0004));
    }

    #[test]
    fn test_custom_balance() {
        let config = ShadowConfig::new(dec!(50000.0));
        assert_eq!(config.initial_balance, dec!(50000.0));
    }

    #[test]
    fn test_builder_pattern() {
        let config = ShadowConfig::default()
            .with_fee_rate(dec!(0.0002))
            .with_slippage(dec!(0.0001));
        
        assert_eq!(config.fee_rate, dec!(0.0002));
        assert_eq!(config.slippage_rate, dec!(0.0001));
    }
}
