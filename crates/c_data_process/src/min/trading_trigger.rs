#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use crate::types::{
    TradingTriggerInput, TradingDecision, TradingAction, StrategyLevel, VolatilityLevel,
    MinSignalOutput, DaySignalOutput, PriceControlOutput, PriceControlInput,
};
use super::{
    MinMarketStatusGenerator, MinSignalGenerator, MinPriceControlGenerator,
};
use crate::day::{
    DayMarketStatusGenerator, DaySignalGenerator, DayPriceControlGenerator,
};

/// 交易触发器
pub struct TradingTrigger {
    min_status_gen: MinMarketStatusGenerator,
    min_signal_gen: MinSignalGenerator,
    min_price_ctrl: MinPriceControlGenerator,

    day_status_gen: DayMarketStatusGenerator,
    day_signal_gen: DaySignalGenerator,
    day_price_ctrl: DayPriceControlGenerator,
}

impl Default for TradingTrigger {
    fn default() -> Self {
        Self::new()
    }
}

impl TradingTrigger {
    pub fn new() -> Self {
        Self {
            min_status_gen: MinMarketStatusGenerator::new(),
            min_signal_gen: MinSignalGenerator::new(),
            min_price_ctrl: MinPriceControlGenerator::new(),
            day_status_gen: DayMarketStatusGenerator::new(),
            day_signal_gen: DaySignalGenerator::new(),
            day_price_ctrl: DayPriceControlGenerator::new(),
        }
    }

    /// 执行交易决策
    pub fn run(&mut self, input: &TradingTriggerInput) -> TradingDecision {
        // 1. 计算波动率等级
        let vol_level = self.calculate_volatility_level(input);

        // 2. 根据波动率选择策略
        match vol_level {
            VolatilityLevel::HIGH => self.run_min_strategy(input),
            _ => self.run_day_strategy(input),
        }
    }

    /// 计算波动率等级
    fn calculate_volatility_level(&self, input: &TradingTriggerInput) -> VolatilityLevel {
        let tr_15min = input.min_indicators.tr_ratio_15min;

        if tr_15min > dec!(0.13) {
            VolatilityLevel::HIGH
        } else if tr_15min < dec!(0.03) {
            VolatilityLevel::LOW
        } else {
            VolatilityLevel::NORMAL
        }
    }

    /// 运行分钟级策略
    fn run_min_strategy(&self, input: &TradingTriggerInput) -> TradingDecision {
        // 1. SignalGenerator
        let signal = self.min_signal_gen.generate(&input.min_indicators, &VolatilityLevel::HIGH);

        // 2. PriceControlGenerator
        let price_ctrl = self.make_price_control_input(input);
        let price_ctrl_output = self.min_price_ctrl.check(&price_ctrl);

        // 3. 综合决策
        self.make_decision_min(&signal, &price_ctrl_output)
    }

    /// 运行日线级策略
    fn run_day_strategy(&self, input: &TradingTriggerInput) -> TradingDecision {
        // 1. SignalGenerator
        let signal = self.day_signal_gen.generate(&input.day_indicators);

        // 2. PriceControlGenerator
        let price_ctrl = self.make_price_control_input(input);
        let price_ctrl_output = self.day_price_ctrl.check(&price_ctrl);

        // 3. 综合决策
        self.make_decision_day(&signal, &price_ctrl_output)
    }

    /// 构建价格控制输入
    fn make_price_control_input(&self, input: &TradingTriggerInput) -> PriceControlInput {
        // 从 check_list 获取最新持仓
        let (entry_price, side, size) = if !input.check_list.long_positions.is_empty() {
            let pos = &input.check_list.long_positions[0];
            (pos.entry_price, crate::types::PositionSide::LONG, pos.qty)
        } else if !input.check_list.short_positions.is_empty() {
            let pos = &input.check_list.short_positions[0];
            (pos.entry_price, crate::types::PositionSide::SHORT, pos.qty)
        } else {
            (Decimal::ZERO, crate::types::PositionSide::NONE, Decimal::ZERO)
        };

        PriceControlInput {
            position_entry_price: entry_price,
            position_side: side,
            position_size: size,
            current_price: input.current_price,
            profit_threshold: dec!(0.01),
            loss_threshold: dec!(0.02),
            add_threshold: dec!(0.04),
            move_stop_threshold: dec!(0.02),
        }
    }

    /// 综合分钟级决策
    fn make_decision_min(&self, signal: &MinSignalOutput, price_ctrl: &PriceControlOutput) -> TradingDecision {
        // 优先级: 止损 > 止盈 > 对冲 > 开仓 > 等待

        if price_ctrl.should_stop {
            return TradingDecision {
                action: TradingAction::Flat,
                reason: "min_stop_loss".to_string(),
                confidence: 100,
                level: StrategyLevel::MIN,
            };
        }

        if price_ctrl.should_take_profit {
            return TradingDecision {
                action: TradingAction::Flat,
                reason: "min_take_profit".to_string(),
                confidence: 95,
                level: StrategyLevel::MIN,
            };
        }

        if signal.long_hedge {
            return TradingDecision {
                action: TradingAction::Hedge,
                reason: "min_long_hedge".to_string(),
                confidence: 80,
                level: StrategyLevel::MIN,
            };
        }

        if signal.short_hedge {
            return TradingDecision {
                action: TradingAction::Hedge,
                reason: "min_short_hedge".to_string(),
                confidence: 80,
                level: StrategyLevel::MIN,
            };
        }

        if signal.long_entry {
            return TradingDecision {
                action: TradingAction::Long,
                reason: "min_long_entry".to_string(),
                confidence: 75,
                level: StrategyLevel::MIN,
            };
        }

        if signal.short_entry {
            return TradingDecision {
                action: TradingAction::Short,
                reason: "min_short_entry".to_string(),
                confidence: 75,
                level: StrategyLevel::MIN,
            };
        }

        TradingDecision {
            action: TradingAction::Wait,
            reason: "min_no_signal".to_string(),
            confidence: 0,
            level: StrategyLevel::MIN,
        }
    }

    /// 综合日线级决策
    fn make_decision_day(&self, signal: &DaySignalOutput, price_ctrl: &PriceControlOutput) -> TradingDecision {
        if price_ctrl.should_stop {
            return TradingDecision {
                action: TradingAction::Flat,
                reason: "day_stop_loss".to_string(),
                confidence: 100,
                level: StrategyLevel::DAY,
            };
        }

        if price_ctrl.should_take_profit {
            return TradingDecision {
                action: TradingAction::Flat,
                reason: "day_take_profit".to_string(),
                confidence: 95,
                level: StrategyLevel::DAY,
            };
        }

        if signal.long_hedge {
            return TradingDecision {
                action: TradingAction::Hedge,
                reason: "day_long_hedge".to_string(),
                confidence: 80,
                level: StrategyLevel::DAY,
            };
        }

        if signal.short_hedge {
            return TradingDecision {
                action: TradingAction::Hedge,
                reason: "day_short_hedge".to_string(),
                confidence: 80,
                level: StrategyLevel::DAY,
            };
        }

        if signal.long_exit {
            return TradingDecision {
                action: TradingAction::Flat,
                reason: "day_long_exit".to_string(),
                confidence: 85,
                level: StrategyLevel::DAY,
            };
        }

        if signal.short_exit {
            return TradingDecision {
                action: TradingAction::Flat,
                reason: "day_short_exit".to_string(),
                confidence: 85,
                level: StrategyLevel::DAY,
            };
        }

        if signal.long_entry {
            return TradingDecision {
                action: TradingAction::Long,
                reason: "day_long_entry".to_string(),
                confidence: 70,
                level: StrategyLevel::DAY,
            };
        }

        if signal.short_entry {
            return TradingDecision {
                action: TradingAction::Short,
                reason: "day_short_entry".to_string(),
                confidence: 70,
                level: StrategyLevel::DAY,
            };
        }

        TradingDecision {
            action: TradingAction::Wait,
            reason: "day_no_signal".to_string(),
            confidence: 0,
            level: StrategyLevel::DAY,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_input() -> TradingTriggerInput {
        TradingTriggerInput {
            symbol: "BTCUSDT".to_string(),
            current_price: dec!(49000),  // 设为 entry_price 附近，避免触发止盈
            high: dec!(50500),
            low: dec!(49500),
            close: dec!(50000),
            min_indicators: crate::types::MinSignalInput {
                tr_base_60min: dec!(0.16),
                tr_ratio_15min: dec!(0.15),
                zscore_14_1m: dec!(2.5),
                zscore_1h_1m: dec!(1.0),
                tr_ratio_60min_5h: dec!(1.2),
                tr_ratio_10min_1h: dec!(0.8),
                pos_norm_60: dec!(95),
                acc_percentile_1h: dec!(92),
                pine_bg_color: "纯绿".to_string(),
                pine_bar_color: "纯绿".to_string(),
                price_deviation: dec!(-0.02),
                price_deviation_horizontal_position: dec!(100),
                velocity_percentile_1h: dec!(95),
            },
            day_indicators: crate::types::DaySignalInput {
                pine_bar_color_100_200: "纯绿".to_string(),
                pine_bg_color_100_200: "纯绿".to_string(),
                pine_bar_color_20_50: "纯绿".to_string(),
                pine_bg_color_20_50: "纯绿".to_string(),
                pine_bar_color_12_26: "纯绿".to_string(),
                pine_bg_color_12_26: "纯绿".to_string(),
                tr_ratio_5d_20d: dec!(1.5),
                tr_ratio_20d_60d: dec!(1.2),
                ma5_in_20d_ma5_pos: dec!(75),
            },
            check_list: crate::types::CheckList {
                long_positions: vec![crate::types::PositionRecord {
                    entry_price: dec!(49000),
                    qty: dec!(0.1),
                }],
                short_positions: vec![],
            },
        }
    }

    #[test]
    fn test_high_volatility_triggers_min_strategy() {
        let mut trigger = TradingTrigger::new();
        let input = create_test_input();

        let decision = trigger.run(&input);

        assert_eq!(decision.level, StrategyLevel::MIN);
        assert_eq!(decision.action, TradingAction::Long); // long_entry signal
    }

    #[test]
    fn test_stop_loss_priority() {
        let mut trigger = TradingTrigger::new();
        let mut input = create_test_input();

        // 设置亏损超过阈值
        input.current_price = dec!(47000); // 从 49000 跌到 47000, 亏损 > 4%

        let decision = trigger.run(&input);

        assert_eq!(decision.action, TradingAction::Flat);
        assert!(decision.reason.contains("stop"));
    }
}
