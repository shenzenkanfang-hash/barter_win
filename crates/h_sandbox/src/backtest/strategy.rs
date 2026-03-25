//! 回测策略接口
//!
//! 定义策略在回测中的回调接口

use rust_decimal::Decimal;
use chrono::{DateTime, Utc};

/// 回测信号
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    /// 买入/做多
    Long,
    /// 卖出/做空
    Short,
    /// 平多
    CloseLong,
    /// 平空
    CloseShort,
    /// 持有/观望
    Hold,
}

impl Signal {
    /// 是否是开仓信号
    pub fn is_open(&self) -> bool {
        matches!(self, Signal::Long | Signal::Short)
    }

    /// 是否是平仓信号
    pub fn is_close(&self) -> bool {
        matches!(self, Signal::CloseLong | Signal::CloseShort)
    }

    /// 是否是持仓信号
    pub fn is_hold(&self) -> bool {
        matches!(self, Signal::Hold)
    }
}

/// 回测 Tick 数据（简化版）
#[derive(Debug, Clone)]
pub struct BacktestTick {
    pub symbol: String,
    pub price: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub volume: Decimal,
    pub timestamp: DateTime<Utc>,
    pub kline_timestamp: DateTime<Utc>,
}

/// 回测订单
#[derive(Debug, Clone)]
pub struct BacktestOrder {
    pub id: String,
    pub symbol: String,
    pub side: Signal,
    pub price: Decimal,
    pub qty: Decimal,
    pub timestamp: DateTime<Utc>,
}

/// 回测成交
#[derive(Debug, Clone)]
pub struct BacktestFill {
    pub order_id: String,
    pub symbol: String,
    pub side: Signal,
    pub price: Decimal,
    pub qty: Decimal,
    pub timestamp: DateTime<Utc>,
    pub pnl: Option<Decimal>, // 平仓时才有
}

/// 回测策略 trait
pub trait BacktestStrategy: Send + Sync {
    /// 获取策略名称
    fn name(&self) -> &str;

    /// Tick 回调（每个 tick 调用一次）
    fn on_tick(&mut self, tick: &BacktestTick) -> Signal;

    /// 信号回调（可选，用于更复杂的策略）
    fn on_signal(&mut self, _tick: &BacktestTick, _signal: Signal) {}

    /// 订单成交回调
    fn on_fill(&mut self, _fill: &BacktestFill) {}

    /// 回测开始回调
    fn on_start(&mut self) {}

    /// 回测结束回调
    fn on_end(&mut self) {}
}

/// 空策略（用于测试）
pub struct EmptyStrategy;

impl BacktestStrategy for EmptyStrategy {
    fn name(&self) -> &str {
        "Empty"
    }

    fn on_tick(&mut self, _tick: &BacktestTick) -> Signal {
        Signal::Hold
    }
}

/// 简单均线策略示例
pub struct MaCrossStrategy {
    name: String,
    fast_period: u32,
    slow_period: u32,
    prices: Vec<Decimal>,
}

impl MaCrossStrategy {
    pub fn new(fast_period: u32, slow_period: u32) -> Self {
        Self {
            name: format!("MA{}{}", fast_period, slow_period),
            fast_period,
            slow_period,
            prices: Vec::new(),
        }
    }

    fn calculate_ma(&self, period: u32) -> Option<Decimal> {
        if self.prices.len() < period as usize {
            return None;
        }
        let start = self.prices.len() - period as usize;
        let sum: Decimal = self.prices[start..].iter().sum();
        Some(sum / Decimal::from(period))
    }
}

impl BacktestStrategy for MaCrossStrategy {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_tick(&mut self, tick: &BacktestTick) -> Signal {
        self.prices.push(tick.price);

        if self.prices.len() < self.slow_period as usize {
            return Signal::Hold;
        }

        let fast_ma = self.calculate_ma(self.fast_period);
        let slow_ma = self.calculate_ma(self.slow_period);

        match (fast_ma, slow_ma) {
            (Some(f), Some(s)) if f > s => Signal::Long,
            (Some(f), Some(s)) if f < s => Signal::CloseLong,
            _ => Signal::Hold,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_signal() {
        assert!(Signal::Long.is_open());
        assert!(Signal::Short.is_open());
        assert!(!Signal::Long.is_close());
        assert!(Signal::CloseLong.is_close());
    }

    #[test]
    fn test_ma_strategy() {
        let mut strategy = MaCrossStrategy::new(5, 10);

        let tick = BacktestTick {
            symbol: "BTCUSDT".to_string(),
            price: dec!(50000),
            high: dec!(50100),
            low: dec!(49900),
            volume: dec!(100),
            timestamp: Utc::now(),
            kline_timestamp: Utc::now(),
        };

        // 前 10 个 tick 应该 Hold
        for i in 0..10 {
            let t = BacktestTick {
                price: Decimal::from(50000 + i),
                ..tick.clone()
            };
            let signal = strategy.on_tick(&t);
            assert_eq!(signal, Signal::Hold);
        }

        // 之后应该有信号
        let signal = strategy.on_tick(&tick);
        assert!(matches!(signal, Signal::Hold | Signal::Long));
    }
}
