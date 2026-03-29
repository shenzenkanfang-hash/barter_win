//! Strategy Trading Event Tracker - 策略交易事件追踪器
//!
//! 追踪策略回放中的所有关键事件：
//! - 信号生成
//! - 风控检查
//! - 订单构造
//! - 模拟成交
//! - 仓位变化

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

// ============================================================================
// 事件类型
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum TradingEvent {
    /// 信号生成
    Signal {
        timestamp: DateTime<Utc>,
        signal_type: String,
        direction: String,
        price: Decimal,
        strength: u8,
        conditions_met: Vec<String>,
    },
    /// 风控检查
    RiskCheck {
        timestamp: DateTime<Utc>,
        signal_type: String,
        passed: bool,
        reject_reason: Option<String>,
        check_items: HashMap<String, bool>,
    },
    /// 订单构造
    OrderConstructed {
        timestamp: DateTime<Utc>,
        order_id: String,
        symbol: String,
        side: String,
        order_type: String,
        price: Decimal,
        qty: Decimal,
    },
    /// 模拟成交
    OrderFilled {
        timestamp: DateTime<Utc>,
        order_id: String,
        filled_price: Decimal,
        filled_qty: Decimal,
        slippage: Decimal,
        commission: Decimal,
    },
    /// 订单拒绝
    OrderRejected {
        timestamp: DateTime<Utc>,
        order_id: String,
        reason: String,
    },
    /// 仓位变化
    PositionChanged {
        timestamp: DateTime<Utc>,
        side: String,
        qty: Decimal,
        entry_price: Decimal,
        position_value: Decimal,
        unrealized_pnl: Decimal,
    },
    /// PnL 更新
    PnlUpdate {
        timestamp: DateTime<Utc>,
        realized_pnl: Decimal,
        unrealized_pnl: Decimal,
        total_pnl: Decimal,
        balance: Decimal,
    },
}

// ============================================================================
// 事件统计
// ============================================================================

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EventStats {
    pub total_signals: u64,
    pub signals_by_type: HashMap<String, u64>,
    pub total_risk_checks: u64,
    pub risk_checks_passed: u64,
    pub risk_checks_rejected: u64,
    pub reject_reasons: HashMap<String, u64>,
    pub total_orders: u64,
    pub orders_filled: u64,
    pub orders_rejected: u64,
    pub total_slippage: Decimal,
    pub total_commission: Decimal,
}

// ============================================================================
// PnL 曲线点
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PnlDataPoint {
    pub tick: u64,
    pub timestamp: DateTime<Utc>,
    pub price: Decimal,
    pub realized_pnl: Decimal,
    pub unrealized_pnl: Decimal,
    pub total_pnl: Decimal,
    pub position: Decimal,
    pub balance: Decimal,
}

// ============================================================================
// 回放报告
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayReport {
    pub symbol: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub total_ticks: u64,
    pub stats: EventStats,
    pub pnl_curve: Vec<PnlDataPoint>,
    pub key_events: Vec<TradingEvent>,
    /// 最大盈利时刻
    pub max_profit: Option<MaxProfitMoment>,
    /// 最大回撤时刻
    pub max_drawdown: Option<MaxDrawdownMoment>,
    /// 失效时段
    pub invalid_periods: Vec<InvalidPeriod>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaxProfitMoment {
    pub tick: u64,
    pub timestamp: DateTime<Utc>,
    pub pnl: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaxDrawdownMoment {
    pub tick: u64,
    pub timestamp: DateTime<Utc>,
    pub pnl: Decimal,
    pub drawdown: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvalidPeriod {
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub reason: String,
}

// ============================================================================
// 策略事件追踪器
// ============================================================================

pub struct StrategyEventTracker {
    events: Arc<RwLock<Vec<TradingEvent>>>,
    stats: Arc<RwLock<EventStats>>,
    pnl_curve: Arc<RwLock<Vec<PnlDataPoint>>>,
    position: Arc<RwLock<PositionState>>,
    balance: Decimal,
    initial_balance: Decimal,
    tick_count: u64,
}

#[derive(Debug, Clone, Default)]
pub struct PositionState {
    pub side: Option<String>,        // "long" or "short"
    pub qty: Decimal,
    pub entry_price: Decimal,
}

impl StrategyEventTracker {
    pub fn new(initial_balance: Decimal) -> Self {
        Self {
            events: Arc::new(RwLock::new(Vec::new())),
            stats: Arc::new(RwLock::new(EventStats::default())),
            pnl_curve: Arc::new(RwLock::new(Vec::new())),
            position: Arc::new(RwLock::new(PositionState::default())),
            balance: initial_balance,
            initial_balance,
            tick_count: 0,
        }
    }

    /// 记录信号生成
    pub fn record_signal(
        &self,
        timestamp: DateTime<Utc>,
        signal_type: &str,
        direction: &str,
        price: Decimal,
        strength: u8,
        conditions_met: Vec<String>,
    ) {
        let event = TradingEvent::Signal {
            timestamp,
            signal_type: signal_type.to_string(),
            direction: direction.to_string(),
            price,
            strength,
            conditions_met: conditions_met.clone(),
        };

        self.events.write().unwrap().push(event.clone());

        let mut stats = self.stats.write().unwrap();
        stats.total_signals += 1;
        *stats.signals_by_type.entry(signal_type.to_string()).or_insert(0) += 1;
    }

    /// 记录风控检查
    pub fn record_risk_check(
        &self,
        timestamp: DateTime<Utc>,
        signal_type: &str,
        passed: bool,
        reject_reason: Option<String>,
        check_items: HashMap<String, bool>,
    ) {
        let event = TradingEvent::RiskCheck {
            timestamp,
            signal_type: signal_type.to_string(),
            passed,
            reject_reason: reject_reason.clone(),
            check_items,
        };

        self.events.write().unwrap().push(event.clone());

        let mut stats = self.stats.write().unwrap();
        stats.total_risk_checks += 1;
        if passed {
            stats.risk_checks_passed += 1;
        } else {
            stats.risk_checks_rejected += 1;
            if let Some(ref reason) = reject_reason {
                *stats.reject_reasons.entry(reason.clone()).or_insert(0) += 1;
            }
        }
    }

    /// 记录订单构造
    pub fn record_order(
        &self,
        timestamp: DateTime<Utc>,
        order_id: &str,
        symbol: &str,
        side: &str,
        order_type: &str,
        price: Decimal,
        qty: Decimal,
    ) {
        let event = TradingEvent::OrderConstructed {
            timestamp,
            order_id: order_id.to_string(),
            symbol: symbol.to_string(),
            side: side.to_string(),
            order_type: order_type.to_string(),
            price,
            qty,
        };

        self.events.write().unwrap().push(event.clone());

        let mut stats = self.stats.write().unwrap();
        stats.total_orders += 1;
    }

    /// 记录订单成交
    pub fn record_filled(&mut self,
        timestamp: DateTime<Utc>,
        order_id: &str,
        filled_price: Decimal,
        filled_qty: Decimal,
        slippage: Decimal,
        commission: Decimal,
    ) {
        let event = TradingEvent::OrderFilled {
            timestamp,
            order_id: order_id.to_string(),
            filled_price,
            filled_qty,
            slippage,
            commission,
        };

        self.events.write().unwrap().push(event.clone());

        let mut stats = self.stats.write().unwrap();
        stats.orders_filled += 1;
        stats.total_slippage += slippage;
        stats.total_commission += commission;

        // 更新仓位
        let mut position = self.position.write().unwrap();
        if position.side.is_none() {
            // 开仓
            position.side = Some(if filled_qty > Decimal::ZERO { "long".to_string() } else { "short".to_string() });
            position.qty = filled_qty.abs();
            position.entry_price = filled_price;
        } else {
            // 平仓
            position.qty = position.qty.saturating_sub(filled_qty.abs());
            if position.qty == Decimal::ZERO {
                position.side = None;
            }
        }

        // 记录仓位变化
        let pos_event = TradingEvent::PositionChanged {
            timestamp,
            side: position.side.clone().unwrap_or_default(),
            qty: position.qty,
            entry_price: position.entry_price,
            position_value: position.qty * filled_price,
            unrealized_pnl: Decimal::ZERO, // 需要实时计算
        };
        self.events.write().unwrap().push(pos_event);

        // 扣除手续费
        self.balance -= commission;
    }

    /// 记录订单拒绝
    pub fn record_rejected(
        &self,
        timestamp: DateTime<Utc>,
        order_id: &str,
        reason: &str,
    ) {
        let event = TradingEvent::OrderRejected {
            timestamp,
            order_id: order_id.to_string(),
            reason: reason.to_string(),
        };

        self.events.write().unwrap().push(event.clone());

        let mut stats = self.stats.write().unwrap();
        stats.orders_rejected += 1;
    }

    /// 更新 tick 并记录 PnL
    pub fn tick(&mut self,
        tick: u64,
        timestamp: DateTime<Utc>,
        price: Decimal,
    ) {
        self.tick_count = tick;

        let position = self.position.read().unwrap();
        let unrealized = if position.side.is_some() && position.qty > Decimal::ZERO {
            let pos_value = position.qty * price;
            let cost = position.qty * position.entry_price;
            if position.side.as_ref().unwrap() == "long" {
                pos_value - cost
            } else {
                cost - pos_value
            }
        } else {
            Decimal::ZERO
        };

        let total = unrealized;
        let balance = self.balance + unrealized;

        drop(position);

        let point = PnlDataPoint {
            tick,
            timestamp,
            price,
            realized_pnl: self.balance - self.initial_balance,
            unrealized_pnl: unrealized,
            total_pnl: total,
            position: self.position.read().unwrap().qty,
            balance,
        };

        self.pnl_curve.write().unwrap().push(point);
    }

    /// 生成回放报告
    pub fn generate_report(
        &self,
        symbol: &str,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> ReplayReport {
        let stats = self.stats.read().unwrap().clone();
        let pnl_curve = self.pnl_curve.read().unwrap().clone();
        let events = self.events.read().unwrap().clone();

        // 找最大盈利和最大回撤
        let mut max_profit = None;
        let mut max_drawdown = None;
        let mut peak = Decimal::ZERO;

        for point in &pnl_curve {
            if point.total_pnl > peak {
                peak = point.total_pnl;
                max_profit = Some(MaxProfitMoment {
                    tick: point.tick,
                    timestamp: point.timestamp,
                    pnl: point.total_pnl,
                });
            }

            let drawdown = peak - point.total_pnl;
            if drawdown > max_drawdown.as_ref().map(|m: &MaxDrawdownMoment| m.drawdown).unwrap_or(Decimal::ZERO) {
                max_drawdown = Some(MaxDrawdownMoment {
                    tick: point.tick,
                    timestamp: point.timestamp,
                    pnl: point.total_pnl,
                    drawdown,
                });
            }
        }

        ReplayReport {
            symbol: symbol.to_string(),
            start_time,
            end_time,
            total_ticks: self.tick_count,
            stats,
            pnl_curve,
            key_events: events,
            max_profit,
            max_drawdown,
            invalid_periods: Vec::new(), // 需要额外分析逻辑
        }
    }

    /// 获取当前仓位
    pub fn get_position(&self) -> PositionState {
        self.position.read().unwrap().clone()
    }

    /// 获取余额
    pub fn get_balance(&self) -> Decimal {
        self.balance
    }
}

// ============================================================================
// 简化撮合引擎
// ============================================================================

pub struct SimpleMatchEngine {
    /// 滑点率 (0.001 = 0.1%)
    slippage_rate: Decimal,
    /// 手续费率 (0.0004 = 0.04%)
    commission_rate: Decimal,
}

impl SimpleMatchEngine {
    pub fn new() -> Self {
        Self {
            slippage_rate: dec!(0.001),   // 0.1% 滑点
            commission_rate: dec!(0.0004), // 0.04% 手续费
        }
    }

    /// 模拟订单成交
    ///
    /// 基于 K 线 OHLC 估算成交价格：
    /// - 买入: 在 close 价格上加滑点
    /// - 卖出: 在 close 价格上减滑点
    pub fn simulate_fill(
        &self,
        order_price: Decimal,
        _kline_high: Decimal,
        _kline_low: Decimal,
        kline_close: Decimal,
        side: &str,
    ) -> (Decimal, Decimal, Decimal) {
        // 计算滑点
        let slippage = if side == "buy" {
            // 买入：使用 close 或更高的价格
            let effective_price = order_price.max(kline_close);
            (effective_price - order_price).abs()
        } else {
            // 卖出：使用 close 或更低的价格
            let effective_price = order_price.min(kline_close);
            (order_price - effective_price).abs()
        };

        // 确保滑点不为负
        let slippage = slippage.max(Decimal::ZERO);

        // 成交价格 = 订单价格 + 滑点(buy) 或 - 滑点(sell)
        let filled_price = if side == "buy" {
            order_price + slippage
        } else {
            order_price - slippage
        };

        // 手续费 = 成交价格 * 数量 * 手续费率
        let commission = kline_close * self.commission_rate;

        (filled_price, slippage, commission)
    }
}

impl Default for SimpleMatchEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_signal_recording() {
        let tracker = StrategyEventTracker::new(dec!(10000));

        tracker.record_signal(
            Utc::now(),
            "long_entry",
            "buy",
            dec!(0.00088),
            5,
            vec!["pin>=4".to_string()],
        );

        let report = tracker.generate_report("HOTUSDT", Utc::now(), Utc::now());
        assert_eq!(report.stats.total_signals, 1);
        assert_eq!(report.stats.signals_by_type.get("long_entry"), Some(&1));
    }

    #[test]
    fn test_risk_check_recording() {
        let tracker = StrategyEventTracker::new(dec!(10000));

        tracker.record_risk_check(
            Utc::now(),
            "long_entry",
            false,
            Some("PositionLimitExceeded".to_string()),
            HashMap::from([("balance".to_string(), true), ("position_limit".to_string(), false)]),
        );

        let report = tracker.generate_report("HOTUSDT", Utc::now(), Utc::now());
        assert_eq!(report.stats.total_risk_checks, 1);
        assert_eq!(report.stats.risk_checks_rejected, 1);
        assert_eq!(report.stats.reject_reasons.get("PositionLimitExceeded"), Some(&1));
    }

    #[test]
    fn test_match_engine() {
        let engine = SimpleMatchEngine::new();

        let (filled, slippage, commission) = engine.simulate_fill(
            dec!(0.00088),  // order price
            dec!(0.00089),  // kline high
            dec!(0.00087),  // kline low
            dec!(0.00088),  // kline close
            "buy",
        );

        assert!(filled >= dec!(0.00088));
        assert!(slippage >= Decimal::ZERO);
        assert!(commission > Decimal::ZERO);
    }
}
