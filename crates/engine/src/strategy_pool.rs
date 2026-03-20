use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 策略资金分配信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyAllocation {
    /// 策略ID
    pub strategy_id: String,
    /// 分配资金
    pub allocated: Decimal,
    /// 已使用资金
    pub used: Decimal,
    /// 可用资金
    pub available: Decimal,
    /// 优先级 (0-100)
    pub priority: u8,
    /// 是否启用
    pub enabled: bool,
}

/// 策略池
///
/// 管理多策略的资金分配。
/// 支持:
/// - 按优先级分配资金
/// - 分钟/小时级资金再平衡
/// - 策略级别风控
///
/// 设计依据: 设计文档 17.3.7 StrategyPool
pub struct StrategyPool {
    /// 策略分配映射: strategy_id -> allocation
    allocations: HashMap<String, StrategyAllocation>,
    /// 总分配资金
    total_allocated: Decimal,
    /// 最后更新时间戳
    last_update_ts: i64,
    /// 分配时间周期 (秒)
    rebalance_interval_secs: i64,
}

impl Default for StrategyPool {
    fn default() -> Self {
        Self::new()
    }
}

impl StrategyPool {
    /// 创建策略池
    pub fn new() -> Self {
        Self {
            allocations: HashMap::new(),
            total_allocated: dec!(0),
            last_update_ts: 0,
            rebalance_interval_secs: 60, // 默认 1 分钟
        }
    }

    // ========== 策略注册 ==========

    /// 注册策略
    pub fn register_strategy(
        &mut self,
        strategy_id: &str,
        initial_allocation: Decimal,
        priority: u8,
    ) {
        let allocation = StrategyAllocation {
            strategy_id: strategy_id.to_string(),
            allocated: initial_allocation,
            used: dec!(0),
            available: initial_allocation,
            priority,
            enabled: true,
        };
        self.total_allocated += initial_allocation;
        self.allocations.insert(strategy_id.to_string(), allocation);
    }

    /// 注销策略
    pub fn unregister_strategy(&mut self, strategy_id: &str) {
        if let Some(allocation) = self.allocations.remove(strategy_id) {
            self.total_allocated -= allocation.allocated;
        }
    }

    // ========== 资金操作 ==========

    /// 检查策略是否可以开仓
    pub fn can_open_position(&self, strategy_id: &str, required_margin: Decimal) -> bool {
        if let Some(allocation) = self.allocations.get(strategy_id) {
            return allocation.enabled && allocation.available >= required_margin;
        }
        false
    }

    /// 预占策略资金
    pub fn reserve_margin(
        &mut self,
        strategy_id: &str,
        amount: Decimal,
    ) -> Result<(), String> {
        let allocation = self.allocations
            .get_mut(strategy_id)
            .ok_or_else(|| format!("策略 {} 未注册", strategy_id))?;

        if !allocation.enabled {
            return Err(format!("策略 {} 已禁用", strategy_id));
        }

        if allocation.available < amount {
            return Err(format!(
                "策略 {} 可用资金 {} 不足，需要 {}",
                strategy_id, allocation.available, amount
            ));
        }

        allocation.available -= amount;
        allocation.used += amount;
        Ok(())
    }

    /// 释放策略资金
    pub fn release_margin(&mut self, strategy_id: &str, amount: Decimal) {
        if let Some(allocation) = self.allocations.get_mut(strategy_id) {
            allocation.used -= amount.min(allocation.used);
            allocation.available += amount;
        }
    }

    /// 更新策略盈亏 (重新计算可用资金)
    pub fn update_strategy_pnl(&mut self, strategy_id: &str, pnl: Decimal) {
        if let Some(allocation) = self.allocations.get_mut(strategy_id) {
            allocation.available += pnl;
        }
    }

    // ========== 分配调整 ==========

    /// 设置策略分配金额
    pub fn set_allocation(&mut self, strategy_id: &str, amount: Decimal) -> Result<(), String> {
        let allocation = self.allocations
            .get_mut(strategy_id)
            .ok_or_else(|| format!("策略 {} 未注册", strategy_id))?;

        // 调整总分配
        self.total_allocated -= allocation.allocated;
        self.total_allocated += amount;

        // 调整分配
        allocation.allocated = amount;
        allocation.available = amount - allocation.used;

        Ok(())
    }

    /// 增加策略分配
    pub fn add_allocation(&mut self, strategy_id: &str, amount: Decimal) -> Result<(), String> {
        let allocation = self.allocations
            .get_mut(strategy_id)
            .ok_or_else(|| format!("策略 {} 未注册", strategy_id))?;

        allocation.allocated += amount;
        allocation.available += amount;
        self.total_allocated += amount;
        Ok(())
    }

    /// 设置策略优先级
    pub fn set_priority(&mut self, strategy_id: &str, priority: u8) {
        if let Some(allocation) = self.allocations.get_mut(strategy_id) {
            allocation.priority = priority;
        }
    }

    /// 启用/禁用策略
    pub fn set_enabled(&mut self, strategy_id: &str, enabled: bool) {
        if let Some(allocation) = self.allocations.get_mut(strategy_id) {
            allocation.enabled = enabled;
        }
    }

    // ========== 再平衡 ==========

    /// 检查是否需要再平衡
    pub fn needs_rebalance(&self, current_ts: i64) -> bool {
        current_ts - self.last_update_ts >= self.rebalance_interval_secs
    }

    /// 再平衡策略分配 (按优先级比例重新分配)
    ///
    /// 公式: 新分配 = 总资金 * (策略优先级 / 总优先级)
    pub fn rebalance(&mut self, total_funds: Decimal, current_ts: i64) {
        if self.allocations.is_empty() {
            return;
        }

        // 计算总优先级
        let total_priority: u32 = self.allocations
            .values()
            .filter(|a| a.enabled)
            .map(|a| a.priority as u32)
            .sum();

        if total_priority == 0 {
            return;
        }

        // 按优先级重新分配
        for allocation in self.allocations.values_mut() {
            if !allocation.enabled {
                continue;
            }

            let priority_ratio = allocation.priority as Decimal
                / Decimal::from(total_priority);
            let new_allocated = total_funds * priority_ratio;

            // 保留已使用资金
            let used_diff = if new_allocated > allocation.allocated {
                dec!(0)
            } else {
                allocation.allocated - new_allocated
            };

            allocation.allocated = new_allocated;
            allocation.available = new_allocated - allocation.used;
        }

        self.total_allocated = total_funds;
        self.last_update_ts = current_ts;
    }

    /// 设置再平衡间隔
    pub fn set_rebalance_interval(&mut self, secs: i64) {
        self.rebalance_interval_secs = secs;
    }

    // ========== 查询 ==========

    /// 获取策略分配
    pub fn get_allocation(&self, strategy_id: &str) -> Option<&StrategyAllocation> {
        self.allocations.get(strategy_id)
    }

    /// 获取所有策略分配
    pub fn get_all_allocations(&self) -> &HashMap<String, StrategyAllocation> {
        &self.allocations
    }

    /// 获取总分配
    pub fn total_allocated(&self) -> Decimal {
        self.total_allocated
    }

    /// 获取策略已使用资金
    pub fn used_margin(&self, strategy_id: &str) -> Decimal {
        self.allocations
            .get(strategy_id)
            .map(|a| a.used)
            .unwrap_or(dec!(0))
    }

    /// 获取策略可用资金
    pub fn available_margin(&self, strategy_id: &str) -> Decimal {
        self.allocations
            .get(strategy_id)
            .map(|a| a.available)
            .unwrap_or(dec!(0))
    }

    /// 获取启用策略数量
    pub fn enabled_count(&self) -> usize {
        self.allocations.values().filter(|a| a.enabled).count()
    }

    /// 获取策略优先级
    pub fn priority(&self, strategy_id: &str) -> Option<u8> {
        self.allocations.get(strategy_id).map(|a| a.priority)
    }

    /// 是否启用
    pub fn is_enabled(&self, strategy_id: &str) -> bool {
        self.allocations
            .get(strategy_id)
            .map(|a| a.enabled)
            .unwrap_or(false)
    }

    /// 重置策略池
    pub fn reset(&mut self) {
        self.allocations.clear();
        self.total_allocated = dec!(0);
        self.last_update_ts = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_strategy() {
        let mut pool = StrategyPool::new();
        pool.register_strategy("trend", dec!(50000), 80);

        let alloc = pool.get_allocation("trend").unwrap();
        assert_eq!(alloc.allocated, dec!(50000));
        assert_eq!(alloc.priority, 80);
        assert!(alloc.enabled);
    }

    #[test]
    fn test_reserve_margin() {
        let mut pool = StrategyPool::new();
        pool.register_strategy("trend", dec!(50000), 80);

        pool.reserve_margin("trend", dec!(10000)).unwrap();

        let alloc = pool.get_allocation("trend").unwrap();
        assert_eq!(alloc.used, dec!(10000));
        assert_eq!(alloc.available, dec!(40000));
    }

    #[test]
    fn test_release_margin() {
        let mut pool = StrategyPool::new();
        pool.register_strategy("trend", dec!(50000), 80);

        pool.reserve_margin("trend", dec!(10000)).unwrap();
        pool.release_margin("trend", dec!(5000)).unwrap();

        let alloc = pool.get_allocation("trend").unwrap();
        assert_eq!(alloc.used, dec!(5000));
        assert_eq!(alloc.available, dec!(45000));
    }

    #[test]
    fn test_rebalance() {
        let mut pool = StrategyPool::new();
        pool.register_strategy("trend", dec!(50000), 80);
        pool.register_strategy("martin", dec!(30000), 60);

        // 总资金 80000，按优先级分配
        // trend: 80/(80+60) = 57.14% -> 45714
        // martin: 60/(80+60) = 42.86% -> 34286
        pool.reserve_margin("trend", dec!(10000)).unwrap(); // 已用 10000

        pool.rebalance(dec!(80000), 1000);

        let trend = pool.get_allocation("trend").unwrap();
        let martin = pool.get_allocation("martin").unwrap();

        // 总优先级 = 80 + 60 = 140
        // trend: 80000 * 80/140 = 45714
        // martin: 80000 * 60/140 = 34286
        assert!(trend.allocated > dec!(45000));
        assert!(martin.allocated < dec!(35000));
    }

    #[test]
    fn test_disable_strategy() {
        let mut pool = StrategyPool::new();
        pool.register_strategy("trend", dec!(50000), 80);

        pool.set_enabled("trend", false);

        assert!(!pool.can_open_position("trend", dec!(1000)));
    }
}
