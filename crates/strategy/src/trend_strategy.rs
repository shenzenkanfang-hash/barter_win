use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

/// 趋势策略状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrendState {
    /// 等待入场
    Idle,
    /// 持有多头
    Long,
    /// 持有空头
    Short,
}

impl Default for TrendState {
    fn default() -> Self {
        TrendState::Idle
    }
}

/// 趋势策略信号
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrendSignal {
    /// 做多入场
    LongEntry,
    /// 做空入场
    ShortEntry,
    /// 平多
    LongExit,
    /// 平空
    ShortExit,
    /// 观望
    Watch,
}

/// 趋势策略配置
#[derive(Debug, Clone)]
pub struct TrendStrategyConfig {
    /// 多头入场置信度阈值
    pub long_entry_confidence: u8,
    /// 空头入场置信度阈值
    pub short_entry_confidence: u8,
    /// 平仓置信度阈值
    pub exit_confidence: u8,
    /// 价格位置超买阈值
    pub price_pos_overbought: Decimal,
    /// 价格位置超卖阈值
    pub price_pos_oversold: Decimal,
    /// 趋势确认 EMA 金叉/死叉阈值
    pub ema_cross_threshold: Decimal,
}

impl Default for TrendStrategyConfig {
    fn default() -> Self {
        Self {
            long_entry_confidence: 70,
            short_entry_confidence: 70,
            exit_confidence: 50,
            price_pos_overbought: dec!(70),
            price_pos_oversold: dec!(30),
            ema_cross_threshold: dec!(0.001),
        }
    }
}

/// 趋势策略
///
/// 基于 EMA、PineColor、RSI 等指标的趋势跟踪策略。
///
/// 状态机:
/// Idle -> Long (做多入场条件满足)
/// Idle -> Short (做空入场条件满足)
/// Long -> Idle (平多条件满足)
/// Short -> Idle (平空条件满足)
///
/// 设计依据: 设计文档 16.10.1
pub struct TrendStrategy {
    /// 策略ID
    strategy_id: String,
    /// 当前状态
    state: TrendState,
    /// 配置
    config: TrendStrategyConfig,
}

impl TrendStrategy {
    /// 创建新的趋势策略
    pub fn new(strategy_id: &str) -> Self {
        Self {
            strategy_id: strategy_id.to_string(),
            state: TrendState::Idle,
            config: TrendStrategyConfig::default(),
        }
    }

    /// 创建带配置的趋势策略
    pub fn with_config(strategy_id: &str, config: TrendStrategyConfig) -> Self {
        Self {
            strategy_id: strategy_id.to_string(),
            state: TrendState::Idle,
            config,
        }
    }

    /// 判断信号
    ///
    /// 输入指标数据，返回交易信号。
    pub fn check_signal(
        &self,
        pine_color: &str,         // Pine 颜色: "green", "red", "neutral"
        rsi_value: Decimal,       // RSI 值: 0-100
        price_position: Decimal,  // 价格位置: 0-100
        ema_fast: Decimal,        // 快线 EMA
        ema_slow: Decimal,        // 慢线 EMA
        confidence: u8,            // 置信度: 0-100
    ) -> TrendSignal {
        match self.state {
            TrendState::Idle => {
                self.check_entry_signal(pine_color, rsi_value, price_position, ema_fast, ema_slow, confidence)
            }
            TrendState::Long => {
                self.check_long_exit_signal(pine_color, rsi_value, price_position, ema_fast, ema_slow)
            }
            TrendState::Short => {
                self.check_short_exit_signal(pine_color, rsi_value, price_position, ema_fast, ema_slow)
            }
        }
    }

    /// 判断入场信号 (Idle 状态)
    fn check_entry_signal(
        &self,
        pine_color: &str,
        rsi_value: Decimal,
        price_position: Decimal,
        ema_fast: Decimal,
        ema_slow: Decimal,
        confidence: u8,
    ) -> TrendSignal {
        // EMA 金叉/死叉
        let ema_cross = (ema_fast - ema_slow).abs() / ema_slow;
        let has_ema_trend = ema_cross > self.config.ema_cross_threshold;

        // 做多条件 (置信度 >= 阈值 且 Pine 绿色 且价格位置底部)
        if confidence >= self.config.long_entry_confidence
            && pine_color == "green"
            && price_position < self.config.price_pos_oversold
            && has_ema_trend
            && ema_fast > ema_slow
        {
            return TrendSignal::LongEntry;
        }

        // 做空条件 (置信度 >= 阈值 且 Pine 红色/紫色 且价格位置顶部)
        if confidence >= self.config.short_entry_confidence
            && (pine_color == "red" || pine_color == "purple")
            && price_position > self.config.price_pos_overbought
            && has_ema_trend
            && ema_fast < ema_slow
        {
            return TrendSignal::ShortEntry;
        }

        TrendSignal::Watch
    }

    /// 判断多头出场信号 (Long 状态)
    fn check_long_exit_signal(
        &self,
        pine_color: &str,
        rsi_value: Decimal,
        price_position: Decimal,
        ema_fast: Decimal,
        ema_slow: Decimal,
    ) -> TrendSignal {
        // 平多条件: Pine 颜色非纯绿 或 RSI 超买 或 EMA 死叉
        if pine_color != "green" || rsi_value > dec!(70) || ema_fast < ema_slow {
            return TrendSignal::LongExit;
        }

        // 价格位置在中间区域，平多观望
        if price_position > dec!(40) && price_position < dec!(60) {
            return TrendSignal::Watch;
        }

        TrendSignal::Watch
    }

    /// 判断空头出场信号 (Short 状态)
    fn check_short_exit_signal(
        &self,
        pine_color: &str,
        rsi_value: Decimal,
        price_position: Decimal,
        ema_fast: Decimal,
        ema_slow: Decimal,
    ) -> TrendSignal {
        // 平空条件: Pine 颜色非纯红 或 RSI 超卖 或 EMA 金叉
        if pine_color != "red" || rsi_value < dec!(30) || ema_fast > ema_slow {
            return TrendSignal::ShortExit;
        }

        // 价格位置在中间区域，平空观望
        if price_position > dec!(40) && price_position < dec!(60) {
            return TrendSignal::Watch;
        }

        TrendSignal::Watch
    }

    /// 更新状态 (根据信号)
    pub fn update_state(&mut self, signal: TrendSignal) {
        match (self.state, signal) {
            (TrendState::Idle, TrendSignal::LongEntry) => self.state = TrendState::Long,
            (TrendState::Idle, TrendSignal::ShortEntry) => self.state = TrendState::Short,
            (TrendState::Long, TrendSignal::LongExit) => self.state = TrendState::Idle,
            (TrendState::Short, TrendSignal::ShortExit) => self.state = TrendState::Idle,
            _ => {}
        }
    }

    /// 获取当前状态
    pub fn state(&self) -> TrendState {
        self.state
    }

    /// 获取策略ID
    pub fn strategy_id(&self) -> &str {
        &self.strategy_id
    }

    /// 重置策略状态
    pub fn reset(&mut self) {
        self.state = TrendState::Idle;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trend_strategy_idle_to_long() {
        let strategy = TrendStrategy::new("trend_btc");

        // 满足做多条件
        let signal = strategy.check_signal(
            "green",      // pine_color
            dec!(35),     // rsi (超卖)
            dec!(25),     // price_position (底部)
            dec!(50500),  // ema_fast
            dec!(50000),  // ema_slow (金叉)
            75,          // confidence (超过70阈值)
        );

        assert_eq!(signal, TrendSignal::LongEntry);
    }

    #[test]
    fn test_trend_strategy_idle_to_short() {
        let strategy = TrendStrategy::new("trend_btc");

        // 满足做空条件
        let signal = strategy.check_signal(
            "red",        // pine_color
            dec!(75),     // rsi (超买)
            dec!(80),     // price_position (顶部)
            dec!(49500),  // ema_fast
            dec!(50000),  // ema_slow (死叉)
            75,          // confidence
        );

        assert_eq!(signal, TrendSignal::ShortEntry);
    }

    #[test]
    fn test_trend_strategy_long_to_idle() {
        let mut strategy = TrendStrategy::new("trend_btc");
        strategy.update_state(TrendSignal::LongEntry); // 先入场
        assert_eq!(strategy.state(), TrendState::Long);

        // Pine 变红，触发平多
        let signal = strategy.check_signal(
            "red",        // pine_color 变红
            dec!(75),     // rsi 超买
            dec!(70),     // price_position
            dec!(49500),  // ema_fast
            dec!(50000),  // ema_slow
            50,
        );

        assert_eq!(signal, TrendSignal::LongExit);
    }

    #[test]
    fn test_trend_strategy_watch() {
        let strategy = TrendStrategy::new("trend_btc");

        // 不满足条件，观望
        let signal = strategy.check_signal(
            "neutral",    // pine_color
            dec!(50),    // rsi 中性
            dec!(50),    // price_position 中间
            dec!(50000),  // ema_fast
            dec!(50000),  // ema_slow 无交叉
            30,          // confidence 不足
        );

        assert_eq!(signal, TrendSignal::Watch);
    }
}
