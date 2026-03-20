use crate::check_table::CheckTable;
use crate::pipeline_form::PipelineForm;
use crate::round_guard::RoundGuard;
use indicator::{BigCycleCalculator, EMA, PineColor, PineColorBig, PineColorDetector, PricePosition, RSI};
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
        let ema_f = self.ema_fast.calculate(price);
        let ema_s = self.ema_slow.calculate(price);
        let rsi_value = self.rsi.calculate(ema_f - ema_s);
        let macd = ema_f - ema_s;
        let pine_color = PineColorDetector::detect(macd, ema_s, rsi_value);
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

    #[test]
    fn test_volatility_channel_creation() {
        let channel = VolatilityChannel::new(
            "BTCUSDT".to_string(),
            StrategyId("trend".to_string()),
        );
        assert_eq!(channel.symbol, "BTCUSDT");
        assert_eq!(channel.current_channel, ChannelType::Slow);
    }
}
