use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 熔断状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CircuitBreakerState {
    /// 正常
    Normal,
    /// 部分熔断 (限制开仓)
    Partial,
    /// 完全熔断 (禁止所有交易)
    Full,
}

/// 账户信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInfo {
    /// 账户ID
    pub account_id: String,
    /// 总权益
    pub total_equity: Decimal,
    /// 可用资金
    pub available: Decimal,
    /// 持仓占用保证金
    pub margin_used: Decimal,
    /// 冻结资金
    pub frozen: Decimal,
    /// 累计盈利
    pub cumulative_profit: Decimal,
    /// 熔断状态
    pub circuit_state: CircuitBreakerState,
}

impl Default for AccountInfo {
    fn default() -> Self {
        Self {
            account_id: "default".to_string(),
            total_equity: dec!(0),
            available: dec!(0),
            margin_used: dec!(0),
            frozen: dec!(0),
            cumulative_profit: dec!(0),
            circuit_state: CircuitBreakerState::Normal,
        }
    }
}

/// 账户保证金池
///
/// 管理账户级别的保证金，支持熔断保护。
/// 当资金损失超过阈值时，自动触发熔断。
///
/// 设计依据: 设计文档 17.3.7 AccountPool
pub struct AccountPool {
    /// 账户信息
    account: AccountInfo,
    /// 初始资金 (用于计算熔断阈值)
    initial_balance: Decimal,
    /// 熔断阈值 (累计亏损超过此比例触发熔断)
    circuit_threshold: Decimal,
    /// 部分熔断阈值
    partial_circuit_threshold: Decimal,
    /// 熔断恢复阈值 (盈利超过此值时恢复)
    recovery_threshold: Decimal,
    /// 熔断冷却时间 (秒)
    circuit_cooldown_secs: i64,
    /// 最后熔断时间
    last_circuit_ts: i64,
}

impl Default for AccountPool {
    fn default() -> Self {
        Self::new()
    }
}

impl AccountPool {
    /// 创建账户保证金池
    pub fn new() -> Self {
        Self {
            account: AccountInfo::default(),
            initial_balance: dec!(100000.0), // 默认 10 万
            circuit_threshold: dec!(0.20),   // 20% 亏损触发完全熔断
            partial_circuit_threshold: dec!(0.10), // 10% 亏损触发部分熔断
            recovery_threshold: dec!(0.05),    // 5% 盈利恢复
            circuit_cooldown_secs: 300,        // 5 分钟冷却
            last_circuit_ts: 0,
        }
    }

    /// 创建带配置的账户保证金池
    pub fn with_config(
        initial_balance: Decimal,
        circuit_threshold: Decimal,
        partial_circuit_threshold: Decimal,
    ) -> Self {
        Self {
            account: AccountInfo {
                account_id: "default".to_string(),
                total_equity: initial_balance,
                available: initial_balance,
                margin_used: dec!(0),
                frozen: dec!(0),
                cumulative_profit: dec!(0),
                circuit_state: CircuitBreakerState::Normal,
            },
            initial_balance,
            circuit_threshold,
            partial_circuit_threshold,
            recovery_threshold: circuit_threshold / dec!(4),
            circuit_cooldown_secs: 300,
            last_circuit_ts: 0,
        }
    }

    // ========== 状态查询 ==========

    /// 获取账户信息
    pub fn account(&self) -> &AccountInfo {
        &self.account
    }

    /// 获取可用资金
    pub fn available(&self) -> Decimal {
        self.account.available
    }

    /// 获取总权益
    pub fn total_equity(&self) -> Decimal {
        self.account.total_equity
    }

    /// 获取熔断状态
    pub fn circuit_state(&self) -> CircuitBreakerState {
        self.account.circuit_state
    }

    /// 是否允许交易
    pub fn can_trade(&self, required_margin: Decimal) -> bool {
        if self.account.circuit_state == CircuitBreakerState::Full {
            return false;
        }
        if self.account.circuit_state == CircuitBreakerState::Partial {
            // 部分熔断时，只能用一半资金
            return self.account.available >= required_margin * dec!(2);
        }
        self.account.available >= required_margin
    }

    /// 获取实际可用的保证金
    pub fn available_margin(&self) -> Decimal {
        match self.account.circuit_state {
            CircuitBreakerState::Full => dec!(0),
            CircuitBreakerState::Partial => self.account.available / dec!(2),
            CircuitBreakerState::Normal => self.account.available,
        }
    }

    // ========== 资金操作 ==========

    /// 冻结保证金
    pub fn freeze(&mut self, amount: Decimal) -> Result<(), String> {
        if amount > self.account.available {
            return Err("可用资金不足".to_string());
        }
        self.account.available -= amount;
        self.account.frozen += amount;
        Ok(())
    }

    /// 解冻保证金
    pub fn unfreeze(&mut self, amount: Decimal) {
        let to_unfreeze = amount.min(self.account.frozen);
        self.account.available += to_unfreeze;
        self.account.frozen -= to_unfreeze;
    }

    /// 扣除保证金 (下单成交后)
    pub fn deduct_margin(&mut self, amount: Decimal) -> Result<(), String> {
        if amount > self.account.frozen {
            return Err("冻结资金不足".to_string());
        }
        self.account.frozen -= amount;
        self.account.margin_used += amount;
        Ok(())
    }

    /// 释放保证金 (平仓后)
    pub fn release_margin(&mut self, amount: Decimal) {
        let to_release = amount.min(self.account.margin_used);
        self.account.margin_used -= to_release;
        self.account.available += to_release;
    }

    /// 更新权益 (成交回报后)
    pub fn update_equity(&mut self, realized_pnl: Decimal, current_ts: i64) {
        self.account.cumulative_profit += realized_pnl;
        self.account.total_equity = self.initial_balance + self.account.cumulative_profit;
        self.account.available += realized_pnl;

        // 检查是否需要更新熔断状态
        self.update_circuit_state(current_ts);
    }

    /// 检查并更新熔断状态
    fn update_circuit_state(&mut self, current_ts: i64) {
        // 计算亏损比例
        let loss_ratio = if self.initial_balance > dec!(0) {
            -self.account.cumulative_profit / self.initial_balance
        } else {
            dec!(0)
        };

        // 检查是否在冷却期
        if current_ts - self.last_circuit_ts < self.circuit_cooldown_secs {
            return; // 冷却期内不更新
        }

        let old_state = self.account.circuit_state;

        if loss_ratio >= self.circuit_threshold {
            self.account.circuit_state = CircuitBreakerState::Full;
            if old_state != CircuitBreakerState::Full {
                self.last_circuit_ts = current_ts;
            }
        } else if loss_ratio >= self.partial_circuit_threshold {
            self.account.circuit_state = CircuitBreakerState::Partial;
            if old_state != CircuitBreakerState::Partial {
                self.last_circuit_ts = current_ts;
            }
        } else if loss_ratio <= -self.recovery_threshold
            && old_state != CircuitBreakerState::Normal
        {
            // 盈利恢复到阈值以上，解除熔断
            self.account.circuit_state = CircuitBreakerState::Normal;
        }
    }

    /// 强制重置熔断状态
    pub fn reset_circuit(&mut self) {
        self.account.circuit_state = CircuitBreakerState::Normal;
        self.last_circuit_ts = 0;
    }

    /// 获取持仓占用保证金
    pub fn margin_used(&self) -> Decimal {
        self.account.margin_used
    }

    /// 获取累计盈利
    pub fn cumulative_profit(&self) -> Decimal {
        self.account.cumulative_profit
    }

    /// 获取亏损比例
    pub fn loss_ratio(&self) -> Decimal {
        if self.initial_balance > dec!(0) {
            -self.account.cumulative_profit / self.initial_balance
        } else {
            dec!(0)
        }
    }

    /// 重置账户
    pub fn reset(&mut self) {
        self.account = AccountInfo {
            account_id: "default".to_string(),
            total_equity: self.initial_balance,
            available: self.initial_balance,
            margin_used: dec!(0),
            frozen: dec!(0),
            cumulative_profit: dec!(0),
            circuit_state: CircuitBreakerState::Normal,
        };
        self.last_circuit_ts = 0;
    }

    /// 注入初始资金
    pub fn set_initial_balance(&mut self, amount: Decimal) {
        self.initial_balance = amount;
        self.account.total_equity = amount;
        self.account.available = amount;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_account_pool_basic() {
        let mut pool = AccountPool::with_config(
            dec!(100000),
            dec!(0.20),  // 20% 熔断阈值
            dec!(0.10),  // 10% 部分熔断
        );

        assert_eq!(pool.available(), dec!(100000));
        assert_eq!(pool.circuit_state(), CircuitBreakerState::Normal);
    }

    #[test]
    fn test_freeze_and_deduct() {
        let mut pool = AccountPool::new();
        pool.freeze(dec!(10000)).unwrap();
        assert_eq!(pool.available(), dec!(90000));

        pool.deduct_margin(dec!(10000)).unwrap();
        assert_eq!(pool.margin_used(), dec!(10000));
    }

    #[test]
    fn test_circuit_breaker() {
        let mut pool = AccountPool::with_config(
            dec!(100000),
            dec!(0.10),  // 10% 熔断阈值
            dec!(0.05),  // 5% 部分熔断
        );

        // 亏损 8%，触发部分熔断
        pool.update_equity(dec!(-8000), 1000);
        assert_eq!(pool.circuit_state(), CircuitBreakerState::Partial);

        // 亏损 15%，触发完全熔断
        pool.update_equity(dec!(-7000), 2000);
        assert_eq!(pool.circuit_state(), CircuitBreakerState::Full);
    }

    #[test]
    fn test_cannot_trade_when_circuit() {
        let mut pool = AccountPool::new();
        pool.update_equity(dec!(-25000), 1000); // 25% 亏损，完全熔断

        assert!(!pool.can_trade(dec!(1000)));
    }
}
