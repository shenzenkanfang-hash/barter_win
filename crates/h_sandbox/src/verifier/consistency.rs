//! Consistency Checker - 状态一致性检查器
//!
//! 验证沙盒维护的"模拟交易所状态"与 Trader 内部状态一致

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use chrono::{DateTime, Utc};

use a_common::exchange::{ExchangeAccount, ExchangePosition};

/// 一致性检查结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsistencyReport {
    /// 检查时间
    pub timestamp: DateTime<Utc>,
    /// 检查点类型
    pub check_type: CheckType,
    /// 是否通过
    pub passed: bool,
    /// 沙盒侧值
    pub sandbox_value: String,
    /// Trader 侧值
    pub trader_value: String,
    /// 差异描述
    pub diff_description: String,
    /// 差异百分比
    pub diff_percentage: Option<Decimal>,
}

/// 检查点类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckType {
    Position,     // 持仓一致性
    Order,        // 订单一致性
    Account,      // 资金一致性
    Fill,         // 成交一致性
}

/// 持仓快照（用于对比）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionSnapshot {
    pub symbol: String,
    pub qty: Decimal,
    pub avg_price: Decimal,
    pub direction: String,
    pub timestamp: DateTime<Utc>,
}

/// 订单快照（用于对比）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderSnapshot {
    pub order_id: String,
    pub symbol: String,
    pub qty: Decimal,
    pub filled_qty: Decimal,
    pub price: Decimal,
    pub status: String,
    pub timestamp: DateTime<Utc>,
}

/// 账户快照（用于对比）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountSnapshot {
    pub available: Decimal,
    pub total_equity: Decimal,
    pub unrealized_pnl: Decimal,
    pub used_margin: Decimal,
    pub timestamp: DateTime<Utc>,
}

/// 成交记录（用于对比）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FillRecord {
    pub fill_id: String,
    pub order_id: String,
    pub symbol: String,
    pub qty: Decimal,
    pub price: Decimal,
    pub side: String,
    pub timestamp: DateTime<Utc>,
}

/// 状态一致性检查器
pub struct ConsistencyChecker {
    /// 沙盒侧持仓记录
    sandbox_positions: RwLock<HashMap<String, PositionSnapshot>>,
    /// 沙盒侧订单记录
    sandbox_orders: RwLock<HashMap<String, OrderSnapshot>>,
    /// 沙盒侧账户记录
    sandbox_account: RwLock<Option<AccountSnapshot>>,
    /// 沙盒侧成交记录
    sandbox_fills: RwLock<Vec<FillRecord>>,

    /// 检查历史
    history: RwLock<Vec<ConsistencyReport>>,

    /// 初始资金（用于资金曲线验证）
    initial_fund: Decimal,
}

impl ConsistencyChecker {
    /// 创建检查器
    pub fn new(initial_fund: Decimal) -> Self {
        Self {
            sandbox_positions: RwLock::new(HashMap::new()),
            sandbox_orders: RwLock::new(HashMap::new()),
            sandbox_account: RwLock::new(None),
            sandbox_fills: RwLock::new(Vec::new()),
            history: RwLock::new(Vec::new()),
            initial_fund,
        }
    }

    /// 记录沙盒侧持仓
    pub fn record_sandbox_position(&self, snapshot: PositionSnapshot) {
        let mut positions = self.sandbox_positions.write();
        positions.insert(snapshot.symbol.clone(), snapshot);
    }

    /// 记录沙盒侧订单
    pub fn record_sandbox_order(&self, snapshot: OrderSnapshot) {
        let mut orders = self.sandbox_orders.write();
        orders.insert(snapshot.order_id.clone(), snapshot);
    }

    /// 记录沙盒侧账户
    pub fn record_sandbox_account(&self, snapshot: AccountSnapshot) {
        let mut account = self.sandbox_account.write();
        *account = Some(snapshot);
    }

    /// 记录沙盒侧成交
    pub fn record_sandbox_fill(&self, record: FillRecord) {
        let mut fills = self.sandbox_fills.write();
        fills.push(record);
    }

    /// 检查持仓一致性
    pub fn check_position(&self, symbol: &str, trader_position: &PositionSnapshot) -> ConsistencyReport {
        let sandbox_positions = self.sandbox_positions.read();
        let sandbox_pos = sandbox_positions.get(symbol);

        let (passed, sandbox_val, trader_val, diff_desc, diff_pct) = match sandbox_pos {
            Some(sp) => {
                let qty_match = sp.qty == trader_position.qty;
                let price_match = sp.avg_price == trader_position.avg_price;
                let direction_match = sp.direction == trader_position.direction;

                if qty_match && price_match && direction_match {
                    (true, format!("qty={}, price={}", sp.qty, sp.avg_price),
                     format!("qty={}, price={}", trader_position.qty, trader_position.avg_price),
                     "完全一致".to_string(), None)
                } else {
                    let mut diffs = Vec::new();
                    if !qty_match {
                        diffs.push(format!("qty: 沙盒{} vs Trader{}", sp.qty, trader_position.qty));
                    }
                    if !price_match {
                        diffs.push(format!("price: 沙盒{} vs Trader{}", sp.avg_price, trader_position.avg_price));
                    }
                    if !direction_match {
                        diffs.push(format!("direction: 沙盒{} vs Trader{}", sp.direction, trader_position.direction));
                    }

                    let diff_pct = if sp.qty > Decimal::ZERO {
                        ((trader_position.qty - sp.qty) / sp.qty * dec!(100)).round_dp(2)
                    } else {
                        Decimal::ZERO
                    };

                    (false, format!("qty={}, price={}", sp.qty, sp.avg_price),
                     format!("qty={}, price={}", trader_position.qty, trader_position.avg_price),
                     diffs.join("; "), Some(diff_pct))
                }
            }
            None => {
                if trader_position.qty == Decimal::ZERO {
                    (true, "无持仓".to_string(), "无持仓".to_string(), "一致".to_string(), None)
                } else {
                    (false, "无记录".to_string(),
                     format!("qty={}", trader_position.qty),
                     "沙盒无记录但 Trader 有持仓".to_string(),
                     Some(dec!(100)))
                }
            }
        };

        let report = ConsistencyReport {
            timestamp: Utc::now(),
            check_type: CheckType::Position,
            passed,
            sandbox_value: sandbox_val,
            trader_value: trader_val,
            diff_description: diff_desc,
            diff_percentage: diff_pct,
        };

        // 记录到历史
        self.history.write().push(report.clone());

        report
    }

    /// 检查订单一致性
    pub fn check_order(&self, order_id: &str, trader_order: &OrderSnapshot) -> ConsistencyReport {
        let sandbox_orders = self.sandbox_orders.read();
        let sandbox_order = sandbox_orders.get(order_id);

        let (passed, sandbox_val, trader_val, diff_desc, _) = match sandbox_order {
            Some(so) => {
                let qty_match = so.filled_qty == trader_order.filled_qty;
                let status_match = so.status == trader_order.status;

                if qty_match && status_match {
                    (true, format!("filled={}, status={}", so.filled_qty, so.status),
                     format!("filled={}, status={}", trader_order.filled_qty, trader_order.status),
                     "完全一致".to_string())
                } else {
                    let mut diffs = Vec::new();
                    if !qty_match {
                        diffs.push(format!("filled_qty: 沙盒{} vs Trader{}", so.filled_qty, trader_order.filled_qty));
                    }
                    if !status_match {
                        diffs.push(format!("status: 沙盒{} vs Trader{}", so.status, trader_order.status));
                    }
                    (false, format!("filled={}, status={}", so.filled_qty, so.status),
                     format!("filled={}, status={}", trader_order.filled_qty, trader_order.status),
                     diffs.join("; "))
                }
            }
            None => {
                if trader_order.filled_qty == Decimal::ZERO {
                    (true, "无订单".to_string(), "无订单".to_string(), "一致".to_string())
                } else {
                    (false, "无记录".to_string(),
                     format!("filled={}", trader_order.filled_qty),
                     "沙盒无记录但 Trader 有订单".to_string())
                }
            }
        };

        let report = ConsistencyReport {
            timestamp: Utc::now(),
            check_type: CheckType::Order,
            passed,
            sandbox_value: sandbox_val,
            trader_value: trader_val,
            diff_description: diff_desc,
            diff_percentage: None,
        };

        self.history.write().push(report.clone());

        report
    }

    /// 检查账户资金一致性
    pub fn check_account(&self, trader_account: &AccountSnapshot) -> ConsistencyReport {
        let sandbox_account = self.sandbox_account.read();
        let sandbox_acc = sandbox_account.as_ref();

        let (passed, sandbox_val, trader_val, diff_desc, diff_pct) = match sandbox_acc {
            Some(sa) => {
                let eq_match = sa.total_equity == trader_account.total_equity;
                let avail_match = sa.available == trader_account.available;

                if eq_match && avail_match {
                    (true, format!("equity={}, avail={}", sa.total_equity, sa.available),
                     format!("equity={}, avail={}", trader_account.total_equity, trader_account.available),
                     "完全一致".to_string(), None)
                } else {
                    let mut diffs = Vec::new();
                    if !eq_match {
                        diffs.push(format!("equity: 沙盒{} vs Trader{}", sa.total_equity, trader_account.total_equity));
                    }
                    if !avail_match {
                        diffs.push(format!("available: 沙盒{} vs Trader{}", sa.available, trader_account.available));
                    }

                    let diff_pct = if sa.total_equity > Decimal::ZERO {
                        ((trader_account.total_equity - sa.total_equity) / sa.total_equity * dec!(100)).round_dp(2)
                    } else {
                        Decimal::ZERO
                    };

                    (false, format!("equity={}", sa.total_equity),
                     format!("equity={}", trader_account.total_equity),
                     diffs.join("; "), Some(diff_pct))
                }
            }
            None => {
                (false, "无记录".to_string(),
                 format!("equity={}", trader_account.total_equity),
                 "沙盒无账户记录".to_string(), None)
            }
        };

        let report = ConsistencyReport {
            timestamp: Utc::now(),
            check_type: CheckType::Account,
            passed,
            sandbox_value: sandbox_val,
            trader_value: trader_val,
            diff_description: diff_desc,
            diff_percentage: diff_pct,
        };

        self.history.write().push(report.clone());

        report
    }

    /// 获取历史统计
    pub fn get_statistics(&self) -> ConsistencyStatistics {
        let history = self.history.read();
        let total = history.len();
        let passed = history.iter().filter(|r| r.passed).count();
        let failed = total - passed;

        let by_type: HashMap<CheckType, (usize, usize)> = history.iter()
            .fold(HashMap::new(), |mut acc, r| {
                let entry = acc.entry(r.check_type).or_insert((0, 0));
                if r.passed {
                    entry.0 += 1;
                } else {
                    entry.1 += 1;
                }
                acc
            });

        ConsistencyStatistics {
            total_checks: total,
            passed_count: passed,
            failed_count: failed,
            pass_rate: if total > 0 { dec!(passed) * dec!(100) / dec!(total) } else { Decimal::ZERO },
            by_type,
        }
    }

    /// 获取所有历史记录
    pub fn get_history(&self) -> Vec<ConsistencyReport> {
        self.history.read().clone()
    }
}

/// 一致性统计
#[derive(Debug, Clone)]
pub struct ConsistencyStatistics {
    pub total_checks: usize,
    pub passed_count: usize,
    pub failed_count: usize,
    pub pass_rate: Decimal,
    pub by_type: HashMap<CheckType, (usize, usize)>, // (passed, failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_consistency() {
        let checker = ConsistencyChecker::new(dec!(10000));

        // 记录沙盒持仓
        checker.record_sandbox_position(PositionSnapshot {
            symbol: "BTCUSDT".to_string(),
            qty: dec!(0.5),
            avg_price: dec!(50000),
            direction: "Long".to_string(),
            timestamp: Utc::now(),
        });

        // 检查一致（相同值）
        let report = checker.check_position("BTCUSDT", &PositionSnapshot {
            symbol: "BTCUSDT".to_string(),
            qty: dec!(0.5),
            avg_price: dec!(50000),
            direction: "Long".to_string(),
            timestamp: Utc::now(),
        });

        assert!(report.passed, "相同值应该通过");

        // 检查不一致（不同值）
        let report2 = checker.check_position("BTCUSDT", &PositionSnapshot {
            symbol: "BTCUSDT".to_string(),
            qty: dec!(0.6),
            avg_price: dec!(50000),
            direction: "Long".to_string(),
            timestamp: Utc::now(),
        });

        assert!(!report2.passed, "不同值应该失败");
    }

    #[test]
    fn test_statistics() {
        let checker = ConsistencyChecker::new(dec!(10000));

        checker.record_sandbox_position(PositionSnapshot {
            symbol: "BTCUSDT".to_string(),
            qty: dec!(0.5),
            avg_price: dec!(50000),
            direction: "Long".to_string(),
            timestamp: Utc::now(),
        });

        let _ = checker.check_position("BTCUSDT", &PositionSnapshot {
            symbol: "BTCUSDT".to_string(),
            qty: dec!(0.6),  // 不一致
            avg_price: dec!(50000),
            direction: "Long".to_string(),
            timestamp: Utc::now(),
        });

        let stats = checker.get_statistics();
        assert_eq!(stats.total_checks, 1);
        assert_eq!(stats.failed_count, 1);
        assert_eq!(stats.pass_rate, dec!(0));
    }
}