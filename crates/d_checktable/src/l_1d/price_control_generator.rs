#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use crate::types::{PriceControlInput, PriceControlOutput, PositionSide};

/// 日线级价格控制器
pub struct DayPriceControlGenerator;

impl Default for DayPriceControlGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl DayPriceControlGenerator {
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

        if entry <= Decimal::ZERO || current <= Decimal::ZERO {
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
