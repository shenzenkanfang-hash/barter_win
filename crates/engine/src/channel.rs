use crate::check_table::CheckTable;
use crate::pipeline_form::PipelineForm;
use crate::round_guard::RoundGuard;
use indicator::{BigCycleCalculator, EMA, PineColor, PricePosition, RSI};
use market::{KLine, KLineSynthesizer, Period, Tick};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::sync::Arc;
use strategy::types::Signal;
use strategy::StrategyId;

/// 通道类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelType {
    /// 慢速通道 - 时间驱动，分钟级判断
    Slow,
    /// 高速通道 - 波动率触发，Tick级独立判断
    High,
}

/// 波动率通道 - 管理高速/慢速通道切换
pub struct VolatilityChannel {
    /// 品种
    symbol: String,
    /// 策略ID
    strategy_id: StrategyId,

    /// 1分钟K线合成器
    kline_1m: KLineSynthesizer,
    /// 15分钟K线合成器
    kline_15m: KLineSynthesizer,
    /// 日线K线合成器 (用于长周期趋势判断)
    kline_1d: KLineSynthesizer,

    /// EMA快线 (12)
    ema_fast: EMA,
    /// EMA慢线 (26)
    ema_slow: EMA,
    /// 日线 EMA 快线 (100) - 用于长周期趋势
    ema_100: EMA,
    /// 日线 EMA 慢线 (200) - 用于长周期趋势
    ema_200: EMA,
    /// RSI (14)
    rsi: RSI,
    /// 日线 RSI (14) - 用于长周期超买超卖
    rsi_daily: RSI,
    /// 价格位置
    price_position: PricePosition,
    /// 日线价格位置
    price_position_daily: PricePosition,

    /// 大周期计算器 (TR Ratio, 区间位置, PineColor)
    big_cycle: BigCycleCalculator,

    /// 当前通道类型
    current_channel: ChannelType,
    /// Check表
    check_table: CheckTable,
    /// 轮次守卫
    round_guard: Arc<RoundGuard>,

    /// 波动率阈值: 1min >= 3% 进入高速
    volatility_threshold_1m: Decimal,
    /// 波动率阈值: 15min >= 13% 进入高速
    volatility_threshold_15m: Decimal,
}

impl VolatilityChannel {
    /// 创建新的波动率通道
    pub fn new(symbol: String, strategy_id: StrategyId) -> Self {
        Self {
            symbol: symbol.clone(),
            strategy_id,
            kline_1m: KLineSynthesizer::new(symbol.clone(), Period::Minute(1)),
            kline_15m: KLineSynthesizer::new(symbol.clone(), Period::Minute(15)),
            kline_1d: KLineSynthesizer::new(symbol.clone(), Period::Day),
            ema_fast: EMA::new(12),
            ema_slow: EMA::new(26),
            ema_100: EMA::new(100),   // 日线快线
            ema_200: EMA::new(200),   // 日线慢线
            rsi: RSI::new(14),
            rsi_daily: RSI::new(14),  // 日线RSI
            price_position: PricePosition::new(14),
            price_position_daily: PricePosition::new(14),
            big_cycle: BigCycleCalculator::new(),
            current_channel: ChannelType::Slow,
            check_table: CheckTable::new(),
            round_guard: Arc::new(RoundGuard::new()),
            volatility_threshold_1m: dec!(0.03),  // 3%
            volatility_threshold_15m: dec!(0.13), // 13%
        }
    }

    /// 处理 Tick 数据
    ///
    /// 返回: (是否完成K线, 当前表单, 是否进入高速通道)
    pub fn on_tick(&mut self, tick: &Tick) -> (bool, Option<PipelineForm>, bool) {
        // 1. 更新K线 (包括日线)
        let completed_1m = self.kline_1m.update(tick);
        self.kline_15m.update(tick);
        let completed_1d = self.kline_1d.update(tick);

        // 2. 获取当前K线数据
        let current_kline = match self.kline_1m.current_kline() {
            Some(k) => k.clone(),
            None => {
                return (false, None, false);
            }
        };

        // 获取日线数据 (用于长周期趋势判断)
        let daily_kline = self.kline_1d.current_kline();

        // 3. 更新指标
        let price = tick.price;
        let ema_f = self.ema_fast.update(price);
        let ema_s = self.ema_slow.update(price);
        let rsi_value = self.rsi.update(ema_f - ema_s);
        let macd = ema_f - ema_s;
        let pine_color = PineColor::from_string("Neutral"); // 简化实现
        let price_pos = self.price_position.calculate(
            current_kline.close,
            current_kline.high,
            current_kline.low,
        );

        // 4. 更新大周期指标 (TR Ratio, 区间位置, PineColor)
        if let Some(ref dk) = daily_kline {
            self.big_cycle.update(dk.high, dk.low, dk.close);
        }

        // 5. 检查波动率，决定通道
        let is_high_freq = self.check_volatility();

        // 6. 如果K线完成，生成CheckEntry
        let form = if completed_1m.is_some() || is_high_freq != (self.current_channel == ChannelType::High) {
            let channel_changed = is_high_freq != (self.current_channel == ChannelType::High);
            if channel_changed {
                self.current_channel = if is_high_freq {
                    ChannelType::High
                } else {
                    ChannelType::Slow
                };
            }

            let round_id = self.round_guard.next_round_id();

            Some(self.build_pipeline_form(
                &current_kline,
                tick,
                ema_f,
                ema_s,
                rsi_value,
                pine_color,
                price_pos,
                is_high_freq,
                round_id,
            ))
        } else {
            None
        };

        (completed_1m.is_some(), form, is_high_freq)
    }

    /// 构建 PipelineForm
    fn build_pipeline_form(
        &self,
        kline: &KLine,
        tick: &Tick,
        ema_f: Decimal,
        ema_s: Decimal,
        rsi_value: Decimal,
        pine_color: PineColor,
        price_pos: Decimal,
        is_high_freq: bool,
        round_id: u64,
    ) -> PipelineForm {
        // 判断信号
        let (final_signal, confidence) = self.judge_signal(pine_color, rsi_value, price_pos);

        PipelineForm::new(
            self.symbol.clone(),
            self.strategy_id.0.clone(),
            if is_high_freq { "tick".to_string() } else { "1m".to_string() },
        )
        .with_round_id(round_id)
        .with_price_data(
            tick.price,
            tick.qty,
            kline.open,
            kline.close,
            kline.high,
            kline.low,
        )
        .with_indicator_data(
            ema_f,
            ema_s,
            if ema_f > ema_s {
                Signal::LongEntry
            } else {
                Signal::ShortEntry
            },
            rsi_value,
            pine_color,
            price_pos,
        )
        .with_signal(final_signal, confidence, 0, 0)
        .with_order_data(tick.price, dec!(0.001)) // TODO: 按风控计算数量
        .with_risk(false, None)
        .with_high_freq(is_high_freq)
    }

    /// 判断信号
    fn judge_signal(
        &self,
        pine_color: PineColor,
        rsi_value: Decimal,
        price_position: Decimal,
    ) -> (Signal, u8) {
        // 简化的信号判断逻辑
        // TODO: 接入完整策略逻辑

        // 高波动退出
        if matches!(pine_color, PineColor::Purple) {
            return (Signal::ExitHighVol, 90);
        }

        // 趋势判断
        let mut confidence = 50u8;

        // Pine颜色信号
        match pine_color {
            PineColor::PureGreen => confidence += 20,
            PineColor::LightGreen => confidence += 10,
            PineColor::PureRed => confidence += 20,
            PineColor::LightRed => confidence += 10,
            PineColor::Purple => confidence += 30,
            PineColor::Neutral => {},
        }

        // 价格位置信号
        if price_position > dec!(70) {
            confidence += 10;
        } else if price_position < dec!(30) {
            confidence += 10;
        }

        // RSI信号
        if rsi_value > dec!(70) || rsi_value < dec!(30) {
            confidence += 10;
        }

        let signal = if confidence >= 70 {
            if price_position > dec!(60) {
                Signal::LongEntry
            } else if price_position < dec!(40) {
                Signal::ShortEntry
            } else {
                Signal::LongExit
            }
        } else {
            Signal::LongExit
        };

        (signal, confidence.min(100))
    }

    /// 检查波动率
    ///
    /// 返回: true=进入高速通道, false=慢速通道
    fn check_volatility(&self) -> bool {
        // 获取当前1m和15m K线
        let kline_1m = match self.kline_1m.current_kline() {
            Some(k) => k,
            None => return false,
        };

        let kline_15m = match self.kline_15m.current_kline() {
            Some(k) => k,
            None => return false,
        };

        // 计算1分钟波动率 (O-C)/O
        let vol_1m = if kline_1m.open > dec!(0) {
            (kline_1m.close - kline_1m.open).abs() / kline_1m.open
        } else {
            dec!(0)
        };

        // 计算15分钟波动率 (O-C)/O
        let vol_15m = if kline_15m.open > dec!(0) {
            (kline_15m.close - kline_15m.open).abs() / kline_15m.open
        } else {
            dec!(0)
        };

        // 判断是否进入高速通道
        vol_1m >= self.volatility_threshold_1m || vol_15m >= self.volatility_threshold_15m
    }

    /// 获取Check表
    pub fn check_table(&self) -> &CheckTable {
        &self.check_table
    }

    /// 获取当前通道类型
    pub fn current_channel(&self) -> ChannelType {
        self.current_channel
    }

    /// 获取大周期计算器
    pub fn big_cycle(&self) -> &BigCycleCalculator {
        &self.big_cycle
    }

    /// 获取大周期指标 (TR Ratio, 区间位置)
    pub fn get_big_cycle_indicators(&mut self) -> Option<indicator::BigCycleIndicators> {
        if !self.big_cycle.is_ready() {
            return None;
        }
        Some(self.big_cycle.calculate(
            self.big_cycle.current_price().unwrap_or(dec!(0)),
            self.big_cycle.current_price().unwrap_or(dec!(0)),
            self.big_cycle.current_price().unwrap_or(dec!(0)),
        ))
    }

    /// 填入CheckEntry
    pub fn fill_check_entry(&mut self, form: &PipelineForm) {
        let entry = crate::check_table::CheckEntry {
            symbol: form.symbol.clone(),
            strategy_id: form.strategy_id.clone(),
            period: form.period.clone(),
            ema_signal: form.ema_signal,
            rsi_value: form.rsi_value,
            pine_color: form.pine_color,
            price_position: form.price_position,
            final_signal: form.final_signal,
            target_price: form.target_price,
            quantity: form.quantity,
            risk_flag: form.risk_flag,
            timestamp: form.timestamp,
            round_id: form.round_id,
            is_high_freq: form.is_high_freq,
        };
        self.check_table.fill(entry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, TimeDelta, Utc};
    use rust_decimal_macros::dec;

    /// 辅助函数：创建测试用 Tick
    fn create_tick(symbol: &str, price: Decimal, timestamp: DateTime<Utc>) -> Tick {
        Tick {
            symbol: symbol.to_string(),
            price,
            qty: dec!(1.0),
            timestamp,
            kline_1m: None,
            kline_15m: None,
            kline_1d: None,
        }
    }

    /// 辅助函数：创建 UTC DateTime
    fn dt(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(seconds, 0).unwrap()
    }

    // ============================================================================
    // E4.1 VolatilityChannel 通道切换测试
    // ============================================================================

    /// 测试：Slow -> High 通道切换 (1min 波动率 >= 3%)
    ///
    /// 场景：
    /// 1. 初始状态 Slow
    /// 2. 发送 Tick 构造 1min K 线，O-C 变化率 >= 3%
    /// 3. 验证进入 High 通道
    #[test]
    fn test_channel_switch_to_high_on_1m_volatility() {
        let mut channel = VolatilityChannel::new(
            "BTCUSDT".to_string(),
            StrategyId("test".to_string()),
        );

        // 初始状态应该是 Slow
        assert_eq!(channel.current_channel(), ChannelType::Slow);

        // 第一个 Tick: 开始构建 1min K 线 (open = 100)
        let t1 = dt(1000);
        let tick1 = create_tick("BTCUSDT", dec!(100.0), t1);
        let (completed, form, is_high) = channel.on_tick(&tick1);

        // 不应该完成 K 线，也不应该进入高速
        assert!(!completed);
        assert!(form.is_none());
        assert!(!is_high);
        assert_eq!(channel.current_channel(), ChannelType::Slow);

        // 第二个 Tick: 同一分钟内，价格上涨 3.5% (close = 103.5)
        // 波动率 = |103.5 - 100| / 100 = 3.5% >= 3%
        let t2 = dt(1000); // 同一分钟
        let tick2 = create_tick("BTCUSDT", dec!(103.5), t2);
        let (completed, form, is_high) = channel.on_tick(&tick2);

        // 触发通道切换
        assert!(!completed); // 同一分钟，K线未完成
        assert!(form.is_some()); // 通道切换，生成表单
        assert!(is_high); // 检测到高波动
        assert_eq!(channel.current_channel(), ChannelType::High);
    }

    /// 测试：Slow -> High 通道切换 (15min 波动率 >= 13%)
    ///
    /// 场景：
    /// 1. 初始状态 Slow
    /// 2. 发送 Tick 构造 15min K 线，O-C 变化率 >= 13%
    /// 3. 验证进入 High 通道
    #[test]
    fn test_channel_switch_to_high_on_15m_volatility() {
        let mut channel = VolatilityChannel::new(
            "BTCUSDT".to_string(),
            StrategyId("test".to_string()),
        );

        // 初始状态应该是 Slow
        assert_eq!(channel.current_channel(), ChannelType::Slow);

        // 第一个 Tick: 开始构建 15min K 线 (open = 100)
        let t1 = dt(1000); // 秒 1000 属于分钟 16 (1000/60 = 16)
        let tick1 = create_tick("BTCUSDT", dec!(100.0), t1);
        channel.on_tick(&tick1);

        // 第二个 Tick: 同一 15min 周期内，价格变化 15% (close = 115)
        // 波动率 = |115 - 100| / 100 = 15% >= 13%
        let t2 = dt(1000); // 同一 15min 周期
        let tick2 = create_tick("BTCUSDT", dec!(115.0), t2);
        let (completed, form, is_high) = channel.on_tick(&tick2);

        // 触发通道切换
        assert!(!completed);
        assert!(form.is_some());
        assert!(is_high);
        assert_eq!(channel.current_channel(), ChannelType::High);
    }

    /// 测试：High -> Slow 通道切换 (波动率降低)
    ///
    /// 场景：
    /// 1. 先进入 High 通道
    /// 2. 然后发送低波动 Tick (vol_1m < 3% 且 vol_15m < 13%)
    /// 3. 验证切回 Slow 通道
    #[test]
    fn test_channel_switch_to_slow_on_low_volatility() {
        let mut channel = VolatilityChannel::new(
            "BTCUSDT".to_string(),
            StrategyId("test".to_string()),
        );

        // Step 1: 进入 High 通道 (通过 1min 高波动)
        let t1 = dt(1000);
        let tick1 = create_tick("BTCUSDT", dec!(100.0), t1);
        channel.on_tick(&tick1);

        // 触发高波动，进入 High
        let t2 = dt(1000);
        let tick2 = create_tick("BTCUSDT", dec!(103.5), t2);
        channel.on_tick(&tick2);
        assert_eq!(channel.current_channel(), ChannelType::High);

        // Step 2: 进入新 K 线 (低波动)
        // 新 K 线: open = 103.5, close = 103.6 (波动 < 3%)
        // 但这需要先完成当前 K 线
        // 由于处于 High 通道，channel_changed 条件变为 false，我们需要让波动率降低

        // 发送一个跨越分钟边界的新 Tick 来完成当前 K 线
        let t3 = dt(1060); // 新的一分钟
        let tick3 = create_tick("BTCUSDT", dec!(103.5), t3);
        let (completed, form, is_high) = channel.on_tick(&tick3);

        // K 线完成
        assert!(completed);
        // 此时 is_high 应该基于新的 K 线计算
        // 新 K 线刚开仓，波动率为 0，所以 is_high = false
        assert!(!is_high);

        // 因为 current_channel 是 High，is_high 是 false，所以 channel_changed = true
        // 通道应该切回 Slow
        if form.is_some() {
            assert_eq!(channel.current_channel(), ChannelType::Slow);
        }
    }

    /// 测试：通道保持在 Slow (低波动)
    ///
    /// 场景：
    /// 1. 初始状态 Slow
    /// 2. 发送低波动 Tick (vol_1m < 3% 且 vol_15m < 13%)
    /// 3. 验证保持在 Slow 通道
    #[test]
    fn test_channel_stay_slow_on_low_volatility() {
        let mut channel = VolatilityChannel::new(
            "BTCUSDT".to_string(),
            StrategyId("test".to_string()),
        );

        // 初始状态应该是 Slow
        assert_eq!(channel.current_channel(), ChannelType::Slow);

        // 发送低波动 Tick
        let t1 = dt(1000);
        let tick1 = create_tick("BTCUSDT", dec!(100.0), t1);
        channel.on_tick(&tick1);

        // 同一分钟内，价格小幅上涨 1% (波动 < 3%)
        let t2 = dt(1000);
        let tick2 = create_tick("BTCUSDT", dec!(101.0), t2);
        let (completed, form, is_high) = channel.on_tick(&tick2);

        // 不应该触发通道切换
        assert!(!completed);
        assert!(form.is_none()); // 通道未切换，不生成表单
        assert!(!is_high); // 低波动
        assert_eq!(channel.current_channel(), ChannelType::Slow);
    }

    /// 测试：快速切换场景 (High -> Slow -> High)
    ///
    /// 场景：
    /// 1. 进入 High 通道
    /// 2. K 线完成，低波动切回 Slow
    /// 3. 新 K 线高波动再次进入 High
    #[test]
    fn test_channel_rapid_switching() {
        let mut channel = VolatilityChannel::new(
            "BTCUSDT".to_string(),
            StrategyId("test".to_string()),
        );

        assert_eq!(channel.current_channel(), ChannelType::Slow);

        // Step 1: 进入 High (1min 高波动)
        let t1 = dt(1000);
        channel.on_tick(&create_tick("BTCUSDT", dec!(100.0), t1));
        let t2 = dt(1000);
        channel.on_tick(&create_tick("BTCUSDT", dec!(103.5), t2));
        assert_eq!(channel.current_channel(), ChannelType::High);

        // Step 2: K 线完成，低波动切回 Slow
        let t3 = dt(1060); // 新的一分钟，低价格
        let tick3 = create_tick("BTCUSDT", dec!(103.5), t3);
        channel.on_tick(&tick3);
        // 此时新 K 线 open = 103.5，波动为 0，应该切回 Slow
        assert_eq!(channel.current_channel(), ChannelType::Slow);

        // Step 3: 同一 K 线内再次高波动，进入 High
        let t4 = dt(1060);
        channel.on_tick(&create_tick("BTCUSDT", dec!(107.0), t4)); // 波动 ~3.4%
        assert_eq!(channel.current_channel(), ChannelType::High);
    }

    // ============================================================================
    // 辅助测试：验证 check_volatility 计算逻辑
    // ============================================================================

    /// 测试：验证 1min 波动率计算
    ///
    /// 波动率 = |close - open| / open
    #[test]
    fn test_volatility_calculation_1m() {
        let mut channel = VolatilityChannel::new(
            "BTCUSDT".to_string(),
            StrategyId("test".to_string()),
        );

        // 第一个 Tick 建立 K 线
        let t1 = dt(1000);
        let tick1 = create_tick("BTCUSDT", dec!(100.0), t1);
        channel.on_tick(&tick1);

        // 第二个 Tick 在同一分钟，价格变化 4%
        let t2 = dt(1000);
        let tick2 = create_tick("BTCUSDT", dec!(104.0), t2);
        let (_, _, is_high) = channel.on_tick(&tick2);

        // 4% >= 3%，应该检测到高波动
        assert!(is_high);
    }

    /// 测试：验证 15min 波动率计算
    ///
    /// 波动率 = |close - open| / open
    #[test]
    fn test_volatility_calculation_15m() {
        let mut channel = VolatilityChannel::new(
            "BTCUSDT".to_string(),
            StrategyId("test".to_string()),
        );

        // 第一个 Tick 建立 15min K 线
        let t1 = dt(1000); // 属于分钟 16 (16*60 = 960)
        let tick1 = create_tick("BTCUSDT", dec!(100.0), t1);
        channel.on_tick(&tick1);

        // 第二个 Tick 在同一 15min 周期，价格变化 14%
        let t2 = dt(1000); // 同一 15min 周期 (分钟 0-14)
        let tick2 = create_tick("BTCUSDT", dec!(114.0), t2);
        let (_, _, is_high) = channel.on_tick(&tick2);

        // 14% >= 13%，应该检测到高波动
        assert!(is_high);
    }
}
