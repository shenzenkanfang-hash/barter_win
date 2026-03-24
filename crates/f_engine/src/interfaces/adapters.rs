//! 接口适配器
//!
//! 将现有实现适配到新接口，确保：
//! 1. 现有代码无需大规模重构
//! 2. 新接口契约逐步推广
//! 3. 渐进式迁移到新架构

use crate::interfaces::{
    market_data::{MarketDataProvider, MarketKLine, MarketTick},
    risk::{
        AccountInfo, ExecutedOrder, OrderRequest as RiskOrderRequest, OrderSide, OrderType,
        PositionInfo, RiskCheckResult, RiskChecker, RiskThresholds, RiskWarning,
    },
    strategy::{
        MarketKLine as StrategyMarketKLine, MarketStatusType, SignalAggregator as StrategySignalAggregator,
        SignalDirection, SignalType, StrategyExecutor as StrategyExecutorTrait, StrategyInstance,
        StrategyState as StrategyStateInfo, TradingSignal, VolatilityInfo,
    },
    execution::{
        ExchangeGateway as ExecutionGateway, OrderResult as ExecutionOrderResult,
    },
};
use chrono::Utc;
use rust_decimal::Decimal;
use std::sync::Arc;

/// 将 b_data_source::MarketStream 适配到 MarketDataProvider
pub struct MarketDataAdapter;

impl MarketDataAdapter {
    /// 将 MarketTick 转换为接口契约
    pub fn adapt_tick(tick: &b_data_source::Tick) -> MarketTick {
        MarketTick {
            symbol: tick.symbol.clone(),
            price: tick.price,
            qty: tick.qty,
            timestamp: tick.timestamp,
        }
    }

    /// 将 KLine 转换为接口契约
    pub fn adapt_kline(kline: &b_data_source::KLine, is_closed: bool) -> MarketKLine {
        let period = match kline.period {
            b_data_source::Period::Minute(m) => format!("{}m", m),
            b_data_source::Period::Day => "1d".to_string(),
        };
        MarketKLine {
            symbol: kline.symbol.clone(),
            period,
            open: kline.open,
            high: kline.high,
            low: kline.low,
            close: kline.close,
            volume: kline.volume,
            timestamp: kline.timestamp,
            is_closed,
        }
    }
}

/// 将 a_common 类型适配到接口契约
pub struct CommonAdapter;

impl CommonAdapter {
    pub fn adapt_account(account: &a_common::ExchangeAccount) -> AccountInfo {
        AccountInfo {
            account_id: account.account_id.clone(),
            total_equity: account.total_equity,
            available: account.available,
            frozen_margin: account.frozen_margin,
            unrealized_pnl: account.unrealized_pnl,
        }
    }

    pub fn adapt_position(pos: &a_common::ExchangePosition) -> PositionInfo {
        use a_common::exchange::PositionDirection as CommonDir;
        let direction = match pos.net_direction() {
            CommonDir::Long => crate::interfaces::risk::PositionDirection::Long,
            CommonDir::Short => crate::interfaces::risk::PositionDirection::Short,
            CommonDir::NetLong => crate::interfaces::risk::PositionDirection::NetLong,
            CommonDir::NetShort => crate::interfaces::risk::PositionDirection::NetShort,
            CommonDir::Flat => crate::interfaces::risk::PositionDirection::Flat,
        };

        PositionInfo {
            symbol: pos.symbol.clone(),
            direction,
            quantity: pos.long_qty + pos.short_qty,
            entry_price: if pos.long_qty > Decimal::ZERO {
                pos.long_avg_price
            } else {
                pos.short_avg_price
            },
            unrealized_pnl: pos.unrealized_pnl,
            margin_used: pos.margin_used,
        }
    }
}

/// 将 f_engine::Strategy 适配到 StrategyInstance
pub struct StrategyAdapter<S: f_engine::strategy::Strategy> {
    inner: Arc<S>,
}

impl<S: f_engine::strategy::Strategy> StrategyAdapter<S> {
    pub fn new(inner: Arc<S>) -> Self {
        Self { inner }
    }
}

impl<S: f_engine::strategy::Strategy> StrategyInstance for StrategyAdapter<S> {
    fn id(&self) -> &str {
        self.inner.id()
    }

    fn name(&self) -> &str {
        self.inner.name()
    }

    fn symbols(&self) -> Vec<String> {
        self.inner.symbols()
    }

    fn is_enabled(&self) -> bool {
        self.inner.is_enabled()
    }

    fn state(&self) -> StrategyStateInfo {
        let s = self.inner.state();
        StrategyStateInfo {
            id: s.id.clone(),
            name: s.id.clone(), // 暂时用 id
            enabled: s.enabled,
            position_direction: match s.position_direction {
                f_engine::strategy::Direction::Long => SignalDirection::Long,
                f_engine::strategy::Direction::Short => SignalDirection::Short,
                f_engine::strategy::Direction::Flat => SignalDirection::Flat,
            },
            position_qty: s.position_qty,
            status: match s.status {
                f_engine::strategy::StrategyStatus::Idle => crate::interfaces::strategy::StrategyStatus::Idle,
                f_engine::strategy::StrategyStatus::Running => crate::interfaces::strategy::StrategyStatus::Running,
                f_engine::strategy::StrategyStatus::Waiting => crate::interfaces::strategy::StrategyStatus::Waiting,
                f_engine::strategy::StrategyStatus::Error => crate::interfaces::strategy::StrategyStatus::Error,
            },
            last_signal_time: None,
        }
    }

    fn on_bar(&self, bar: &MarketKLine) -> Option<TradingSignal> {
        let strategy_bar = f_engine::strategy::StrategyKLine {
            symbol: bar.symbol.clone(),
            period: bar.period.clone(),
            open: bar.open,
            high: bar.high,
            low: bar.low,
            close: bar.close,
            volume: bar.volume,
            timestamp: bar.timestamp,
        };
        self.inner.on_bar(&strategy_bar).map(|s| TradingSignal {
            id: s.strategy_id.clone(),
            symbol: s.symbol,
            direction: match s.direction {
                f_engine::strategy::Direction::Long => SignalDirection::Long,
                f_engine::strategy::Direction::Short => SignalDirection::Short,
                f_engine::strategy::Direction::Flat => SignalDirection::Flat,
            },
            signal_type: match s.signal_type {
                f_engine::strategy::SignalType::Open => SignalType::Open,
                f_engine::strategy::SignalType::Add => SignalType::Add,
                f_engine::strategy::SignalType::Reduce => SignalType::Reduce,
                f_engine::strategy::SignalType::Close => SignalType::Close,
            },
            quantity: s.quantity,
            price: s.price,
            stop_loss: s.stop_loss,
            take_profit: s.take_profit,
            priority: s.priority,
            confidence: 50,
            timestamp: s.timestamp,
        })
    }

    fn on_volatility_change(&self, volatility: &VolatilityInfo) {
        self.inner.on_volatility(volatility.value);
    }

    fn set_enabled(&self, enabled: bool) {
        self.inner.state().set_enabled(enabled);
    }

    fn update_market_status(&self, status: MarketStatusType) {
        let market_status = f_engine::strategy::MarketStatus {
            status: match status {
                MarketStatusType::Pin => f_engine::strategy::MarketStatusType::Pin,
                MarketStatusType::Trend => f_engine::strategy::MarketStatusType::Trend,
                MarketStatusType::Range => f_engine::strategy::MarketStatusType::Range,
            },
            volatility: f_engine::strategy::VolatilityTier::Low,
            volatility_value: 0.0,
        };
        self.inner.on_market_status(&market_status);
    }

    fn market_status(&self) -> Option<MarketStatusType> {
        self.inner.state().market_status().map(|s| match s.status {
            f_engine::strategy::MarketStatusType::Pin => MarketStatusType::Pin,
            f_engine::strategy::MarketStatusType::Trend => MarketStatusType::Trend,
            f_engine::strategy::MarketStatusType::Range => MarketStatusType::Range,
        })
    }
}
