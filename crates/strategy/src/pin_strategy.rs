use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

/// PinStrategy 状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PinState {
    /// 等待入场
    Idle,
    /// 多头马丁加仓中
    LongOpening,
    /// 空头马丁加仓中
    ShortOpening,
    /// 多头持仓中
    LongHolding,
    /// 空头持仓中
    ShortHolding,
    /// 对冲中
    Hedging,
    /// 平仓中
    Closing,
}

/// PinStrategy 信号
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PinSignal {
    /// 做多开仓
    LongEntry,
    /// 做空开仓
    ShortEntry,
    /// 多头加仓
    LongAdd,
    /// 空头加仓
    ShortAdd,
    /// 对冲
    Hedge,
    /// 平仓
    Close,
    /// 全平
    CloseAll,
    /// 观望
    Watch,
}

/// PinStrategy 配置
#[derive(Debug, Clone)]
pub struct PinStrategyConfig {
    /// 加仓阈值 (价格偏离百分比)
    pub add_threshold: Decimal,
    /// 对冲阈值
    pub hedge_threshold: Decimal,
    /// 止损阈值
    pub stop_loss_threshold: Decimal,
    /// 盈利平仓阈值
    pub profit_threshold: Decimal,
    /// 最大加仓次数
    pub max_add_count: u8,
    /// Z-Score 极端阈值
    pub zscore_extreme: Decimal,
    /// TR-Ratio 极端阈值
    pub tr_ratio_extreme: Decimal,
    /// 价格位置极端阈值
    pub price_pos_extreme: Decimal,
}

impl Default for PinStrategyConfig {
    fn default() -> Self {
        Self {
            add_threshold: dec!(0.02),      // 2% 价格偏离
            hedge_threshold: dec!(0.05),     // 5% 下跌
            stop_loss_threshold: dec!(0.10), // 10% 止损
            profit_threshold: dec!(0.01),    // 1% 盈利
            max_add_count: 5,
            zscore_extreme: dec!(2.0),
            tr_ratio_extreme: dec!(1.0),
            price_pos_extreme: dec!(90),
        }
    }
}

/// PinStrategy 马丁/插针策略
///
/// 基于 Z-Score、TR-Ratio、价格位置等指标的马丁格尔策略。
///
/// 状态机:
///
///     IDLE ──(做多)──> LONG_OPENING ──(加仓)──> LONG_HOLDING
///         │                    │
///         │                    │
///         └──(做空)──> SHORT_OPENING ──(加仓)──> SHORT_HOLDING
///                                                     │
///                         ──(对冲)──> HEDGING <───────┘
///                                         │
///                         ──(盈利回落)──> CLOSING ──(全平)──> IDLE
///
/// 设计依据: 设计文档 16.10.2
pub struct PinStrategy {
    /// 策略ID
    strategy_id: String,
    /// 当前状态
    state: PinState,
    /// 配置
    config: PinStrategyConfig,
    /// 加仓次数
    add_count: u8,
    /// 对冲次数
    hedge_count: u8,
    /// 入场价格
    entry_price: Decimal,
    /// 持仓数量
    position_qty: Decimal,
}

impl PinStrategy {
    /// 创建新的 PinStrategy
    pub fn new(strategy_id: &str) -> Self {
        Self {
            strategy_id: strategy_id.to_string(),
            state: PinState::Idle,
            config: PinStrategyConfig::default(),
            add_count: 0,
            hedge_count: 0,
            entry_price: dec!(0),
            position_qty: dec!(0),
        }
    }

    /// 创建带配置的 PinStrategy
    pub fn with_config(strategy_id: &str, config: PinStrategyConfig) -> Self {
        Self {
            strategy_id: strategy_id.to_string(),
            state: PinState::Idle,
            config,
            add_count: 0,
            hedge_count: 0,
            entry_price: dec!(0),
            position_qty: dec!(0),
        }
    }

    /// 判断信号
    ///
    /// 输入指标数据，返回交易信号。
    pub fn check_signal(
        &self,
        zscore: Decimal,
        tr_ratio: Decimal,
        price_position: Decimal,
        velocity_percentile: Decimal,
        pine_bar_color: &str,
        pine_bg_color: &str,
        current_price: Decimal,
    ) -> PinSignal {
        match self.state {
            PinState::Idle => {
                self.check_entry_signal(zscore, tr_ratio, price_position, velocity_percentile, pine_bar_color, pine_bg_color)
            }
            PinState::LongOpening | PinState::LongHolding => {
                self.check_long_signal(zscore, tr_ratio, price_position, velocity_percentile, pine_bar_color, pine_bg_color, current_price)
            }
            PinState::ShortOpening | PinState::ShortHolding => {
                self.check_short_signal(zscore, tr_ratio, price_position, velocity_percentile, pine_bar_color, pine_bg_color, current_price)
            }
            PinState::Hedging => {
                self.check_hedge_signal(tr_ratio, velocity_percentile, pine_bg_color, current_price)
            }
            PinState::Closing => PinSignal::Watch,
        }
    }

    /// 判断入场信号
    fn check_entry_signal(
        &self,
        zscore: Decimal,
        tr_ratio: Decimal,
        price_position: Decimal,
        velocity_percentile: Decimal,
        pine_bar_color: &str,
        pine_bg_color: &str,
    ) -> PinSignal {
        // 统计满足的条件数
        let mut conditions_met: u8 = 0;

        // 条件1: Z-Score 极端 (> 2 或 < -2)
        if zscore.abs() > self.config.zscore_extreme {
            conditions_met += 1;
        }

        // 条件2: TR-Ratio 极端 (> 1)
        if tr_ratio > self.config.tr_ratio_extreme {
            conditions_met += 1;
        }

        // 条件3: 价格位置极端
        if price_position < dec!(20) || price_position > self.config.price_pos_extreme {
            conditions_met += 1;
        }

        // 条件4: 速度百分位极端
        if velocity_percentile.abs() > dec!(90) {
            conditions_met += 1;
        }

        // 条件5: Pine 颜色
        if pine_bar_color == "green" || pine_bg_color == "green" {
            conditions_met += 1;
        }

        // 条件6: Pine 颜色红色
        if pine_bar_color == "red" || pine_bg_color == "red" {
            conditions_met += 1;
        }

        // 7 条件满足 >= 4
        if conditions_met >= 4 {
            if price_position < dec!(30) {
                return PinSignal::LongEntry;
            } else if price_position > self.config.price_pos_extreme {
                return PinSignal::ShortEntry;
            }
        }

        PinSignal::Watch
    }

    /// 判断多头信号
    fn check_long_signal(
        &self,
        zscore: Decimal,
        tr_ratio: Decimal,
        price_position: Decimal,
        velocity_percentile: Decimal,
        pine_bar_color: &str,
        pine_bg_color: &str,
        current_price: Decimal,
    ) -> PinSignal {
        // 计算盈亏
        let pnl_ratio = if self.entry_price > dec!(0) {
            (current_price - self.entry_price) / self.entry_price
        } else {
            dec!(0)
        };

        // 盈利达到目标，平仓
        if pnl_ratio >= self.config.profit_threshold {
            return PinSignal::Close;
        }

        // 止损
        if pnl_ratio <= -self.config.stop_loss_threshold {
            return PinSignal::CloseAll;
        }

        // 对冲条件 (下跌超过阈值)
        if pnl_ratio <= -self.config.hedge_threshold {
            return PinSignal::Hedge;
        }

        // 加仓条件 (可以加仓且价格继续下跌)
        if self.add_count < self.config.max_add_count && pnl_ratio < dec!(0) {
            // 满足加仓条件
            let mut add_conditions: u8 = 0;

            if zscore < -self.config.zscore_extreme { add_conditions += 1; }
            if tr_ratio > self.config.tr_ratio_extreme { add_conditions += 1; }
            if price_position < dec!(30) { add_conditions += 1; }
            if pine_bar_color == "green" { add_conditions += 1; }

            if add_conditions >= 3 {
                return PinSignal::LongAdd;
            }
        }

        PinSignal::Watch
    }

    /// 判断空头信号
    fn check_short_signal(
        &self,
        zscore: Decimal,
        tr_ratio: Decimal,
        price_position: Decimal,
        velocity_percentile: Decimal,
        pine_bar_color: &str,
        pine_bg_color: &str,
        current_price: Decimal,
    ) -> PinSignal {
        // 计算盈亏
        let pnl_ratio = if self.entry_price > dec!(0) {
            (self.entry_price - current_price) / self.entry_price
        } else {
            dec!(0)
        };

        // 盈利达到目标，平仓
        if pnl_ratio >= self.config.profit_threshold {
            return PinSignal::Close;
        }

        // 止损
        if pnl_ratio <= -self.config.stop_loss_threshold {
            return PinSignal::CloseAll;
        }

        // 对冲条件 (上涨超过阈值)
        if pnl_ratio <= -self.config.hedge_threshold {
            return PinSignal::Hedge;
        }

        // 加仓条件
        if self.add_count < self.config.max_add_count && pnl_ratio < dec!(0) {
            let mut add_conditions: u8 = 0;

            if zscore > self.config.zscore_extreme { add_conditions += 1; }
            if tr_ratio > self.config.tr_ratio_extreme { add_conditions += 1; }
            if price_position > self.config.price_pos_extreme { add_conditions += 1; }
            if pine_bar_color == "red" { add_conditions += 1; }

            if add_conditions >= 3 {
                return PinSignal::ShortAdd;
            }
        }

        PinSignal::Watch
    }

    /// 判断对冲信号
    fn check_hedge_signal(
        &self,
        tr_ratio: Decimal,
        velocity_percentile: Decimal,
        pine_bg_color: &str,
        current_price: Decimal,
    ) -> PinSignal {
        // 计算盈亏比
        let pnl_ratio = if self.entry_price > dec!(0) {
            (current_price - self.entry_price) / self.entry_price
        } else {
            dec!(0)
        };

        // 盈利回落 1%，全平
        if pnl_ratio > dec!(0) && pnl_ratio < self.config.profit_threshold * dec!(0.1) {
            return PinSignal::CloseAll;
        }

        // 继续持有对冲仓位
        PinSignal::Watch
    }

    /// 更新状态 (根据信号和操作)
    pub fn update_state(&mut self, signal: PinSignal, qty: Decimal, price: Decimal) {
        match (self.state, signal) {
            // 入场
            (PinState::Idle, PinSignal::LongEntry) => {
                self.state = PinState::LongOpening;
                self.entry_price = price;
                self.position_qty = qty;
                self.add_count = 0;
            }
            (PinState::Idle, PinSignal::ShortEntry) => {
                self.state = PinState::ShortOpening;
                self.entry_price = price;
                self.position_qty = qty;
                self.add_count = 0;
            }
            // 加仓
            (PinState::LongOpening, PinSignal::LongAdd) | (PinState::LongHolding, PinSignal::LongAdd) => {
                self.position_qty += qty;
                self.add_count += 1;
                if self.add_count >= 1 {
                    self.state = PinState::LongHolding;
                }
            }
            (PinState::ShortOpening, PinSignal::ShortAdd) | (PinState::ShortHolding, PinSignal::ShortAdd) => {
                self.position_qty += qty;
                self.add_count += 1;
                if self.add_count >= 1 {
                    self.state = PinState::ShortHolding;
                }
            }
            // 对冲
            (PinState::LongHolding, PinSignal::Hedge) | (PinState::ShortHolding, PinSignal::Hedge) => {
                self.state = PinState::Hedging;
                self.hedge_count += 1;
            }
            // 平仓
            (PinState::LongHolding, PinSignal::Close) | (PinState::ShortHolding, PinSignal::Close) => {
                self.state = PinState::Closing;
            }
            // 全平
            (_, PinSignal::CloseAll) => {
                self.reset();
            }
            _ => {}
        }
    }

    /// 获取当前状态
    pub fn state(&self) -> PinState {
        self.state
    }

    /// 获取策略ID
    pub fn strategy_id(&self) -> &str {
        &self.strategy_id
    }

    /// 获取加仓次数
    pub fn add_count(&self) -> u8 {
        self.add_count
    }

    /// 获取入场价格
    pub fn entry_price(&self) -> Decimal {
        self.entry_price
    }

    /// 获取持仓数量
    pub fn position_qty(&self) -> Decimal {
        self.position_qty
    }

    /// 重置策略状态
    pub fn reset(&mut self) {
        self.state = PinState::Idle;
        self.add_count = 0;
        self.hedge_count = 0;
        self.entry_price = dec!(0);
        self.position_qty = dec!(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pin_strategy_idle() {
        let strategy = PinStrategy::new("pin_sol");
        assert_eq!(strategy.state(), PinState::Idle);
    }

    #[test]
    fn test_pin_strategy_entry() {
        let mut strategy = PinStrategy::new("pin_sol");

        // 满足做多条件
        let signal = strategy.check_signal(
            dec!(-2.5),   // zscore (极端)
            dec!(1.5),     // tr_ratio (极端)
            dec!(15),      // price_position (底部极端)
            dec!(-95),     // velocity_percentile (极端)
            "green",       // pine_bar_color
            "green",       // pine_bg_color
            dec!(100),     // current_price
        );

        assert_eq!(signal, PinSignal::LongEntry);
    }

    #[test]
    fn test_pin_strategy_stop_loss() {
        let mut strategy = PinStrategy::new("pin_sol");

        // 先入场
        strategy.update_state(PinSignal::LongEntry, dec!(1), dec!(100));
        assert_eq!(strategy.state(), PinState::LongOpening);

        // 触发止损
        let signal = strategy.check_signal(
            dec!(0),
            dec!(0.5),
            dec!(50),
            dec!(0),
            "red",
            "green",
            dec!(88),  // 下跌 12%，超过 10% 止损
        );

        assert_eq!(signal, PinSignal::CloseAll);
    }

    #[test]
    fn test_pin_strategy_add() {
        let mut strategy = PinStrategy::new("pin_sol");

        // 入场
        strategy.update_state(PinSignal::LongEntry, dec!(1), dec!(100));
        assert_eq!(strategy.state(), PinState::LongOpening);

        // 价格下跌，触发加仓
        let signal = strategy.check_signal(
            dec!(-2.5),   // zscore 极端
            dec!(1.5),     // tr_ratio 极端
            dec!(10),      // price_position 底部
            dec!(-90),     // velocity 极端
            "green",       // pine_bar_color
            "green",       // pine_bg_color
            dec!(98),      // 价格小跌
        );

        assert_eq!(signal, PinSignal::LongAdd);
    }
}
