//! Pin 策略动态杠杆模块
//!
//! **警告**: Trend 策略禁用此模块！

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

/// 波动级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PinVolatilityLevel {
    Low,
    Normal,
    High,
    Extreme,
}

/// 插针动态杠杆配置
///
/// 使用数组而非 HashMap，提高性能。
#[derive(Debug, Clone)]
pub struct PinLeverageConfig {
    /// 各波动级别对应的杠杆倍数 [Low, Normal, High, Extreme]
    leverage_by_level: [Decimal; 4],
    /// 高波动阈值 (超过此值认为是高波动)
    high_volatility_threshold: Decimal,
}

impl Default for PinLeverageConfig {
    fn default() -> Self {
        Self {
            // [Low=15x, Normal=10x, High=5x, Extreme=2x]
            leverage_by_level: [dec!(15), dec!(10), dec!(5), dec!(2)],
            high_volatility_threshold: dec!(0.03), // 3%
        }
    }
}

impl PinLeverageConfig {
    /// 获取指定波动级别的杠杆
    pub fn get_leverage(&self, level: PinVolatilityLevel) -> Decimal {
        match level {
            PinVolatilityLevel::Low => self.leverage_by_level[0],
            PinVolatilityLevel::Normal => self.leverage_by_level[1],
            PinVolatilityLevel::High => self.leverage_by_level[2],
            PinVolatilityLevel::Extreme => self.leverage_by_level[3],
        }
    }
}

/// Pin策略杠杆守卫 (Pin 专用)
///
/// **警告**: Trend 策略不应使用此模块！
#[derive(Debug, Clone)]
pub struct PinRiskLeverageGuard {
    config: PinLeverageConfig,
}

impl PinRiskLeverageGuard {
    /// 创建 Pin 杠杆守卫
    pub fn new(config: PinLeverageConfig) -> Self {
        Self { config }
    }

    /// 获取波动级别
    pub fn get_volatility_level(&self, volatility: Decimal) -> PinVolatilityLevel {
        if volatility >= self.config.high_volatility_threshold * dec!(2) {
            PinVolatilityLevel::Extreme
        } else if volatility >= self.config.high_volatility_threshold * dec!(1.5) {
            PinVolatilityLevel::High
        } else if volatility >= self.config.high_volatility_threshold {
            PinVolatilityLevel::Normal
        } else {
            PinVolatilityLevel::Low
        }
    }

    /// 计算当前应该使用的杠杆
    ///
    /// 返回 min(级别杠杆, 基础杠杆)
    pub fn calculate_leverage(
        &self,
        current_volatility: Decimal,
        base_leverage: Decimal,
    ) -> Decimal {
        let level = self.get_volatility_level(current_volatility);
        let level_leverage = self.config.get_leverage(level);
        level_leverage.min(base_leverage)
    }

    /// 是否应该降杠杆
    pub fn should_reduce_leverage(&self, current_volatility: Decimal) -> bool {
        current_volatility >= self.config.high_volatility_threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_volatility_level() {
        let guard = PinRiskLeverageGuard::new(PinLeverageConfig::default());

        // Low: < 3%
        assert_eq!(guard.get_volatility_level(dec!(0.01)), PinVolatilityLevel::Low);
        assert_eq!(guard.get_volatility_level(dec!(0.02)), PinVolatilityLevel::Low);

        // Normal: >= 3%
        assert_eq!(guard.get_volatility_level(dec!(0.03)), PinVolatilityLevel::Normal);
        assert_eq!(guard.get_volatility_level(dec!(0.04)), PinVolatilityLevel::Normal);

        // High: >= 4.5% (3% * 1.5)
        assert_eq!(guard.get_volatility_level(dec!(0.05)), PinVolatilityLevel::High);

        // Extreme: >= 6% (3% * 2)
        assert_eq!(guard.get_volatility_level(dec!(0.07)), PinVolatilityLevel::Extreme);
    }

    #[test]
    fn test_calculate_leverage() {
        let guard = PinRiskLeverageGuard::new(PinLeverageConfig::default());

        // 低波动: 使用 15x (vs base 10x = 10x)
        assert_eq!(guard.calculate_leverage(dec!(0.01), dec!(10)), dec!(10));

        // 高波动: 使用 5x (vs base 10x = 5x)
        assert_eq!(guard.calculate_leverage(dec!(0.05), dec!(10)), dec!(5));

        // 基础杠杆 3x，高波动 5x，取小值 = 3x
        assert_eq!(guard.calculate_leverage(dec!(0.05), dec!(3)), dec!(3));
    }

    #[test]
    fn test_should_reduce_leverage() {
        let guard = PinRiskLeverageGuard::new(PinLeverageConfig::default());

        assert!(!guard.should_reduce_leverage(dec!(0.01)));
        assert!(guard.should_reduce_leverage(dec!(0.03)));
    }
}
