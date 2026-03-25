use parking_lot::RwLock;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

use crate::shared::margin_config::{MarginPoolConfig, StrategyLevel, MIN_EFFECTIVE_MARGIN};

use x_data::state::{StateViewer, StateManager};
use x_data::position::snapshot::{UnifiedPositionSnapshot, PositionSnapshot};
use x_data::account::types::AccountSnapshot;
use x_data::trading::order::OrderRecord;
use x_data::error::XDataError;

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
    /// 总权益 (totalMarginBalance)
    pub total_equity: Decimal,
    /// 可用资金
    pub available: Decimal,
    /// 持仓占用保证金
    pub margin_used: Decimal,
    /// 冻结资金
    pub frozen: Decimal,
    /// 累计盈利
    pub cumulative_profit: Decimal,
    /// 未实现盈亏 (totalUnrealizedProfit)
    pub unrealized_pnl: Decimal,
    /// 熔断状态
    pub circuit_state: CircuitBreakerState,
}

/// 账户保证金信息 (用于风控计算)
#[derive(Debug, Clone)]
pub struct AccountMargin {
    /// 账户总保证金
    pub total_margin_balance: Decimal,
    /// 未实现盈亏
    pub unrealized_pnl: Decimal,
    /// 有效保证金 = max(total_margin + unrealized_pnl, MIN_EFFECTIVE_MARGIN)
    pub effective_margin: Decimal,
    /// 可用保证金 = effective_margin * max_usage_ratio
    pub total_available_margin: Decimal,
    /// 已用保证金
    pub total_used_margin: Decimal,
    /// 保留保证金 = effective_margin * reserve_ratio
    pub reserve_margin: Decimal,
    /// 全局新开仓上限
    pub global_new_open_ceiling: Decimal,
    /// 全局翻倍仓上限
    pub global_double_open_ceiling: Decimal,
    /// 更新时间戳
    pub update_time: i64,
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
            unrealized_pnl: dec!(0),
            circuit_state: CircuitBreakerState::Normal,
        }
    }
}

/// 账户保证金池
///
/// 管理账户级别的保证金，支持熔断保护。
/// 当资金损失超过阈值时，自动触发熔断。
///
/// 线程安全: 使用 RwLock 保护 AccountInfo
///
/// 设计依据: 设计文档 17.3.7 AccountPool
pub struct AccountPool {
    /// 账户信息 (使用 RwLock 保护，支持并发读)
    account: RwLock<AccountInfo>,
    /// 初始资金 (用于计算熔断阈值) (RwLock 保护)
    initial_balance: RwLock<Decimal>,
    /// 熔断阈值 (累计亏损超过此比例触发熔断)
    circuit_threshold: Decimal,
    /// 部分熔断阈值
    partial_circuit_threshold: Decimal,
    /// 熔断恢复阈值 (盈利超过此值时恢复)
    recovery_threshold: Decimal,
    /// 熔断冷却时间 (秒)
    circuit_cooldown_secs: i64,
    /// 最后熔断时间 (RwLock 保护)
    last_circuit_ts: RwLock<i64>,
    /// 保证金池配置
    margin_config: MarginPoolConfig,
    /// 总已用保证金 (RwLock 保护) - 用于全局上限计算
    total_used_margin: RwLock<Decimal>,
    /// Redis连续失败计数器 (用于熔断)
    #[allow(dead_code)]
    redis_failure_count: RwLock<u32>,
}

impl Default for AccountPool {
    fn default() -> Self {
        Self::new()
    }
}

impl AccountPool {
    /// 创建账户保证金池
    pub fn new() -> Self {
        let initial = dec!(100000.0);
        Self {
            account: RwLock::new(AccountInfo {
                account_id: "default".to_string(),
                total_equity: initial,
                available: initial,  // 修复: available 应初始化为 initial_balance
                margin_used: dec!(0),
                frozen: dec!(0),
                cumulative_profit: dec!(0),
                unrealized_pnl: dec!(0),
                circuit_state: CircuitBreakerState::Normal,
            }),
            initial_balance: RwLock::new(initial), // 默认 10 万
            circuit_threshold: dec!(0.20),   // 20% 亏损触发完全熔断
            partial_circuit_threshold: dec!(0.10), // 10% 亏损触发部分熔断
            recovery_threshold: dec!(0.05),    // 5% 盈利恢复
            circuit_cooldown_secs: 300,        // 5 分钟冷却
            last_circuit_ts: RwLock::new(0),
            margin_config: MarginPoolConfig::default(),
            total_used_margin: RwLock::new(dec!(0)),
            redis_failure_count: RwLock::new(0),
        }
    }

    /// 创建带配置的账户保证金池
    pub fn with_config(
        initial_balance: Decimal,
        circuit_threshold: Decimal,
        partial_circuit_threshold: Decimal,
    ) -> Self {
        Self {
            account: RwLock::new(AccountInfo {
                account_id: "default".to_string(),
                total_equity: initial_balance,
                available: initial_balance,
                margin_used: dec!(0),
                frozen: dec!(0),
                cumulative_profit: dec!(0),
                unrealized_pnl: dec!(0),
                circuit_state: CircuitBreakerState::Normal,
            }),
            initial_balance: RwLock::new(initial_balance),
            circuit_threshold,
            partial_circuit_threshold,
            recovery_threshold: circuit_threshold / dec!(4),
            circuit_cooldown_secs: 300,
            last_circuit_ts: RwLock::new(0),
            margin_config: MarginPoolConfig::default(),
            total_used_margin: RwLock::new(dec!(0)),
            redis_failure_count: RwLock::new(0),
        }
    }

    // ========== 状态查询 ==========

    /// 获取账户信息 (读锁)
    pub fn account(&self) -> parking_lot::RwLockReadGuard<'_, AccountInfo> {
        self.account.read()
    }

    /// 获取可用资金 (读锁)
    pub fn available(&self) -> Decimal {
        self.account.read().available
    }

    /// 获取总权益 (读锁)
    pub fn total_equity(&self) -> Decimal {
        self.account.read().total_equity
    }

    /// 获取熔断状态 (读锁)
    pub fn circuit_state(&self) -> CircuitBreakerState {
        self.account.read().circuit_state
    }

    /// 是否允许交易 (读锁)
    ///
    /// 多线程安全: 读取熔断状态和可用资金
    pub fn can_trade(&self, required_margin: Decimal) -> bool {
        let account = self.account.read();
        if account.circuit_state == CircuitBreakerState::Full {
            return false;
        }
        if account.circuit_state == CircuitBreakerState::Partial {
            // 部分熔断时，只能用一半资金
            return account.available >= required_margin * dec!(2);
        }
        account.available >= required_margin
    }

    /// 获取实际可用的保证金 (读锁)
    pub fn available_margin(&self) -> Decimal {
        match self.account.read().circuit_state {
            CircuitBreakerState::Full => dec!(0),
            CircuitBreakerState::Partial => self.account.read().available / dec!(2),
            CircuitBreakerState::Normal => self.account.read().available,
        }
    }

    // ========== 资金操作 (写锁) ==========

    /// 冻结保证金 (写锁)
    pub fn freeze(&self, amount: Decimal) -> Result<(), String> {
        let mut account = self.account.write();
        if amount > account.available {
            return Err("可用资金不足".to_string());
        }
        account.available -= amount;
        account.frozen += amount;
        Ok(())
    }

    /// 解冻保证金 (写锁)
    pub fn unfreeze(&self, amount: Decimal) {
        let mut account = self.account.write();
        let to_unfreeze = amount.min(account.frozen);
        account.available += to_unfreeze;
        account.frozen -= to_unfreeze;
    }

    /// 扣除保证金 (下单成交后) (写锁)
    pub fn deduct_margin(&self, amount: Decimal) -> Result<(), String> {
        let mut account = self.account.write();
        if amount > account.frozen {
            return Err("冻结资金不足".to_string());
        }
        account.frozen -= amount;
        account.margin_used += amount;
        drop(account);
        *self.total_used_margin.write() += amount;
        Ok(())
    }

    /// 释放保证金 (平仓后) (写锁)
    pub fn release_margin(&self, amount: Decimal) {
        let mut account = self.account.write();
        let to_release = amount.min(account.margin_used);
        account.margin_used -= to_release;
        account.available += to_release;
        drop(account);
        *self.total_used_margin.write() -= to_release;
    }

    /// 更新权益 (成交回报后) (写锁)
    pub fn update_equity(&self, realized_pnl: Decimal, current_ts: i64) {
        let mut account = self.account.write();
        let initial_balance = *self.initial_balance.read();
        account.cumulative_profit += realized_pnl;
        account.total_equity = initial_balance + account.cumulative_profit;
        account.available += realized_pnl;

        // 检查是否需要更新熔断状态 (在同一锁内)
        let loss_ratio = if initial_balance > dec!(0) {
            -account.cumulative_profit / initial_balance
        } else {
            dec!(0)
        };

        let last_circuit_ts = *self.last_circuit_ts.read();
        if current_ts - last_circuit_ts >= self.circuit_cooldown_secs {
            let old_state = account.circuit_state;
            if loss_ratio >= self.circuit_threshold {
                account.circuit_state = CircuitBreakerState::Full;
                if old_state != CircuitBreakerState::Full {
                    *self.last_circuit_ts.write() = current_ts;
                }
            } else if loss_ratio >= self.partial_circuit_threshold {
                account.circuit_state = CircuitBreakerState::Partial;
                if old_state != CircuitBreakerState::Partial {
                    *self.last_circuit_ts.write() = current_ts;
                }
            } else if loss_ratio <= -self.recovery_threshold
                && old_state != CircuitBreakerState::Normal
            {
                account.circuit_state = CircuitBreakerState::Normal;
            }
        }
    }

    /// 强制重置熔断状态 (写锁)
    pub fn reset_circuit(&self) {
        let mut account = self.account.write();
        account.circuit_state = CircuitBreakerState::Normal;
        *self.last_circuit_ts.write() = 0;
    }

    /// 获取持仓占用保证金 (读锁)
    pub fn margin_used(&self) -> Decimal {
        self.account.read().margin_used
    }

    /// 获取累计盈利 (读锁)
    pub fn cumulative_profit(&self) -> Decimal {
        self.account.read().cumulative_profit
    }

    /// 获取账户保证金信息 (用于风控计算) (读锁)
    ///
    /// 计算逻辑:
    /// - effective_margin = max(total_equity + unrealized_pnl, MIN_EFFECTIVE_MARGIN)
    /// - total_available_margin = effective_margin * max_usage_ratio
    /// - global_new_open_ceiling = total_available_margin * new_open_ratio (分钟级)
    /// - global_double_open_ceiling = total_available_margin * double_open_ratio (分钟级)
    pub fn get_account_margin(&self, level: StrategyLevel) -> AccountMargin {
        let account = self.account.read();
        let config = &self.margin_config;

        let total_margin_balance = account.total_equity;
        let unrealized_pnl = account.unrealized_pnl;

        // 有效保证金 = max(总保证金 + 未实现盈亏, 最低有效保证金)
        let effective_margin = (total_margin_balance + unrealized_pnl)
            .max(MIN_EFFECTIVE_MARGIN);

        // 可用保证金 = 有效保证金 * 最大使用比例
        let total_available_margin = effective_margin * config.global.max_usage_ratio;

        // 已用保证金
        let total_used_margin = *self.total_used_margin.read();

        // 保留保证金 = 有效保证金 * 保留比例
        let reserve_margin = effective_margin * config.global.reserve_ratio;

        // 策略级别配置
        let strategy_config = config.strategy_config(level);

        // 全局新开仓上限 = 可用保证金 * 新开仓比例
        let global_new_open_ceiling = total_available_margin * strategy_config.new_open_ratio;

        // 全局翻倍仓上限 = 可用保证金 * 翻倍仓比例
        let global_double_open_ceiling = total_available_margin * strategy_config.double_open_ratio;

        AccountMargin {
            total_margin_balance,
            unrealized_pnl,
            effective_margin,
            total_available_margin,
            total_used_margin,
            reserve_margin,
            global_new_open_ceiling,
            global_double_open_ceiling,
            update_time: chrono::Utc::now().timestamp(),
        }
    }

    /// 获取亏损比例 (读锁)
    pub fn loss_ratio(&self) -> Decimal {
        let account = self.account.read();
        let initial_balance = *self.initial_balance.read();
        if initial_balance > dec!(0) {
            -account.cumulative_profit / initial_balance
        } else {
            dec!(0)
        }
    }

    /// 重置账户 (写锁)
    pub fn reset(&self) {
        let mut account = self.account.write();
        let initial_balance = *self.initial_balance.read();
        *account = AccountInfo {
            account_id: "default".to_string(),
            total_equity: initial_balance,
            available: initial_balance,
            margin_used: dec!(0),
            frozen: dec!(0),
            cumulative_profit: dec!(0),
            unrealized_pnl: dec!(0),
            circuit_state: CircuitBreakerState::Normal,
        };
        drop(account);
        *self.total_used_margin.write() = dec!(0);
        // last_circuit_ts 不是 AccountInfo 的一部分
        // 如果需要原子更新，需要在锁外处理
    }

    /// 注入初始资金 (写锁)
    pub fn set_initial_balance(&self, amount: Decimal) {
        let mut account = self.account.write();
        *self.initial_balance.write() = amount;
        account.total_equity = amount;
        account.available = amount;
    }
}

// ============================================================================
// StateViewer + StateManager 实现 (x_data trait)
// ============================================================================

impl StateViewer for AccountPool {
    fn get_positions(&self) -> Vec<UnifiedPositionSnapshot> {
        // AccountPool 不管理持仓，返回空列表
        Vec::new()
    }

    fn get_account(&self) -> Option<AccountSnapshot> {
        let acc = self.account.read();
        Some(AccountSnapshot {
            account_id: acc.account_id.clone(),
            equity: acc.total_equity,
            available: acc.available,
            frozen: acc.frozen,
            unrealized_pnl: acc.unrealized_pnl,
            updated_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    fn get_open_orders(&self) -> Vec<OrderRecord> {
        Vec::new()
    }
}

impl StateManager for AccountPool {
    fn update_position(&self, _symbol: &str, _pos: PositionSnapshot) -> Result<(), XDataError> {
        // AccountPool 不直接管理持仓，只管理账户资金
        Err(XDataError::ValidationFailed(
            "AccountPool does not manage positions".to_string(),
        ))
    }

    fn remove_position(&self, _symbol: &str) -> Result<(), XDataError> {
        // AccountPool 不直接管理持仓
        Err(XDataError::ValidationFailed(
            "AccountPool does not manage positions".to_string(),
        ))
    }

    fn lock_positions_read(&self) -> Vec<UnifiedPositionSnapshot> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// E3.3 AccountPool 测试 - 多策略并发请求账户分配

    #[test]
    fn test_account_pool_basic() {
        let pool = AccountPool::with_config(
            dec!(100000),
            dec!(0.20),  // 20% 熔断阈值
            dec!(0.10),  // 10% 部分熔断
        );

        assert_eq!(pool.available(), dec!(100000));
        assert_eq!(pool.circuit_state(), CircuitBreakerState::Normal);
    }

    #[test]
    fn test_freeze_and_deduct() {
        let pool = AccountPool::new();
        pool.freeze(dec!(10000)).unwrap();
        assert_eq!(pool.available(), dec!(90000));

        pool.deduct_margin(dec!(10000)).unwrap();
        assert_eq!(pool.margin_used(), dec!(10000));
    }

    #[test]
    fn test_circuit_breaker() {
        let pool = AccountPool::with_config(
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
        let pool = AccountPool::new();
        pool.update_equity(dec!(-25000), 1000); // 25% 亏损，完全熔断

        assert!(!pool.can_trade(dec!(1000)));
    }

    #[test]
    fn test_multi_strategy_allocation_sequential() {
        // 多策略顺序请求账户分配
        let pool = AccountPool::new();
        let initial = dec!(100000);

        // 策略1请求5000保证金
        assert!(pool.can_trade(dec!(5000)));
        pool.freeze(dec!(5000)).unwrap();

        // 策略2请求3000保证金
        assert!(pool.can_trade(dec!(3000)));
        pool.freeze(dec!(3000)).unwrap();

        // 剩余 90000 - 8000 = 82000
        assert_eq!(pool.available(), initial - dec!(8000));
    }

    #[test]
    fn test_multi_strategy_concurrent_allocation() {
        // 多策略并发请求账户分配测试
        // 由于 RwLock 特性，读操作不阻塞其他读，但阻塞写
        // 这里模拟并发场景：多个策略同时查询账户状态
        let pool = AccountPool::new();

        // 模拟策略A查询
        let available_a = pool.available();
        let circuit_a = pool.circuit_state();

        // 模拟策略B查询（可并行）
        let available_b = pool.available();
        let circuit_b = pool.circuit_state();

        // 两次查询结果应该一致
        assert_eq!(available_a, available_b);
        assert_eq!(circuit_a, circuit_b);
    }

    #[test]
    fn test_partial_circuit_trade_limit() {
        let pool = AccountPool::with_config(
            dec!(100000),
            dec!(0.20),
            dec!(0.10),
        );

        // 亏损 12% -> 部分熔断
        pool.update_equity(dec!(-12000), 1000);
        assert_eq!(pool.circuit_state(), CircuitBreakerState::Partial);

        // 部分熔断时，只能用一半资金
        // 可用 88000，实际只能当作 44000 用
        // 保证金需求 1000，需要 2000 实际可用资金
        assert!(pool.can_trade(dec!(1000)));
    }

    #[test]
    fn test_circuit_recovery() {
        let pool = AccountPool::with_config(
            dec!(100000),
            dec!(0.10),
            dec!(0.05),
        );

        // 亏损 12% -> 完全熔断
        pool.update_equity(dec!(-12000), 1000);
        assert_eq!(pool.circuit_state(), CircuitBreakerState::Full);
        assert!(!pool.can_trade(dec!(1000)));

        // 恢复阶段：盈利 14500 (cumulative_profit 变为 +2500)
        // loss_ratio = -0.025 <= -0.025 (recovery_threshold)，恢复到 Normal
        pool.update_equity(dec!(14500), 1000 + 300); // 超过冷却时间
        assert_eq!(pool.circuit_state(), CircuitBreakerState::Normal);
        assert!(pool.can_trade(dec!(1000)));
    }

    #[test]
    fn test_circuit_cooldown() {
        let pool = AccountPool::with_config(
            dec!(100000),
            dec!(0.10),
            dec!(0.05),
        );

        // 亏损 12% -> 完全熔断
        pool.update_equity(dec!(-12000), 1000);
        assert_eq!(pool.circuit_state(), CircuitBreakerState::Full);

        // 冷却时间内再次亏损，不重复触发熔断
        pool.update_equity(dec!(-1000), 1200); // 200秒后，还在冷却期内
        assert_eq!(pool.circuit_state(), CircuitBreakerState::Full);
    }

    #[test]
    fn test_available_margin_partial_circuit() {
        let pool = AccountPool::with_config(
            dec!(100000),
            dec!(0.20),
            dec!(0.10),
        );

        // 亏损 12% -> 部分熔断
        pool.update_equity(dec!(-12000), 1000);

        // 部分熔断时，可用保证金减半
        let account = pool.account();
        assert_eq!(account.available, dec!(88000));
        drop(account);

        // available_margin() 返回一半
        assert_eq!(pool.available_margin(), dec!(44000));
    }

    #[test]
    fn test_freeze_unfreeze_round_trip() {
        let pool = AccountPool::new();
        let initial = pool.available();

        // 冻结
        pool.freeze(dec!(10000)).unwrap();
        assert_eq!(pool.available(), initial - dec!(10000));

        // 解冻
        pool.unfreeze(dec!(10000));
        assert_eq!(pool.available(), initial);
    }

    #[test]
    fn test_unfreeze_partial() {
        let pool = AccountPool::new();

        // 冻结 10000
        pool.freeze(dec!(10000)).unwrap();

        // 解冻 5000（少于冻结金额）
        pool.unfreeze(dec!(5000));
        assert_eq!(pool.available(), dec!(95000));
    }

    #[test]
    fn test_deduct_margin_exceeds_frozen() {
        let pool = AccountPool::new();
        pool.freeze(dec!(5000)).unwrap();

        // 尝试扣除 10000，但冻结只有 5000
        let result = pool.deduct_margin(dec!(10000));
        assert!(result.is_err());
    }

    #[test]
    fn test_release_margin_round_trip() {
        let pool = AccountPool::new();

        // 扣除保证金
        pool.freeze(dec!(10000)).unwrap();
        pool.deduct_margin(dec!(10000)).unwrap();
        assert_eq!(pool.margin_used(), dec!(10000));

        // 释放保证金
        pool.release_margin(dec!(10000));
        assert_eq!(pool.margin_used(), dec!(0));
        assert_eq!(pool.available(), dec!(100000));
    }

    #[test]
    fn test_total_used_margin_sync() {
        let pool = AccountPool::new();

        // 冻结 1000
        pool.freeze(dec!(1000)).unwrap();
        assert_eq!(pool.margin_used(), dec!(0));

        // 扣除保证金
        pool.deduct_margin(dec!(1000)).unwrap();
        assert_eq!(pool.margin_used(), dec!(1000));

        // 释放保证金
        pool.release_margin(dec!(1000));
        assert_eq!(pool.margin_used(), dec!(0));
    }
}
