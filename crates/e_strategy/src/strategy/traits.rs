use crate::strategy::error::StrategyError;
use crate::strategy::types::{Signal, TradingDecision, TradingMode};
use b_data_source::{KLine, Tick};
use rust_decimal::Decimal;

/// 分钟指标数据 (由 IndicatorLayer 计算)
#[derive(Debug, Clone)]
pub struct MinuteIndicators {
    /// EMA 快线
    pub ema_fast: Decimal,
    /// EMA 慢线
    pub ema_slow: Decimal,
    /// RSI 值 (0-100)
    pub rsi: Decimal,
    /// Pine 颜色
    pub pine_color: String,
    /// 价格位置 (0-100)
    pub price_position: Decimal,
    /// TR Ratio
    pub tr_ratio: Decimal,
}

impl MinuteIndicators {
    pub fn new(
        ema_fast: Decimal,
        ema_slow: Decimal,
        rsi: Decimal,
        pine_color: String,
        price_position: Decimal,
        tr_ratio: Decimal,
    ) -> Self {
        Self {
            ema_fast,
            ema_slow,
            rsi,
            pine_color,
            price_position,
            tr_ratio,
        }
    }

    /// 判断是否为上涨趋势
    pub fn is_bullish(&self) -> bool {
        self.ema_fast > self.ema_slow && self.pine_color == "green"
    }

    /// 判断是否为下跌趋势
    pub fn is_bearish(&self) -> bool {
        self.ema_fast < self.ema_slow && self.pine_color == "red"
    }
}

pub trait Strategy: Send + Sync {
    /// 获取策略ID
    fn id(&self) -> &str;

    /// 获取交易模式
    fn mode(&self) -> TradingMode;

    /// K线完成时生成信号
    fn on_kline_close(&self, kline: &KLine, indicators: &MinuteIndicators) -> Option<Signal>;

    /// Tick 级快速判断
    fn on_tick(&self, tick: &Tick, indicators: &MinuteIndicators) -> Option<Signal>;

    /// 获取策略名称
    fn get_name(&self) -> &str;

    /// 合成最终交易决策
    ///
    /// 根据信号和持仓状态，合成最终的TradingDecision
    fn synthesize(
        &self,
        signal: Signal,
        position_direction: Option<crate::strategy::types::Side>,
        current_price: Decimal,
    ) -> TradingDecision;
}
