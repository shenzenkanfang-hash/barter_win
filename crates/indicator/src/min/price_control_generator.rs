#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use crate::types::{PriceControlInput, PriceControlOutput, PositionSide};

/// 分钟级价格控制器
pub struct MinPriceControlGenerator;

impl Default for MinPriceControlGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl MinPriceControlGenerator {
    pub fn new() -> Self {
        Self
    }

    /// 检查价格控制条件
    pub fn check(&self, input: &PriceControlInput) -> PriceControlOutput {
        let (profit_distance_pct, stop_distance_pct) = self.calculate_distances(input);

        let should_stop = self.check_stop(input, stop_distance_pct);
        let should_take_profit = self.check_take_profit(input, profit_distance_pct);
        let should_add = self.check_add(input, profit_distance_pct);
        let should_move_stop = self.check_move_stop(input, profit_distance_pct);

        PriceControlOutput {
            should_add,
            should_stop,
            should_take_profit,
            should_move_stop,
            profit_distance_pct,
            stop_distance_pct,
        }
    }

    /// 计算盈亏距离
    fn calculate_distances(&self, input: &PriceControlInput) -> (Decimal, Decimal) {
        if input.position_size <= Decimal::ZERO {
            return (Decimal::ZERO, Decimal::ZERO);
        }

        let entry = input.position_entry_price;
        let current = input.current_price;

        if entry <= Decimal::ZERO {
            return (Decimal::ZERO, Decimal::ZERO);
        }

        if current <= Decimal::ZERO {
            return (Decimal::ZERO, Decimal::ZERO);
        }

        match input.position_side {
            PositionSide::LONG => {
                let profit = (current - entry) / entry;
                let loss = (entry - current) / entry;
                (profit, loss)
            }
            PositionSide::SHORT => {
                let profit = (entry - current) / entry;
                let loss = (current - entry) / entry;
                (profit, loss)
            }
            PositionSide::NONE => (Decimal::ZERO, Decimal::ZERO),
        }
    }

    /// 检查止损
    fn check_stop(&self, input: &PriceControlInput, stop_distance: Decimal) -> bool {
        if input.position_size <= Decimal::ZERO {
            return false;
        }
        stop_distance >= input.loss_threshold
    }

    /// 检查止盈
    fn check_take_profit(&self, input: &PriceControlInput, profit_distance: Decimal) -> bool {
        if input.position_size <= Decimal::ZERO {
            return false;
        }
        profit_distance >= input.profit_threshold
    }

    /// 检查加仓
    fn check_add(&self, input: &PriceControlInput, profit_distance: Decimal) -> bool {
        if input.position_size <= Decimal::ZERO {
            return false;
        }
        profit_distance >= input.add_threshold
    }

    /// 检查移动止损
    fn check_move_stop(&self, input: &PriceControlInput, profit_distance: Decimal) -> bool {
        if input.position_size <= Decimal::ZERO {
            return false;
        }
        profit_distance >= input.move_stop_threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profit_distance_long() {
        let gen = MinPriceControlGenerator::new();
        let input = PriceControlInput {
            position_entry_price: dec!(100),
            position_side: PositionSide::LONG,
            position_size: dec!(1),
            current_price: dec!(102),
            profit_threshold: dec!(0.01),
            loss_threshold: dec!(0.02),
            add_threshold: dec!(0.04),
            move_stop_threshold: dec!(0.02),
        };

        let output = gen.check(&input);
        assert_eq!(output.profit_distance_pct, dec!(0.02));
        assert!(output.should_take_profit); // 2% > 1% 阈值
    }

    #[test]
    fn test_loss_distance_short() {
        let gen = MinPriceControlGenerator::new();
        let input = PriceControlInput {
            position_entry_price: dec!(100),
            position_side: PositionSide::SHORT,
            position_size: dec!(1),
            current_price: dec!(103),
            profit_threshold: dec!(0.01),
            loss_threshold: dec!(0.02),
            add_threshold: dec!(0.04),
            move_stop_threshold: dec!(0.02),
        };

        let output = gen.check(&input);
        assert_eq!(output.stop_distance_pct, dec!(0.03));
        assert!(output.should_stop); // 3% > 2% 止损阈值
    }

    #[test]
    fn test_no_position() {
        let gen = MinPriceControlGenerator::new();
        let input = PriceControlInput {
            position_entry_price: dec!(100),
            position_side: PositionSide::NONE,
            position_size: dec!(0),
            current_price: dec!(102),
            profit_threshold: dec!(0.01),
            loss_threshold: dec!(0.02),
            add_threshold: dec!(0.04),
            move_stop_threshold: dec!(0.02),
        };

        let output = gen.check(&input);
        assert!(!output.should_stop);
        assert!(!output.should_take_profit);
        assert!(!output.should_add);
    }
}
