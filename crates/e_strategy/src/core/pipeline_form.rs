use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use crate::strategy::types::Signal;

/// PipelineForm - 全流程表单贯穿设计
///
/// 核心理念: 从价格数据开始，一张表单贯穿所有层级，每层携带计算结果进入下一层。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineForm {
    // ========== 基础信息 ==========
    /// 品种
    pub symbol: String,
    /// 策略ID
    pub strategy_id: String,
    /// 周期
    pub period: String,
    /// 时间戳
    pub timestamp: DateTime<Utc>,
    /// 轮次ID
    pub round_id: u64,

    // ========== 价格数据层产出 ==========
    /// 当前价格
    pub tick_price: Decimal,
    /// 当前成交量
    pub tick_volume: Decimal,
    /// K线开盘价
    pub open_price: Decimal,
    /// K线收盘价
    pub close_price: Decimal,
    /// K线最高价
    pub high_price: Decimal,
    /// K线最低价
    pub low_price: Decimal,

    // ========== 指标层产出 ==========
    /// EMA快线
    pub ema_fast: Decimal,
    /// EMA慢线
    pub ema_slow: Decimal,
    /// EMA信号
    pub ema_signal: Signal,
    /// RSI值
    pub rsi_value: Decimal,
    /// Pine颜色
    pub pine_color: c_data_process::PineColor,
    /// 价格位置 0-100
    pub price_position: Decimal,

    // ========== 策略层产出 ==========
    /// 最终信号
    pub final_signal: Signal,
    /// 置信度 0-100
    pub confidence: u8,
    /// 满足条件数
    pub conditions_met: u8,
    /// 总条件数
    pub conditions_total: u8,

    // ========== 下单信息 ==========
    /// 目标价格
    pub target_price: Decimal,
    /// 目标数量
    pub quantity: Decimal,

    // ========== 风控标记 ==========
    /// 风险标记
    pub risk_flag: bool,
    /// 拒绝原因
    pub reject_reason: Option<String>,

    // ========== 通道信息 ==========
    /// 是否高速通道
    pub is_high_freq: bool,
}

impl PipelineForm {
    /// 创建新的 PipelineForm
    pub fn new(symbol: String, strategy_id: String, period: String) -> Self {
        Self {
            symbol,
            strategy_id,
            period,
            timestamp: Utc::now(),
            round_id: 0,
            tick_price: Decimal::ZERO,
            tick_volume: Decimal::ZERO,
            open_price: Decimal::ZERO,
            close_price: Decimal::ZERO,
            high_price: Decimal::ZERO,
            low_price: Decimal::ZERO,
            ema_fast: Decimal::ZERO,
            ema_slow: Decimal::ZERO,
            ema_signal: Signal::LongEntry,
            rsi_value: Decimal::ZERO,
            pine_color: c_data_process::PineColor::Purple,
            price_position: Decimal::ZERO,
            final_signal: Signal::LongEntry,
            confidence: 0,
            conditions_met: 0,
            conditions_total: 0,
            target_price: Decimal::ZERO,
            quantity: Decimal::ZERO,
            risk_flag: false,
            reject_reason: None,
            is_high_freq: false,
        }
    }

    /// 设置轮次ID
    pub fn with_round_id(mut self, round_id: u64) -> Self {
        self.round_id = round_id;
        self
    }

    /// 设置价格数据
    pub fn with_price_data(
        mut self,
        price: Decimal,
        volume: Decimal,
        open: Decimal,
        close: Decimal,
        high: Decimal,
        low: Decimal,
    ) -> Self {
        self.tick_price = price;
        self.tick_volume = volume;
        self.open_price = open;
        self.close_price = close;
        self.high_price = high;
        self.low_price = low;
        self
    }

    /// 设置指标数据
    pub fn with_indicator_data(
        mut self,
        ema_fast: Decimal,
        ema_slow: Decimal,
        ema_signal: Signal,
        rsi_value: Decimal,
        pine_color: c_data_process::PineColor,
        price_position: Decimal,
    ) -> Self {
        self.ema_fast = ema_fast;
        self.ema_slow = ema_slow;
        self.ema_signal = ema_signal;
        self.rsi_value = rsi_value;
        self.pine_color = pine_color;
        self.price_position = price_position;
        self
    }

    /// 设置策略信号
    pub fn with_signal(
        mut self,
        final_signal: Signal,
        confidence: u8,
        conditions_met: u8,
        conditions_total: u8,
    ) -> Self {
        self.final_signal = final_signal;
        self.confidence = confidence;
        self.conditions_met = conditions_met;
        self.conditions_total = conditions_total;
        self
    }

    /// 设置下单信息
    pub fn with_order_data(mut self, target_price: Decimal, quantity: Decimal) -> Self {
        self.target_price = target_price;
        self.quantity = quantity;
        self
    }

    /// 设置风控标记
    pub fn with_risk(mut self, risk_flag: bool, reject_reason: Option<String>) -> Self {
        self.risk_flag = risk_flag;
        self.reject_reason = reject_reason;
        self
    }

    /// 设置高速通道
    pub fn with_high_freq(mut self, is_high_freq: bool) -> Self {
        self.is_high_freq = is_high_freq;
        self
    }

    /// 是否有有效交易信号
    pub fn has_trade_signal(&self) -> bool {
        matches!(
            self.final_signal,
            Signal::LongEntry | Signal::ShortEntry | Signal::LongHedge | Signal::ShortHedge
        ) && !self.risk_flag
    }

    /// 是否需要平仓
    pub fn needs_exit(&self) -> bool {
        matches!(
            self.final_signal,
            Signal::LongExit | Signal::ShortExit | Signal::ExitHighVol
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_form_creation() {
        let form = PipelineForm::new(
            "BTCUSDT".to_string(),
            "trend".to_string(),
            "1m".to_string(),
        );
        assert_eq!(form.symbol, "BTCUSDT");
        assert_eq!(form.strategy_id, "trend");
        assert_eq!(form.period, "1m");
    }

    #[test]
    fn test_pipeline_form_builder() {
        let form = PipelineForm::new("BTCUSDT".to_string(), "trend".to_string(), "1m".to_string())
            .with_round_id(1)
            .with_price_data(
                dec!(50000),
                dec!(1.0),
                dec!(49000),
                dec!(50000),
                dec!(51000),
                dec!(49000),
            )
            .with_signal(Signal::LongEntry, 80, 4, 5);

        assert_eq!(form.round_id, 1);
        assert_eq!(form.tick_price, dec!(50000));
        assert_eq!(form.final_signal, Signal::LongEntry);
        assert!(form.has_trade_signal());
    }
}
