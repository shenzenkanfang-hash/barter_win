//! 波动率阈值配置
//!
//! 全局波动率阈值配置，所有模块共享

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

/// 波动率阈值配置
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VolatilityConfig {
    /// 1分钟高波动阈值（默认 0.5% = 0.005）
    pub high_vol_1m: Decimal,
    /// 15分钟高波动阈值（默认 5% = 0.05）
    pub high_vol_15m: Decimal,
}

impl Default for VolatilityConfig {
    fn default() -> Self {
        Self {
            high_vol_1m: dec!(0.005),  // 0.5%
            high_vol_15m: dec!(0.05),  // 5%
        }
    }
}

impl VolatilityConfig {
    /// 创建配置
    pub fn new(high_vol_1m: Decimal, high_vol_15m: Decimal) -> Self {
        Self {
            high_vol_1m,
            high_vol_15m,
        }
    }

    /// 获取1分钟阈值
    pub fn threshold_1m(&self) -> Decimal {
        self.high_vol_1m
    }

    /// 获取15分钟阈值
    pub fn threshold_15m(&self) -> Decimal {
        self.high_vol_15m
    }
}

/// 全局波动率配置实例（静态）
use std::sync::LazyLock;
pub static VOLATILITY_CONFIG: LazyLock<VolatilityConfig> = LazyLock::new(|| {
    VolatilityConfig::default()
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = VolatilityConfig::default();
        assert_eq!(config.high_vol_1m, dec!(0.005));
        assert_eq!(config.high_vol_15m, dec!(0.05));
    }

    #[test]
    fn test_custom_config() {
        let config = VolatilityConfig::new(dec!(0.003), dec!(0.10));
        assert_eq!(config.high_vol_1m, dec!(0.003));
        assert_eq!(config.high_vol_15m, dec!(0.10));
    }
}
