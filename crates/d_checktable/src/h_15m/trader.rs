//! trader.rs - 主交易逻辑
//!
//! 流程：
//! 1. 获取市场数据 + 指标数据
//! 2. 获取持仓数据
//! 3. 信号生成器判断
//! 4. 有指令 → 下单执行 → 状态更新

#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use super::indicator::{Indicator, Signal, MarketData, PositionData};
use x_data::position::PositionSide;
use x_data::trading::signal::{StrategySignal, TradeCommand, StrategyId};

/// ==================== 状态 ====================
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Initial,
    LongFirstOpen,
    LongDoubleAdd,
    LongDayAllow,
    ShortFirstOpen,
    ShortDoubleAdd,
    ShortDayAllow,
    HedgeEnter,
    PosLocked,
}

impl Status {
    pub fn as_str(&self) -> &'static str {
        match self {
            Status::Initial => "Initial",
            Status::LongFirstOpen => "LongFirstOpen",
            Status::LongDoubleAdd => "LongDoubleAdd",
            Status::LongDayAllow => "LongDayAllow",
            Status::ShortFirstOpen => "ShortFirstOpen",
            Status::ShortDoubleAdd => "ShortDoubleAdd",
            Status::ShortDayAllow => "ShortDayAllow",
            Status::HedgeEnter => "HedgeEnter",
            Status::PosLocked => "PosLocked",
        }
    }
}

impl Default for Status {
    fn default() -> Self {
        Status::Initial
    }
}

/// ==================== 交易器 ====================
pub struct Trader {
    pub symbol: String,
    pub status: Status,
    pub indicator: Indicator,
}

impl Trader {
    pub fn new(symbol: &str) -> Self {
        Self {
            symbol: symbol.to_string(),
            status: Status::Initial,
            indicator: Indicator::new(),
        }
    }

    /// 执行一次交易循环
    ///
    /// 返回有信号时返回 StrategySignal
    pub fn tick(
        &mut self,
        market: MarketData,
        position: PositionData,
    ) -> Option<StrategySignal> {
        // 1. 生成信号
        let (signal, qty) = self.indicator.generate(&market, &position);

        if signal == Signal::None {
            return None;
        }

        // 2. 构建信号
        let strategy_signal = self.build_signal(signal, qty, market.price)?;

        // 3. 更新状态
        self.update_status(&strategy_signal);

        Some(strategy_signal)
    }

    /// 构建交易信号
    fn build_signal(
        &self,
        signal: Signal,
        qty: Decimal,
        price: Decimal,
    ) -> Option<StrategySignal> {
        let (command, direction, full_close) = match signal {
            Signal::LongOpen => (TradeCommand::Open, PositionSide::Long, false),
            Signal::ShortOpen => (TradeCommand::Open, PositionSide::Short, false),
            Signal::LongAdd => (TradeCommand::Add, PositionSide::Long, false),
            Signal::ShortAdd => (TradeCommand::Add, PositionSide::Short, false),
            Signal::LongClose => (TradeCommand::FlatPosition, PositionSide::Long, true),
            Signal::ShortClose => (TradeCommand::FlatPosition, PositionSide::Short, true),
            Signal::LongHedge => (TradeCommand::HedgeOpen, PositionSide::Short, false),
            Signal::ShortHedge => (TradeCommand::HedgeOpen, PositionSide::Long, false),
            Signal::None => return None,
        };

        // 计算数量
        let qty = if qty == Decimal::ZERO {
            dec!(0.05) // 默认开仓数量
        } else {
            qty
        };

        Some(StrategySignal {
            command,
            direction,
            quantity: qty,
            target_price: price,
            strategy_id: StrategyId::new_pin_minute(&self.symbol),
            position_ref: None,
            full_close,
            stop_loss_price: None,
            take_profit_price: None,
            reason: format!("{:?}", signal),
            confidence: 80,
            timestamp: chrono::Utc::now().timestamp(),
        })
    }

    /// 更新状态
    fn update_status(&mut self, signal: &StrategySignal) {
        match signal.command {
            TradeCommand::Open => {
                self.status = match signal.direction {
                    PositionSide::Long => Status::LongFirstOpen,
                    PositionSide::Short => Status::ShortFirstOpen,
                    _ => Status::Initial,
                };
            }
            TradeCommand::Add => {
                self.status = match signal.direction {
                    PositionSide::Long => Status::LongDoubleAdd,
                    PositionSide::Short => Status::ShortDoubleAdd,
                    _ => Status::Initial,
                };
            }
            TradeCommand::FlatPosition | TradeCommand::FlatAll => {
                self.status = Status::Initial;
            }
            TradeCommand::HedgeOpen => {
                self.status = Status::HedgeEnter;
            }
            TradeCommand::HedgeClose => {
                self.status = Status::PosLocked;
            }
            _ => {}
        }
    }

    /// 健康检查
    pub fn health(&self) -> TraderHealth {
        TraderHealth {
            symbol: self.symbol.clone(),
            status: self.status.as_str().to_string(),
        }
    }
}

/// 健康状态
#[derive(Debug, Clone)]
pub struct TraderHealth {
    pub symbol: String,
    pub status: String,
}
