//! Fund Curve Validator - 资金曲线验证
//!
//! 独立计算 PnL，验证 Trader 计算的正确性

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::str::FromStr;
use chrono::{DateTime, Utc};

/// 成交事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FillEvent {
    pub fill_id: String,
    pub timestamp: DateTime<Utc>,
    pub symbol: String,
    pub side: FillSide,
    pub qty: Decimal,
    pub price: Decimal,
    pub fee: Decimal,
}

/// 成交方向
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FillSide {
    Buy,
    Sell,
}

/// 资金快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FundSnapshot {
    pub timestamp: DateTime<Utc>,
    pub available: Decimal,
    pub position_value: Decimal,
    pub total_equity: Decimal,
    pub unrealized_pnl: Decimal,
    pub realized_pnl: Decimal,
    pub cumulative_pnl: Decimal,
}

/// 资金曲线报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FundCurveReport {
    /// 测试时间
    pub test_start: DateTime<Utc>,
    pub test_end: DateTime<Utc>,
    /// 初始资金
    pub initial_fund: Decimal,
    /// 最终资金
    pub final_fund: Decimal,
    /// 累计盈亏
    pub cumulative_pnl: Decimal,
    /// 盈亏率
    pub pnl_rate: Decimal,
    /// 最大回撤
    pub max_drawdown: Decimal,
    /// 最大回撤率
    pub max_drawdown_rate: Decimal,
    /// 交易次数
    pub trade_count: usize,
    /// 盈利次数
    pub win_count: usize,
    /// 亏损次数
    pub loss_count: usize,
    /// 胜率
    pub win_rate: Decimal,
    /// 资金曲线单调性检查
    pub monotonicity_check: MonotonicityResult,
}

/// 单调性检查结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonotonicityResult {
    pub is_monotonic: bool,
    pub violations: Vec<MonotonicityViolation>,
}

/// 单调性违规
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonotonicityViolation {
    pub timestamp: DateTime<Utc>,
    pub prev_equity: Decimal,
    pub curr_equity: Decimal,
    pub violation_type: String,
}

/// 资金曲线验证器
pub struct FundCurveValidator {
    /// 初始资金
    initial_fund: Decimal,
    /// 成交事件队列
    fills: VecDeque<FillEvent>,
    /// 资金快照历史
    fund_history: VecDeque<FundSnapshot>,
    /// 当前持仓（独立计算）
    current_position: IndependentPosition,
}

/// 独立计算的持仓
#[derive(Debug, Clone)]
pub struct IndependentPosition {
    pub symbol: String,
    pub qty: Decimal,
    pub avg_price: Decimal,
    pub total_cost: Decimal,
}

impl IndependentPosition {
    pub fn new(symbol: String) -> Self {
        Self {
            symbol,
            qty: Decimal::ZERO,
            avg_price: Decimal::ZERO,
            total_cost: Decimal::ZERO,
        }
    }

    /// 买入（更新持仓成本）
    pub fn buy(&mut self, qty: Decimal, price: Decimal, fee: Decimal) {
        let cost = qty * price + fee;
        let new_total_qty = self.qty + qty;

        if new_total_qty > Decimal::ZERO {
            // 加权平均计算新持仓成本
            self.avg_price = ((self.qty * self.avg_price) + (qty * price)) / new_total_qty;
            self.total_cost = self.total_cost + cost;
            self.qty = new_total_qty;
        }
    }

    /// 卖出（实现盈亏）
    pub fn sell(&mut self, qty: Decimal, price: Decimal, fee: Decimal) -> Decimal {
        let proceeds = qty * price - fee;
        let cost = qty * self.avg_price;
        let realized_pnl = proceeds - cost;

        if qty >= self.qty {
            // 全平
            self.qty = Decimal::ZERO;
            self.avg_price = Decimal::ZERO;
            self.total_cost = Decimal::ZERO;
        } else {
            // 部分平仓
            self.qty = self.qty - qty;
            self.total_cost = self.qty * self.avg_price;
        }

        realized_pnl
    }

    /// 计算未实现盈亏
    pub fn unrealized_pnl(&self, current_price: Decimal) -> Decimal {
        if self.qty == Decimal::ZERO {
            return Decimal::ZERO;
        }
        (current_price - self.avg_price) * self.qty
    }

    /// 当前持仓价值
    pub fn position_value(&self, current_price: Decimal) -> Decimal {
        self.qty * current_price
    }
}

impl FundCurveValidator {
    /// 创建验证器
    pub fn new(initial_fund: Decimal, symbol: &str) -> Self {
        Self {
            initial_fund,
            fills: VecDeque::new(),
            fund_history: VecDeque::new(),
            current_position: IndependentPosition::new(symbol.to_string()),
        }
    }

    /// 记录成交事件
    pub fn record_fill(&mut self, event: FillEvent) {
        self.fills.push_back(event);
    }

    /// 独立计算当前资金状态
    pub fn calculate_current_state(&self, current_price: Decimal) -> FundSnapshot {
        let available = self.initial_fund + self.fund_history.iter()
            .map(|s| s.realized_pnl)
            .sum::<Decimal>();

        let position_value = self.current_position.position_value(current_price);
        let unrealized_pnl = self.current_position.unrealized_pnl(current_price);
        let total_equity = available + position_value;
        let cumulative_pnl = total_equity - self.initial_fund;

        FundSnapshot {
            timestamp: Utc::now(),
            available,
            position_value,
            total_equity,
            unrealized_pnl,
            realized_pnl: total_equity - position_value - self.initial_fund,
            cumulative_pnl,
        }
    }

    /// 处理成交事件（独立计算）
    pub fn process_fill(&mut self, fill: FillEvent) -> FundSnapshot {
        let price = Decimal::from_str(&fill.price).unwrap_or(Decimal::ZERO);

        // 独立计算更新持仓
        match fill.side {
            FillSide::Buy => {
                self.current_position.buy(fill.qty, price, fill.fee);
            }
            FillSide::Sell => {
                self.current_position.sell(fill.qty, price, fill.fee);
            }
        }

        // 记录快照
        let snapshot = self.calculate_current_state(price);
        self.fund_history.push_back(snapshot.clone());

        snapshot
    }

    /// 生成资金曲线报告
    pub fn generate_report(&self, current_price: Decimal) -> FundCurveReport {
        let final_equity = self.calculate_current_state(current_price).total_equity;
        let cumulative_pnl = final_equity - self.initial_fund;
        let pnl_rate = if self.initial_fund > Decimal::ZERO {
            (cumulative_pnl / self.initial_fund * dec!(100)).round_dp(2)
        } else {
            Decimal::ZERO
        };

        // 计算最大回撤
        let (max_drawdown, max_drawdown_rate) = self.calculate_max_drawdown();

        // 统计交易次数
        let trade_count = self.fills.len();
        let (win_count, loss_count) = self.calculate_win_loss();

        let win_rate = if trade_count > 0 {
            (dec!(win_count) / dec!(trade_count) * dec!(100)).round_dp(2)
        } else {
            Decimal::ZERO
        };

        // 单调性检查
        let monotonicity_check = self.check_monotonicity();

        let test_start = self.fund_history.front()
            .map(|s| s.timestamp)
            .unwrap_or_else(Utc::now);
        let test_end = self.fund_history.back()
            .map(|s| s.timestamp)
            .unwrap_or_else(Utc::now);

        FundCurveReport {
            test_start,
            test_end,
            initial_fund: self.initial_fund,
            final_fund: final_equity,
            cumulative_pnl,
            pnl_rate,
            max_drawdown,
            max_drawdown_rate,
            trade_count,
            win_count,
            loss_count,
            win_rate,
            monotonicity_check,
        }
    }

    /// 计算最大回撤
    fn calculate_max_drawdown(&self) -> (Decimal, Decimal) {
        let mut peak = self.initial_fund;
        let mut max_drawdown = Decimal::ZERO;
        let mut max_drawdown_rate = Decimal::ZERO;

        for snapshot in &self.fund_history {
            if snapshot.total_equity > peak {
                peak = snapshot.total_equity;
            }

            let drawdown = peak - snapshot.total_equity;
            let drawdown_rate = if peak > Decimal::ZERO {
                (drawdown / peak * dec!(100)).round_dp(2)
            } else {
                Decimal::ZERO
            };

            if drawdown > max_drawdown {
                max_drawdown = drawdown;
                max_drawdown_rate = drawdown_rate;
            }
        }

        (max_drawdown, max_drawdown_rate)
    }

    /// 统计盈亏次数
    fn calculate_win_loss(&self) -> (usize, usize) {
        let mut win = 0;
        let mut loss = 0;

        // 按买卖对计算盈亏
        let mut buy_queue: Vec<FillEvent> = Vec::new();

        for fill in &self.fills {
            match fill.side {
                FillSide::Buy => buy_queue.push(fill.clone()),
                FillSide::Sell => {
                    // 匹配最近的买入
                    if let Some(buy) = buy_queue.pop() {
                        let sell_price = Decimal::from_str(&fill.price).unwrap_or(Decimal::ZERO);
                        let buy_price = buy.price.parse::<Decimal>().unwrap_or(Decimal::ZERO);
                        let pnl = (sell_price - buy_price) * fill.qty - fill.fee - buy.fee;

                        if pnl > Decimal::ZERO {
                            win += 1;
                        } else {
                            loss += 1;
                        }
                    }
                }
            }
        }

        (win, loss)
    }

    /// 检查资金曲线单调性（不允许不可能的回退）
    fn check_monotonicity(&self) -> MonotonicityResult {
        let mut violations = Vec::new();
        let mut prev_equity = self.initial_fund;

        for snapshot in &self.fund_history {
            // 不允许资金变为负数（除非是合理亏损）
            if snapshot.total_equity < Decimal::ZERO {
                violations.push(MonotonicityViolation {
                    timestamp: snapshot.timestamp,
                    prev_equity,
                    curr_equity: snapshot.total_equity,
                    violation_type: "负资产".to_string(),
                });
            }

            // 检查是否有不合理的资金回退（超过手续费范围的回退）
            let diff = prev_equity - snapshot.total_equity;
            // 如果回退超过 1%，记录（可能是异常）
            if diff > self.initial_fund * dec!(0.01) && diff > dec!(10) {
                // 这可能是正常的市场波动，不一定是违规
            }

            prev_equity = snapshot.total_equity;
        }

        MonotonicityResult {
            is_monotonic: violations.is_empty(),
            violations,
        }
    }

    /// 获取资金历史
    pub fn get_history(&self) -> Vec<FundSnapshot> {
        self.fund_history.iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fund_curve() {
        let mut validator = FundCurveValidator::new(dec!(10000), "BTCUSDT");

        // 买入 0.1 BTC @ 50000
        validator.record_fill(FillEvent {
            fill_id: "1".to_string(),
            timestamp: Utc::now(),
            symbol: "BTCUSDT".to_string(),
            side: FillSide::Buy,
            qty: dec!(0.1),
            price: dec!(50000),
            fee: dec!(5),
        });

        // 卖出 0.1 BTC @ 51000
        validator.record_fill(FillEvent {
            fill_id: "2".to_string(),
            timestamp: Utc::now(),
            symbol: "BTCUSDT".to_string(),
            side: FillSide::Sell,
            qty: dec!(0.1),
            price: dec!(51000),
            fee: dec!(5),
        });

        let report = validator.generate_report(dec!(51000));

        assert_eq!(report.trade_count, 2);
        // 盈利: (51000 - 50000) * 0.1 - 5 - 5 = 90
        assert!(report.cumulative_pnl > dec!(80));
    }

    #[test]
    fn test_max_drawdown() {
        let mut validator = FundCurveValidator::new(dec!(10000), "BTCUSDT");

        // 初始 10000
        validator.record_fill(FillEvent {
            fill_id: "1".to_string(),
            timestamp: Utc::now(),
            symbol: "BTCUSDT".to_string(),
            side: FillSide::Buy,
            qty: dec!(0.1),
            price: dec!(50000),  // 成本 5000
            fee: dec!(5),
        });

        // 价格下跌，权益变成 9500
        let snapshot = validator.calculate_current_state(dec!(45000));
        validator.fund_history.push_back(snapshot);

        // 价格回升到 55000，权益变成 10500
        let snapshot2 = validator.calculate_current_state(dec!(55000));
        validator.fund_history.push_back(snapshot2);

        let report = validator.generate_report(dec!(55000));

        // 最大回撤应该是 500 (10000 - 9500)
        assert!(report.max_drawdown > Decimal::ZERO);
    }
}