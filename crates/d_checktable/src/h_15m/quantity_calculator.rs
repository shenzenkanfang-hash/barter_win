//! d_checktable/src/h_15m/quantity_calculator.rs
//! 分钟级策略数量计算器
//!
//! 对齐日线(l_1d)代码结构，独立实现数量计算逻辑。

#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use crate::types::{MinSignalInput, MinSignalOutput, VolatilityTier};
use x_data::trading::signal::{StrategySignal, StrategyId, PositionRef, PositionSide};

/// 分钟级策略数量配置
#[derive(Debug, Clone)]
pub struct MinQuantityConfig {
    /// 基础开仓数量
    pub base_open_qty: Decimal,
    /// 最大持仓数量
    pub max_position_qty: Decimal,
    /// 加仓倍数（相对于基础数量）
    pub add_multiplier: Decimal,
    /// 波动率调整启用
    pub vol_adjustment: bool,
}

impl Default for MinQuantityConfig {
    fn default() -> Self {
        Self {
            base_open_qty: dec!(0.05),      // 分钟级基础开仓 0.05 BTC（比日线小）
            max_position_qty: dec!(0.15),    // 分钟级最大持仓 0.15 BTC
            add_multiplier: dec!(1.5),       // 加仓 1.5 倍
            vol_adjustment: true,
        }
    }
}

/// 分钟级策略数量计算器
///
/// Pin策略与Trend策略共用此计算器，通过 StrategyId 区分策略类型
pub struct MinQuantityCalculator {
    config: MinQuantityConfig,
}

impl MinQuantityCalculator {
    pub fn new(config: MinQuantityConfig) -> Self {
        Self { config }
    }

    pub fn with_default() -> Self {
        Self::new(MinQuantityConfig::default())
    }

    /// 计算开仓数量
    pub fn calc_open_quantity(&self, vol_tier: &VolatilityTier) -> Decimal {
        let base = self.config.base_open_qty;
        if !self.config.vol_adjustment {
            return base;
        }
        match vol_tier {
            VolatilityTier::Low => base * dec!(1.2),
            VolatilityTier::Medium => base,
            VolatilityTier::High => base * dec!(0.8),
        }
    }

    /// 计算加仓数量
    pub fn calc_add_quantity(
        &self,
        current_position_qty: Decimal,
        vol_tier: &VolatilityTier,
    ) -> Decimal {
        let mut add_qty = self.config.base_open_qty * self.config.add_multiplier;

        // 检查是否会超过最大持仓
        let max_add = self.config.max_position_qty - current_position_qty;
        if add_qty > max_add {
            add_qty = max_add;
        }

        if !self.config.vol_adjustment {
            return add_qty;
        }

        // 根据波动率调整
        match vol_tier {
            VolatilityTier::Low => add_qty * dec!(1.2),
            VolatilityTier::Medium => add_qty,
            VolatilityTier::High => add_qty * dec!(0.7),
            // High波动率时不加仓（极端情况由High统一处理）
        }
    }

    /// 计算平仓数量
    ///
    /// 返回 (平仓数量, 是否全平)
    pub fn calc_close_quantity(
        &self,
        current_position_qty: Decimal,
        signal_output: &MinSignalOutput,
    ) -> (Decimal, bool) {
        // Exit信号 → 全平
        if signal_output.long_exit || signal_output.short_exit {
            return (current_position_qty, true);
        }

        // 高波动退出 → 全平
        if signal_output.exit_high_volatility {
            return (current_position_qty, true);
        }

        // 其他情况暂不处理，由引擎决定
        (Decimal::ZERO, false)
    }

    /// 生成完整的策略信号
    ///
    /// 优先级: Exit > Close > Hedge > Add > Open
    pub fn generate_signal(
        &self,
        _input: &MinSignalInput,
        signal_output: &MinSignalOutput,
        current_position_qty: Decimal,
        vol_tier: &VolatilityTier,
        strategy_id: StrategyId,
        position_ref: Option<PositionRef>,
    ) -> Option<StrategySignal> {
        // 1. Exit 信号（最高优先级）
        if signal_output.long_exit {
            return Some(StrategySignal::flat_all(
                strategy_id,
                position_ref?,
                "分钟多头退出".to_string(),
            ));
        }
        if signal_output.short_exit {
            return Some(StrategySignal::flat_all(
                strategy_id,
                position_ref?,
                "分钟空头退出".to_string(),
            ));
        }

        // 2. 高波动退出
        if signal_output.exit_high_volatility {
            return Some(StrategySignal::flat_all(
                strategy_id,
                position_ref?,
                "高波动退出".to_string(),
            ));
        }

        // 3. Hedge/Add 信号
        if signal_output.long_hedge || signal_output.short_hedge {
            let direction = if signal_output.long_hedge {
                PositionSide::Long
            } else {
                PositionSide::Short
            };
            let qty = self.calc_add_quantity(current_position_qty, vol_tier);
            if qty > Decimal::ZERO {
                return Some(StrategySignal::add(
                    direction,
                    qty,
                    Decimal::ZERO,
                    strategy_id,
                    position_ref?,
                    "分钟加仓".to_string(),
                ));
            }
        }

        // 4. Open 信号（最低优先级）
        if signal_output.long_entry || signal_output.short_entry {
            let direction = if signal_output.long_entry {
                PositionSide::Long
            } else {
                PositionSide::Short
            };
            let qty = self.calc_open_quantity(vol_tier);
            return Some(StrategySignal::open(
                direction,
                qty,
                Decimal::ZERO,
                strategy_id,
                "分钟开仓".to_string(),
            ));
        }

        None
    }
}

impl Default for MinQuantityCalculator {
    fn default() -> Self {
        Self::with_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = MinQuantityConfig::default();
        assert_eq!(config.base_open_qty, dec!(0.05));
        assert_eq!(config.max_position_qty, dec!(0.15));
    }

    #[test]
    fn test_calc_open_quantity() {
        let calc = MinQuantityCalculator::with_default();

        // 低波动多开
        let qty = calc.calc_open_quantity(&VolatilityTier::Low);
        assert_eq!(qty, dec!(0.05) * dec!(1.2));

        // 高波动少开
        let qty = calc.calc_open_quantity(&VolatilityTier::High);
        assert_eq!(qty, dec!(0.05) * dec!(0.8));
    }

    #[test]
    fn test_calc_add_quantity_with_limit() {
        let calc = MinQuantityCalculator::with_default();

        // 已有持仓接近上限时
        let qty = calc.calc_add_quantity(dec!(0.14), &VolatilityTier::Medium);
        // max_position_qty(0.15) - current(0.14) = 0.01
        assert_eq!(qty, dec!(0.01));
    }
}
