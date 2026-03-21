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
/// IDLE --(做多)--> LONG_OPENING --(加仓)--> LONG_HOLDING
///     |                    |
///     |                    |
///     +--(做空)--> SHORT_OPENING --(加仓)--> SHORT_HOLDING
///                                                     |
///                         --(对冲)--> HEDGING <-------+
///                                         |
///                         --(盈利回落)--> CLOSING --(全平)--> IDLE
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

    // ============================================
    // E2.2 PinStrategy 状态机测试
    // ============================================

    /// 测试完整的多头马丁周期: Idle -> LongOpening -> LongHolding -> Closing -> Idle
    #[test]
    fn test_pin_state_machine_long_martin_cycle() {
        let mut strategy = PinStrategy::new("pin_sol");

        // 初始状态: Idle
        assert_eq!(strategy.state(), PinState::Idle);
        assert_eq!(strategy.add_count(), 0);

        // 场景1: Idle -> LongOpening (做多入场)
        let entry_signal = strategy.check_signal(
            dec!(-2.5),   // zscore 极端
            dec!(1.5),     // tr_ratio 极端
            dec!(15),      // price_position 底部
            dec!(-95),     // velocity 极端
            "green",       // pine_bar_color
            "green",       // pine_bg_color
            dec!(100),     // current_price
        );
        assert_eq!(entry_signal, PinSignal::LongEntry);
        strategy.update_state(entry_signal, dec!(1), dec!(100));
        assert_eq!(strategy.state(), PinState::LongOpening);
        assert_eq!(strategy.entry_price(), dec!(100));
        assert_eq!(strategy.position_qty(), dec!(1));

        // 场景2: LongOpening -> LongHolding (加仓后转入持仓)
        let add_signal = strategy.check_signal(
            dec!(-2.5),   // zscore 极端
            dec!(1.5),     // tr_ratio 极端
            dec!(10),      // price_position 底部
            dec!(-90),     // velocity 极端
            "green",       // pine_bar_color
            "green",       // pine_bg_color
            dec!(98),      // 价格下跌，触发加仓
        );
        assert_eq!(add_signal, PinSignal::LongAdd);
        strategy.update_state(add_signal, dec!(1), dec!(98));
        assert_eq!(strategy.state(), PinState::LongHolding);
        assert_eq!(strategy.add_count(), 1);
        assert_eq!(strategy.position_qty(), dec!(2)); // 1 + 1

        // 场景3: LongHolding -> Closing (盈利平仓)
        let close_signal = strategy.check_signal(
            dec!(0),
            dec!(0.5),
            dec!(50),
            dec!(0),
            "green",
            "green",
            dec!(101),  // 上涨 1%，达到盈利阈值
        );
        assert_eq!(close_signal, PinSignal::Close);
        strategy.update_state(close_signal, dec!(0), dec!(0));
        assert_eq!(strategy.state(), PinState::Closing);

        // 场景4: Closing 状态下收到 Watch (等待最后确认)
        let watch_signal = strategy.check_signal(
            dec!(0),
            dec!(0.5),
            dec!(50),
            dec!(0),
            "neutral",
            "neutral",
            dec!(101),
        );
        assert_eq!(watch_signal, PinSignal::Watch);
    }

    /// 测试完整的空头马丁周期: Idle -> ShortOpening -> ShortHolding -> Closing -> Idle
    #[test]
    fn test_pin_state_machine_short_martin_cycle() {
        let mut strategy = PinStrategy::new("pin_sol");

        // 初始状态: Idle
        assert_eq!(strategy.state(), PinState::Idle);

        // 场景1: Idle -> ShortOpening (做空入场)
        let entry_signal = strategy.check_signal(
            dec!(2.5),     // zscore 极端
            dec!(1.5),     // tr_ratio 极端
            dec!(95),      // price_position 顶部
            dec!(95),      // velocity 极端
            "red",         // pine_bar_color
            "red",         // pine_bg_color
            dec!(100),     // current_price
        );
        assert_eq!(entry_signal, PinSignal::ShortEntry);
        strategy.update_state(entry_signal, dec!(1), dec!(100));
        assert_eq!(strategy.state(), PinState::ShortOpening);
        assert_eq!(strategy.entry_price(), dec!(100));

        // 场景2: ShortOpening -> ShortHolding (加仓后转入持仓)
        let add_signal = strategy.check_signal(
            dec!(2.5),     // zscore 极端
            dec!(1.5),     // tr_ratio 极端
            dec!(90),      // price_position 顶部
            dec!(90),     // velocity 极端
            "red",         // pine_bar_color
            "red",         // pine_bg_color
            dec!(102),     // 价格上涨，触发加仓
        );
        assert_eq!(add_signal, PinSignal::ShortAdd);
        strategy.update_state(add_signal, dec!(1), dec!(102));
        assert_eq!(strategy.state(), PinState::ShortHolding);
        assert_eq!(strategy.add_count(), 1);

        // 场景3: ShortHolding -> Closing (盈利平仓)
        let close_signal = strategy.check_signal(
            dec!(0),
            dec!(0.5),
            dec!(50),
            dec!(0),
            "red",
            "red",
            dec!(99),  // 下跌 1%，达到盈利阈值
        );
        assert_eq!(close_signal, PinSignal::Close);
        strategy.update_state(close_signal, dec!(0), dec!(0));
        assert_eq!(strategy.state(), PinState::Closing);
    }

    /// 测试对冲状态转换: LongHolding -> Hedging
    #[test]
    fn test_pin_state_machine_hedging() {
        let mut strategy = PinStrategy::new("pin_sol");

        // 入场 Long
        strategy.update_state(PinSignal::LongEntry, dec!(1), dec!(100));
        assert_eq!(strategy.state(), PinState::LongOpening);

        // 转入持仓
        strategy.update_state(PinSignal::LongAdd, dec!(1), dec!(95));
        assert_eq!(strategy.state(), PinState::LongHolding);

        // 场景: 亏损超过对冲阈值，触发对冲
        let hedge_signal = strategy.check_signal(
            dec!(-2.0),
            dec!(1.5),
            dec!(20),
            dec!(-80),
            "green",
            "green",
            dec!(93),  // 亏损 7%，超过 5% 对冲阈值
        );
        assert_eq!(hedge_signal, PinSignal::Hedge);
        strategy.update_state(hedge_signal, dec!(0), dec!(0));
        assert_eq!(strategy.state(), PinState::Hedging);
    }

    /// 测试 CloseAll 全平重置状态
    #[test]
    fn test_pin_state_machine_close_all_reset() {
        let mut strategy = PinStrategy::new("pin_sol");

        // 入场 Long 并加仓
        strategy.update_state(PinSignal::LongEntry, dec!(1), dec!(100));
        strategy.update_state(PinSignal::LongAdd, dec!(1), dec!(95));
        assert_eq!(strategy.state(), PinState::LongHolding);
        assert_eq!(strategy.add_count(), 1);
        assert!(strategy.entry_price() > dec!(0));

        // 全平 -> 重置到 Idle
        strategy.update_state(PinSignal::CloseAll, dec!(0), dec!(0));
        assert_eq!(strategy.state(), PinState::Idle);
        assert_eq!(strategy.add_count(), 0);
        assert_eq!(strategy.entry_price(), dec!(0));
        assert_eq!(strategy.position_qty(), dec!(0));
    }

    /// 测试止损触发 CloseAll
    #[test]
    fn test_pin_state_machine_stop_loss() {
        let mut strategy = PinStrategy::new("pin_sol");

        // 入场 Long
        strategy.update_state(PinSignal::LongEntry, dec!(1), dec!(100));
        assert_eq!(strategy.state(), PinState::LongOpening);

        // 转入持仓
        strategy.update_state(PinSignal::LongAdd, dec!(1), dec!(98));
        assert_eq!(strategy.state(), PinState::LongHolding);

        // 触发止损 (亏损 10%)
        let stop_loss_signal = strategy.check_signal(
            dec!(0),
            dec!(0.5),
            dec!(50),
            dec!(0),
            "red",
            "green",
            dec!(88),  // 亏损 12%，超过 10% 止损
        );
        assert_eq!(stop_loss_signal, PinSignal::CloseAll);

        // CloseAll 直接重置
        strategy.update_state(stop_loss_signal, dec!(0), dec!(0));
        assert_eq!(strategy.state(), PinState::Idle);
    }

    /// 测试连续加仓逻辑
    #[test]
    fn test_pin_state_machine_multiple_adds() {
        let mut strategy = PinStrategy::new("pin_sol");

        // 入场
        strategy.update_state(PinSignal::LongEntry, dec!(1), dec!(100));
        assert_eq!(strategy.state(), PinState::LongOpening);
        assert_eq!(strategy.add_count(), 0);

        // 第一次加仓 -> LongHolding
        strategy.update_state(PinSignal::LongAdd, dec!(1), dec!(98));
        assert_eq!(strategy.state(), PinState::LongHolding);
        assert_eq!(strategy.add_count(), 1);
        assert_eq!(strategy.position_qty(), dec!(2));

        // 第二次加仓 (继续持仓状态)
        let signal2 = strategy.check_signal(
            dec!(-2.5),
            dec!(1.5),
            dec!(10),
            dec!(-90),
            "green",
            "green",
            dec!(96),
        );
        assert_eq!(signal2, PinSignal::LongAdd);
        strategy.update_state(signal2, dec!(1), dec!(96));
        assert_eq!(strategy.state(), PinState::LongHolding);
        assert_eq!(strategy.add_count(), 2);
        assert_eq!(strategy.position_qty(), dec!(3));

        // 第三次加仓 (价格 95.5，pnl_ratio = -4.5%，低于对冲阈值继续加仓)
        let signal3 = strategy.check_signal(
            dec!(-2.5),
            dec!(1.5),
            dec!(10),
            dec!(-90),
            "green",
            "green",
            dec!(95.5),
        );
        assert_eq!(signal3, PinSignal::LongAdd);
        strategy.update_state(signal3, dec!(1), dec!(95.5));
        assert_eq!(strategy.add_count(), 3);
        assert_eq!(strategy.position_qty(), dec!(4));
    }

    /// 测试 Short 连续加仓
    #[test]
    fn test_pin_state_machine_short_multiple_adds() {
        let mut strategy = PinStrategy::new("pin_sol");

        // 入场 Short
        strategy.update_state(PinSignal::ShortEntry, dec!(1), dec!(100));
        assert_eq!(strategy.state(), PinState::ShortOpening);

        // 第一次加仓 -> ShortHolding
        strategy.update_state(PinSignal::ShortAdd, dec!(1), dec!(102));
        assert_eq!(strategy.state(), PinState::ShortHolding);
        assert_eq!(strategy.add_count(), 1);

        // 第二次加仓
        let signal2 = strategy.check_signal(
            dec!(2.5),
            dec!(1.5),
            dec!(90),
            dec!(90),
            "red",
            "red",
            dec!(104),
        );
        assert_eq!(signal2, PinSignal::ShortAdd);
        strategy.update_state(signal2, dec!(1), dec!(104));
        assert_eq!(strategy.state(), PinState::ShortHolding);
        assert_eq!(strategy.add_count(), 2);
    }

    /// 测试状态机非法转换被忽略
    #[test]
    fn test_pin_state_machine_invalid_transition() {
        let mut strategy = PinStrategy::new("pin_sol");

        // Idle 状态下，收到 Hedge 信号应被忽略
        strategy.update_state(PinSignal::Hedge, dec!(0), dec!(0));
        assert_eq!(strategy.state(), PinState::Idle);

        // Idle 状态下，收到 Close 信号应被忽略
        strategy.update_state(PinSignal::Close, dec!(0), dec!(0));
        assert_eq!(strategy.state(), PinState::Idle);

        // 入场 Long
        strategy.update_state(PinSignal::LongEntry, dec!(1), dec!(100));
        assert_eq!(strategy.state(), PinState::LongOpening);

        // 收到 ShortAdd 不应改变状态
        strategy.update_state(PinSignal::ShortAdd, dec!(1), dec!(98));
        assert_eq!(strategy.state(), PinState::LongOpening); // 状态不变
    }

    /// 测试 reset 方法
    #[test]
    fn test_pin_state_machine_reset() {
        let mut strategy = PinStrategy::new("pin_sol");

        // 设置一些状态
        strategy.update_state(PinSignal::LongEntry, dec!(1), dec!(100));
        strategy.update_state(PinSignal::LongAdd, dec!(1), dec!(98));
        assert_eq!(strategy.state(), PinState::LongHolding);
        assert_eq!(strategy.add_count(), 1);

        // reset
        strategy.reset();
        assert_eq!(strategy.state(), PinState::Idle);
        assert_eq!(strategy.add_count(), 0);
        assert_eq!(strategy.hedge_count, 0);
        assert_eq!(strategy.entry_price(), dec!(0));
        assert_eq!(strategy.position_qty(), dec!(0));
    }
}
