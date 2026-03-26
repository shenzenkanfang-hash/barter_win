//! signal_generator.rs - 信号生成
//!
//! 使用市场数据 + 状态机 → 生成交易信号

#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use crate::types::{MinSignalInput, VolatilityTier};
use super::a_market_data::MarketData;
use super::{MinSignalGenerator, PinStatusMachine, PinStatus};
use x_data::position::PositionSide;
use x_data::trading::signal::{StrategySignal, TradeCommand, StrategyId};
use chrono::Utc;

/// 信号生成器
pub struct SignalGenerator {
    signal_gen: MinSignalGenerator,
    status_machine: PinStatusMachine,
}

impl SignalGenerator {
    pub fn new() -> Self {
        Self {
            signal_gen: MinSignalGenerator::new(),
            status_machine: PinStatusMachine::new(),
        }
    }

    /// 设置状态机状态
    pub fn set_status(&mut self, status: PinStatus) {
        self.status_machine.set_status(status);
    }

    /// 获取当前状态
    pub fn current_status(&self) -> PinStatus {
        self.status_machine.current_status()
    }

    /// 判断波动率通道
    fn volatility_tier(volatility: f64) -> VolatilityTier {
        if volatility > 0.15 {
            VolatilityTier::High
        } else if volatility > 0.05 {
            VolatilityTier::Medium
        } else {
            VolatilityTier::Low
        }
    }

    /// 构建信号输入
    fn build_input(market: &MarketData) -> MinSignalInput {
        MinSignalInput {
            tr_base_60min: dec!(0.1),      // TODO: 实际计算
            tr_ratio_15min: Decimal::from_f64_retain(market.volatility)
                .unwrap_or(Decimal::ZERO),
            zscore_14_1m: dec!(0),
            zscore_1h_1m: dec!(0),
            tr_ratio_60min_5h: dec!(0),
            tr_ratio_10min_1h: dec!(0),
            pos_norm_60: dec!(50),
            acc_percentile_1h: dec!(0),
            velocity_percentile_1h: dec!(0),
            pine_bg_color: String::new(),
            pine_bar_color: String::new(),
            price_deviation: dec!(0),
            price_deviation_horizontal_position: dec!(0),
        }
    }

    /// 生成交易信号
    pub fn generate(&self, market: &MarketData) -> Option<StrategySignal> {
        let input = Self::build_input(market);
        let vol_tier = Self::volatility_tier(market.volatility);
        let status = self.current_status();
        let price = market.price;

        // 生成信号
        let signal_output = self.signal_gen.generate(&input, &vol_tier, None);

        // 根据状态和信号决定动作
        self.decide_action(&status, &signal_output, market, price)
    }

    /// 决策动作
    fn decide_action(
        &self,
        status: &PinStatus,
        signal: &crate::types::MinSignalOutput,
        market: &MarketData,
        price: Decimal,
    ) -> Option<StrategySignal> {
        let symbol = &market.symbol;
        let vol = market.volatility;

        match status {
            // ========== 初始状态 ==========
            PinStatus::Initial | PinStatus::LongInitial | PinStatus::ShortInitial => {
                if signal.long_entry {
                    return Some(self.build_open_signal(symbol, PositionSide::Long, price));
                }
                if signal.short_entry {
                    return Some(self.build_open_signal(symbol, PositionSide::Short, price));
                }
            }

            // ========== 多头状态 ==========
            PinStatus::LongFirstOpen | PinStatus::LongDoubleAdd => {
                if signal.long_entry {
                    return Some(self.build_add_signal(symbol, PositionSide::Long, vol));
                }
                if signal.long_exit {
                    return Some(self.build_close_signal(symbol, PositionSide::Long));
                }
                if signal.long_hedge {
                    return Some(self.build_hedge_signal(symbol, PositionSide::Long, vol));
                }
            }

            // ========== 空头状态 ==========
            PinStatus::ShortFirstOpen | PinStatus::ShortDoubleAdd => {
                if signal.short_entry {
                    return Some(self.build_add_signal(symbol, PositionSide::Short, vol));
                }
                if signal.short_exit {
                    return Some(self.build_close_signal(symbol, PositionSide::Short));
                }
                if signal.short_hedge {
                    return Some(self.build_hedge_signal(symbol, PositionSide::Short, vol));
                }
            }

            // ========== 对冲状态 ==========
            PinStatus::HedgeEnter => {
                if signal.exit_high_volatility {
                    // 锁定仓位
                }
            }

            PinStatus::PosLocked | PinStatus::LongDayAllow | PinStatus::ShortDayAllow => {
                // TODO: 日线方向决策
            }
        }

        None
    }

    /// 构建开仓信号
    fn build_open_signal(&self, symbol: &str, side: PositionSide, price: Decimal) -> StrategySignal {
        let qty = dec!(0.05); // TODO: 计算数量

        StrategySignal {
            command: TradeCommand::Open,
            direction: side,
            quantity: qty,
            target_price: price,
            strategy_id: StrategyId::new_pin_minute(symbol),
            position_ref: None,
            full_close: false,
            stop_loss_price: None,
            take_profit_price: None,
            reason: format!("Open {:?} signal", side),
            confidence: 80,
            timestamp: Utc::now().timestamp(),
        }
    }

    /// 构建加仓信号
    fn build_add_signal(&self, symbol: &str, side: PositionSide, _volatility: f64) -> StrategySignal {
        let qty = dec!(0.05); // TODO: 计算数量

        StrategySignal {
            command: TradeCommand::Add,
            direction: side,
            quantity: qty,
            target_price: Decimal::ZERO,
            strategy_id: StrategyId::new_pin_minute(symbol),
            position_ref: None,
            full_close: false,
            stop_loss_price: None,
            take_profit_price: None,
            reason: format!("Add {:?} position", side),
            confidence: 70,
            timestamp: Utc::now().timestamp(),
        }
    }

    /// 构建平仓信号
    fn build_close_signal(&self, symbol: &str, side: PositionSide) -> StrategySignal {
        StrategySignal {
            command: TradeCommand::FlatPosition,
            direction: side,
            quantity: Decimal::ZERO, // 全平
            target_price: Decimal::ZERO,
            strategy_id: StrategyId::new_pin_minute(symbol),
            position_ref: None,
            full_close: true,
            stop_loss_price: None,
            take_profit_price: None,
            reason: format!("Close {:?} position", side),
            confidence: 90,
            timestamp: Utc::now().timestamp(),
        }
    }

    /// 构建对冲信号
    fn build_hedge_signal(&self, symbol: &str, existing_side: PositionSide, _volatility: f64) -> StrategySignal {
        let hedge_side = match existing_side {
            PositionSide::Long => PositionSide::Short,
            PositionSide::Short => PositionSide::Long,
            _ => PositionSide::Long,
        };
        let qty = dec!(0.03); // TODO: 计算数量

        StrategySignal {
            command: TradeCommand::HedgeOpen,
            direction: hedge_side,
            quantity: qty,
            target_price: Decimal::ZERO,
            strategy_id: StrategyId::new_pin_minute(symbol),
            position_ref: None,
            full_close: false,
            stop_loss_price: None,
            take_profit_price: None,
            reason: format!("Hedge {:?}", hedge_side),
            confidence: 60,
            timestamp: Utc::now().timestamp(),
        }
    }
}

impl Default for SignalGenerator {
    fn default() -> Self {
        Self::new()
    }
}
