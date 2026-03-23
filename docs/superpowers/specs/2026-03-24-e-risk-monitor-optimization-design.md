# e_risk_monitor 全面优化设计文档

---
Author: 软件架构师
Created: 2026-03-24
GSD-Phase: e-risk-optimization
Status: draft
---

## 1. 概述

### 1.1 优化目标

1. **资金安全**：统一预占与冻结系统，解决双系统数据不一致
2. **品种限制**：单品种最大持仓限额，防止过度集中
3. **交易频率**：防止过度交易，保护账户
4. **性能优化**：HashMap/Vec 查询优化，提升高频性能
5. **动态杠杆**：高波动时自动降杠杆，降低风险

### 1.2 现状问题

| 问题 | 严重度 | 说明 |
|------|--------|------|
| 预占与冻结系统分离 | P0 | OrderCheck 和 AccountPool 各自管理冻结资金，可能不一致 |
| total_used_margin 从不更新 | P0 | AccountPool.get_account_margin 读取但从未写入 |
| 缺少品种限额 | P1 | 无法限制单品种最大持仓 |
| 缺少频率限制 | P1 | 无法防止过度交易 |
| Vec/HashMap 性能 | P1 | 交易记录查询效率低 |
| 缺少动态杠杆 | P2 | 高波动时无法自动降杠杆 |

---

## 2. 架构设计

### 2.1 新增模块：guard/

新增 `guard/` 子模块，作为风控的守护层：

```
e_risk_monitor/src/
├── guard/                       # 新增
│   ├── mod.rs
│   ├── symbol_limit.rs           # 品种限额检查
│   ├── rate_limiter.rs          # 频率限制
│   └── dynamic_leverage.rs       # 动态杠杆计算
│
├── risk/                        # 现有
│   └── ...
│
├── position/
│   ├── position_manager.rs
│   ├── position_exclusion.rs
│   └── position_limit.rs        # 持仓限额 (从 guard 迁移)
```

### 2.2 guard 模块职责

```
┌─────────────────────────────────────────────────────────┐
│                     guard/ 守护层                        │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  ┌─────────────────┐  ┌─────────────────┐             │
│  │  SymbolLimit    │  │  RateLimiter    │             │
│  │  品种限额        │  │  频率限制        │             │
│  └────────┬────────┘  └────────┬────────┘             │
│           │                    │                        │
│           └─────────┬──────────┘                        │
│                     ▼                                  │
│            ┌─────────────────┐                         │
│            │ DynamicLeverage │                         │
│            │ 动态杠杆        │                         │
│            └────────┬────────┘                         │
│                     │                                  │
└─────────────────────┼──────────────────────────────────┘
                      │
                      ▼
┌─────────────────────────────────────────────────────────┐
│                     risk/ 风控层                         │
├─────────────────────────────────────────────────────────┤
│  RiskPreChecker  →  RiskReChecker  →  OrderCheck      │
└─────────────────────────────────────────────────────────┘
```

---

## 3. 详细设计

### 3.1 guard/symbol_limit.rs — 品种限额

```rust
/// 单品种持仓限制
#[derive(Debug, Clone)]
pub struct SymbolPositionLimit {
    /// 单品种最大名义价值 (默认 5000 USDT)
    pub max_notional: Decimal,
    /// 单品种最大数量
    pub max_qty: Decimal,
    /// 是否启用严格模式 (超限直接拒绝)
    pub strict_mode: bool,
}

impl Default for SymbolPositionLimit {
    fn default() -> Self {
        Self {
            max_notional: dec!(5000.0),
            max_qty: dec!(0),
            strict_mode: true,
        }
    }
}

/// 全局持仓限制
#[derive(Debug, Clone)]
pub struct GlobalPositionLimit {
    /// 全局最大名义价值
    pub max_total_notional: Decimal,
    /// 最大交易品种数
    pub max_symbol_count: u32,
    /// 全局最大持仓数量
    pub max_total_qty: Decimal,
}

impl Default for GlobalPositionLimit {
    fn default() -> Self {
        Self {
            max_total_notional: dec!(50000.0),
            max_symbol_count: 10,
            max_total_qty: dec!(0),
        }
    }
}

/// 品种限额检查器
#[derive(Debug, Clone)]
pub struct SymbolLimitGuard {
    /// 单品种限制
    symbol_limit: SymbolPositionLimit,
    /// 全局限制
    global_limit: GlobalPositionLimit,
    /// 当前持仓名义价值 (symbol -> notional)
    current_notionals: FnvHashMap<String, Decimal>,
    /// 当前持仓数量 (symbol -> qty)
    current_quantities: FnvHashMap<String, Decimal>,
}

impl SymbolLimitGuard {
    /// 创建品种限额检查器
    pub fn new(symbol_limit: SymbolPositionLimit, global_limit: GlobalPositionLimit) -> Self {
        Self {
            symbol_limit,
            global_limit,
            current_notionals: FnvHashMap::default(),
            current_quantities: FnvHashMap::default(),
        }
    }

    /// 预检订单是否允许
    pub fn pre_check(
        &self,
        symbol: &str,
        order_notional: Decimal,
        order_qty: Decimal,
    ) -> Result<(), EngineError> {
        // 1. 检查单品种限额
        let current_notional = self.current_notionals.get(symbol).copied().unwrap_or(dec!(0));
        let current_qty = self.current_quantities.get(symbol).copied().unwrap_or(dec!(0));

        let new_notional = current_notional + order_notional;
        let new_qty = current_qty + order_qty;

        // 检查单品种名义价值限额
        if self.symbol_limit.max_notional > dec!(0) {
            if new_notional > self.symbol_limit.max_notional {
                return Err(EngineError::PositionLimitExceeded(format!(
                    "品种 {} 名义价值 {} 超过单品种限额 {}",
                    symbol, new_notional, self.symbol_limit.max_notional
                )));
            }
        }

        // 检查单品种数量限额
        if self.symbol_limit.max_qty > dec!(0) {
            if new_qty > self.symbol_limit.max_qty {
                return Err(EngineError::PositionLimitExceeded(format!(
                    "品种 {} 数量 {} 超过单品种限额 {}",
                    symbol, new_qty, self.symbol_limit.max_qty
                )));
            }
        }

        // 2. 检查全局限额
        let total_notional: Decimal = self.current_notionals.values().sum();
        let new_total_notional = total_notional + order_notional;

        if self.global_limit.max_total_notional > dec!(0) {
            if new_total_notional > self.global_limit.max_total_notional {
                return Err(EngineError::PositionLimitExceeded(format!(
                    "全局名义价值 {} 超过全局限额 {}",
                    new_total_notional, self.global_limit.max_total_notional
                )));
            }
        }

        // 检查全局品种数限额
        let current_symbols = self.current_notionals.len() as u32;
        if !self.current_notionals.contains_key(symbol) && current_symbols >= self.global_limit.max_symbol_count {
            return Err(EngineError::PositionLimitExceeded(format!(
                "交易品种数 {} 达到全局上限 {}",
                current_symbols, self.global_limit.max_symbol_count
            )));
        }

        Ok(())
    }

    /// 更新持仓记录 (订单成交后调用)
    pub fn update_position(&mut self, symbol: &str, notional: Decimal, qty: Decimal) {
        self.current_notionals.insert(symbol.to_string(), notional);
        self.current_quantities.insert(symbol.to_string(), qty);
    }

    /// 减少持仓记录 (平仓后调用)
    pub fn reduce_position(&mut self, symbol: &str, notional: Decimal, qty: Decimal) {
        if let Some(current_notional) = self.current_notionals.get_mut(symbol) {
            *current_notional = (*current_notional - notional).max(dec!(0));
        }
        if let Some(current_qty) = self.current_quantities.get_mut(symbol) {
            *current_qty = (*current_qty - qty).max(dec!(0));
        }
    }

    /// 获取当前品种列表
    pub fn current_symbols(&self) -> Vec<&String> {
        self.current_notionals.keys().collect()
    }

    /// 获取指定品种当前名义价值
    pub fn get_notional(&self, symbol: &str) -> Decimal {
        self.current_notionals.get(symbol).copied().unwrap_or(dec!(0))
    }
}
```

### 3.2 guard/rate_limiter.rs — 频率限制

```rust
use std::collections::VecDeque;

/// 订单记录
#[derive(Debug, Clone)]
struct OrderRecord {
    symbol: String,
    timestamp: i64,
}

/// 频率限制器
///
/// 使用滑动窗口算法限制单位时间内的订单数量。
#[derive(Debug, Clone)]
pub struct RateLimiter {
    /// 单品种每分钟最大订单数
    per_symbol_per_minute: u32,
    /// 全局每分钟最大订单数
    global_per_minute: u32,
    /// 滑动窗口记录 (最多保留 10 分钟)
    recent_orders: VecDeque<OrderRecord>,
    /// 窗口大小 (秒)
    window_secs: i64,
}

impl RateLimiter {
    /// 创建频率限制器
    pub fn new(per_symbol_per_minute: u32, global_per_minute: u32) -> Self {
        Self {
            per_symbol_per_minute,
            global_per_minute,
            recent_orders: VecDeque::new(),
            window_secs: 60, // 1分钟窗口
        }
    }

    /// 检查是否允许下单
    pub fn check(&self, symbol: &str, current_ts: i64) -> Result<(), EngineError> {
        let window_start = current_ts - self.window_secs;

        // 清理过期记录
        let valid_orders: Vec<&OrderRecord> = self.recent_orders
            .iter()
            .filter(|o| o.timestamp > window_start)
            .collect();

        // 1. 检查单品种频率
        let symbol_orders = valid_orders.iter()
            .filter(|o| o.symbol == symbol)
            .count() as u32;

        if symbol_orders >= self.per_symbol_per_minute {
            return Err(EngineError::RiskCheckFailed(format!(
                "品种 {} 频率超限: {}/{} (1分钟内)",
                symbol, symbol_orders, self.per_symbol_per_minute
            )));
        }

        // 2. 检查全局频率
        if valid_orders.len() as u32 >= self.global_per_minute {
            return Err(EngineError::RiskCheckFailed(format!(
                "全局频率超限: {}/{} (1分钟内)",
                valid_orders.len(), self.global_per_minute
            )));
        }

        Ok(())
    }

    /// 记录订单
    pub fn record(&mut self, symbol: String, timestamp: i64) {
        self.recent_orders.push_back(OrderRecord { symbol, timestamp });
    }

    /// 清理过期记录
    pub fn cleanup(&mut self, current_ts: i64) {
        let window_start = current_ts - self.window_secs;
        while let Some(oldest) = self.recent_orders.front() {
            if oldest.timestamp <= window_start {
                self.recent_orders.pop_front();
            } else {
                break;
            }
        }
    }

    /// 获取指定品种当前订单数
    pub fn symbol_order_count(&self, symbol: &str, current_ts: i64) -> u32 {
        let window_start = current_ts - self.window_secs;
        self.recent_orders
            .iter()
            .filter(|o| o.symbol == symbol && o.timestamp > window_start)
            .count() as u32
    }

    /// 获取全局当前订单数
    pub fn global_order_count(&self, current_ts: i64) -> u32 {
        let window_start = current_ts - self.window_secs;
        self.recent_orders
            .iter()
            .filter(|o| o.timestamp > window_start)
            .count() as u32
    }
}
```

### 3.3 guard/dynamic_leverage.rs — 动态杠杆

```rust
use std::collections::HashMap;

/// 波动级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VolatilityLevel {
    /// 低波动
    Low,
    /// 正常
    Normal,
    /// 高波动
    High,
    /// 极端波动
    Extreme,
}

/// 动态杠杆配置
#[derive(Debug, Clone)]
pub struct DynamicLeverageConfig {
    /// 低波动阈值
    pub low_volatility_threshold: Decimal,
    /// 正常波动阈值 (低于此值使用基准杠杆)
    pub normal_volatility_threshold: Decimal,
    /// 高波动阈值 (触发降杠杆)
    pub high_volatility_threshold: Decimal,
    /// 极端波动阈值 (强制使用最低杠杆)
    pub extreme_volatility_threshold: Decimal,
    /// 各波动级别对应的杠杆倍数
    pub leverage_by_level: HashMap<VolatilityLevel, Decimal>,
}

impl Default for DynamicLeverageConfig {
    fn default() -> Self {
        let mut leverage_by_level = HashMap::new();
        leverage_by_level.insert(VolatilityLevel::Low, dec!(15));
        leverage_by_level.insert(VolatilityLevel::Normal, dec!(10));
        leverage_by_level.insert(VolatilityLevel::High, dec!(5));
        leverage_by_level.insert(VolatilityLevel::Extreme, dec!(2));

        Self {
            low_volatility_threshold: dec!(0.01),    // 1%
            normal_volatility_threshold: dec!(0.03),  // 3%
            high_volatility_threshold: dec!(0.05),   // 5%
            extreme_volatility_threshold: dec!(0.10), // 10%
            leverage_by_level,
        }
    }
}

/// 动态杠杆计算器
#[derive(Debug, Clone)]
pub struct DynamicLeverageCalculator {
    config: DynamicLeverageConfig,
}

impl DynamicLeverageCalculator {
    /// 创建动态杠杆计算器
    pub fn new(config: DynamicLeverageConfig) -> Self {
        Self { config }
    }

    /// 获取波动级别
    pub fn get_volatility_level(&self, volatility: Decimal) -> VolatilityLevel {
        if volatility >= self.config.extreme_volatility_threshold {
            VolatilityLevel::Extreme
        } else if volatility >= self.config.high_volatility_threshold {
            VolatilityLevel::High
        } else if volatility >= self.config.normal_volatility_threshold {
            VolatilityLevel::Normal
        } else {
            VolatilityLevel::Low
        }
    }

    /// 计算当前应该使用的杠杆
    ///
    /// # 参数
    /// - `current_volatility`: 当前波动率 (TR比率或价格波动)
    /// - `base_leverage`: 基准杠杆倍数
    ///
    /// # 返回
    /// - 实际应使用的杠杆倍数 (不会超过基准杠杆)
    pub fn calculate_leverage(
        &self,
        current_volatility: Decimal,
        base_leverage: Decimal,
    ) -> Decimal {
        let level = self.get_volatility_level(current_volatility);

        // 获取该波动级别对应的杠杆
        let level_leverage = self.config.leverage_by_level
            .get(&level)
            .copied()
            .unwrap_or(base_leverage);

        // 返回较小值：不能超过基准杠杆
        level_leverage.min(base_leverage)
    }

    /// 判断是否需要降杠杆
    pub fn should_reduce_leverage(&self, current_volatility: Decimal) -> bool {
        current_volatility >= self.config.high_volatility_threshold
    }

    /// 获取当前杠杆调整因子 (0.0 ~ 1.0)
    ///
    /// 用于多品种分配时按比例调整。
    pub fn leverage_factor(&self, current_volatility: Decimal) -> Decimal {
        let level = self.get_volatility_level(current_volatility);
        match level {
            VolatilityLevel::Low => dec!(1.0),
            VolatilityLevel::Normal => dec!(1.0),
            VolatilityLevel::High => dec!(0.5),
            VolatilityLevel::Extreme => dec!(0.2),
        }
    }
}
```

### 3.4 guard/mod.rs — 模块入口

```rust
pub mod symbol_limit;
pub mod rate_limiter;
pub mod dynamic_leverage;

pub use symbol_limit::{SymbolPositionLimit, GlobalPositionLimit, SymbolLimitGuard};
pub use rate_limiter::RateLimiter;
pub use dynamic_leverage::{DynamicLeverageConfig, DynamicLeverageCalculator, VolatilityLevel};
```

---

## 4. 性能优化设计

### 4.1 OrderCheck 使用 FnvHashMap

```rust
// order_check.rs

// 原:
reservations: RwLock<HashMap<String, OrderReservation>>,

// 改:
use fnv::FnvHashMap;
reservations: RwLock<FnvHashMap<String, OrderReservation>>,
```

### 4.2 PersistenceService 添加索引

```rust
// persistence.rs

pub struct PersistenceService {
    // ... 现有字段
    trade_records: Vec<TradeRecord>,
    // 新增索引
    trade_index_by_symbol: FnvHashMap<String, Vec<usize>>,  // symbol -> record indices
    trade_index_by_strategy: FnvHashMap<String, Vec<usize>>, // strategy_id -> record indices
}

impl PersistenceService {
    /// 保存交易记录 (更新索引)
    pub fn save_trade(&mut self, record: TradeRecord) {
        let idx = self.trade_records.len();
        self.trade_records.push(record);

        // 更新索引
        let symbol = &self.trade_records[idx].symbol;
        let strategy_id = &self.trade_records[idx].strategy_id;

        self.trade_index_by_symbol
            .entry(symbol.clone())
            .or_insert_with(Vec::new)
            .push(idx);

        self.trade_index_by_strategy
            .entry(strategy_id.clone())
            .or_insert_with(Vec::new)
            .push(idx);
    }

    /// 按品种查询 (使用索引)
    pub fn get_trades_by_symbol(&self, symbol: &str) -> Vec<&TradeRecord> {
        self.trade_index_by_symbol
            .get(symbol)
            .map(|indices| indices.iter().filter_map(|&i| self.trade_records.get(i)).collect())
            .unwrap_or_default()
    }

    /// 按策略查询 (使用索引)
    pub fn get_trades_by_strategy(&self, strategy_id: &str) -> Vec<&TradeRecord> {
        self.trade_index_by_strategy
            .get(strategy_id)
            .map(|indices| indices.iter().filter_map(|&i| self.trade_records.get(i)).collect())
            .unwrap_or_default()
    }
}
```

---

## 5. 统一预占与冻结系统 (P0)

### 5.1 问题分析

当前系统存在两套独立的资金冻结系统：

```
OrderCheck.reserve()    → 冻结在 OrderCheck.reservations
AccountPool.freeze()   → 冻结在 AccountPool.account.frozen
```

这两套系统没有同步，可能导致：
1. OrderCheck 通过但 AccountPool 余额不足
2. 双重冻结导致可用资金计算错误

### 5.2 优化方案

新增 `FundGuard` (资金守护)，作为统一的资金管理层：

```rust
/// 资金守护
///
/// 统一的资金管理层，确保预占、冻结、扣除的一致性。
/// 替代原有的 OrderCheck 预占系统。
#[derive(Debug, Clone)]
pub struct FundGuard {
    /// 账户池引用
    account_pool: Arc<AccountPool>,
    /// 当前预占总额
    total_reserved: RwLock<Decimal>,
    /// 预占记录 (order_id -> amount)
    reservations: RwLock<FnvHashMap<String, Decimal>>,
}

impl FundGuard {
    /// 预检并预占资金
    pub fn pre_reserve(
        &self,
        order_id: &str,
        amount: Decimal,
    ) -> Result<Decimal, EngineError> {
        let available = self.account_pool.available();

        // 1. 资金充足性检查
        if available < amount {
            return Err(EngineError::InsufficientFund(format!(
                "可用资金 {} 不足预占 {}",
                available, amount
            )));
        }

        // 2. 检查是否已存在预占
        {
            let reservations = self.reservations.read();
            if reservations.contains_key(order_id) {
                return Err(EngineError::RiskCheckFailed(format!(
                    "订单 {} 已有预占记录",
                    order_id
                )));
            }
        }

        // 3. 冻结资金
        self.account_pool.freeze(amount)?;

        // 4. 记录预占
        {
            let mut reservations = self.reservations.write();
            reservations.insert(order_id.to_string(), amount);
        }

        // 5. 更新预占总额
        {
            let mut total = self.total_reserved.write();
            *total += amount;
        }

        Ok(amount)
    }

    /// 确认预占 (扣除保证金)
    pub fn confirm(&self, order_id: &str) -> Result<(), EngineError> {
        let amount = {
            let mut reservations = self.reservations.write();
            reservations.remove(order_id)
                .ok_or_else(|| EngineError::RiskCheckFailed(format!(
                    "订单 {} 没有预占记录",
                    order_id
                )))?
        };

        // 从冻结转为已用
        self.account_pool.deduct_margin(amount)?;

        // 更新预占总额
        {
            let mut total = self.total_reserved.write();
            *total -= amount;
        }

        Ok(())
    }

    /// 取消预占 (释放冻结)
    pub fn cancel(&self, order_id: &str) -> Result<Decimal, EngineError> {
        let amount = {
            let mut reservations = self.reservations.write();
            reservations.remove(order_id)
                .ok_or_else(|| EngineError::RiskCheckFailed(format!(
                    "订单 {} 没有预占记录",
                    order_id
                )))?
        };

        // 释放冻结资金
        self.account_pool.unfreeze(amount);

        // 更新预占总额
        {
            let mut total = self.total_reserved.write();
            *total -= amount;
        }

        Ok(amount)
    }

    /// 获取总预占金额
    pub fn total_reserved(&self) -> Decimal {
        *self.total_reserved.read()
    }
}
```

---

## 6. 修复 total_used_margin 问题

### 6.1 问题

```rust
// account_pool.rs 第333行
pub fn get_account_margin(&self, level: StrategyLevel) -> AccountMargin {
    let total_used_margin = *self.total_used_margin.read();
    // ⚠️ total_used_margin 永远是 0，从未更新
}
```

### 6.2 修复方案

在 `deduct_margin` 和 `release_margin` 时同步更新 `total_used_margin`：

```rust
/// 扣除保证金 (下单成交后)
pub fn deduct_margin(&self, amount: Decimal) -> Result<(), String> {
    let mut account = self.account.write();
    if amount > account.frozen {
        return Err("冻结资金不足".to_string());
    }
    account.frozen -= amount;
    account.margin_used += amount;

    // 新增：更新 total_used_margin
    drop(account);
    *self.total_used_margin.write() += amount;

    Ok(())
}

/// 释放保证金 (平仓后)
pub fn release_margin(&self, amount: Decimal) {
    let mut account = self.account.write();
    let to_release = amount.min(account.margin_used);
    account.margin_used -= to_release;
    account.available += to_release;

    // 新增：更新 total_used_margin
    drop(account);
    *self.total_used_margin.write() -= to_release;
}
```

---

## 7. lib.rs 导出更新

```rust
// lib.rs

pub mod guard;  // 新增

pub mod risk;
pub mod position;
pub mod persistence;
pub mod shared;

// 导出
pub use guard::{
    SymbolPositionLimit, GlobalPositionLimit, SymbolLimitGuard,
    RateLimiter, DynamicLeverageConfig, DynamicLeverageCalculator, VolatilityLevel,
    FundGuard,
};

pub use risk::{
    RiskPreChecker, VolatilityMode, RiskReChecker,
    OrderCheck, OrderCheckResult, OrderReservation,
    ThresholdConstants, calculate_hour_open_notional, calculate_minute_open_notional,
    calculate_open_qty_from_notional, MinuteOpenResult,
};

pub use position::{
    Direction, LocalPosition, LocalPositionManager,
    PositionStats, PositionDirection, PositionExclusionChecker, PositionInfo,
};

pub use persistence::{
    PersistenceService, KLineCache, KLineData, TradeRecord,
    PositionSnapshot, PersistenceConfig, PersistenceStats,
};

pub use shared::{
    AccountInfo, AccountMargin, AccountPool, CircuitBreakerState,
    GlobalMarginConfig, MarginPoolConfig, MinuteOpenConfig, StrategyLevel,
    MarketStatus, MarketStatusDetector, PinIntensity, PinDetection,
    PnlManager, RoundGuard, RoundGuardScope,
    // 从 a_common::backup 获取内存备份类型
    MemoryBackup, memory_backup_dir,
};
```

---

## 8. 测试设计

### 8.1 SymbolLimitGuard 测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_limit_basic() {
        let guard = SymbolLimitGuard::new(
            SymbolPositionLimit {
                max_notional: dec!(5000),
                ..Default::default()
            },
            GlobalPositionLimit::default(),
        );

        // 首笔订单 3000，通过
        assert!(guard.pre_check("BTC", dec!(3000), dec!(0)).is_ok());

        // 第二笔订单 3000，总额 6000 > 5000，拒绝
        assert!(guard.pre_check("BTC", dec!(3000), dec!(0)).is_err());
    }

    #[test]
    fn test_global_symbol_count_limit() {
        let guard = SymbolLimitGuard::new(
            SymbolPositionLimit::default(),
            GlobalPositionLimit {
                max_symbol_count: 2,
                ..Default::default()
            },
        );

        // BTC 通过
        assert!(guard.pre_check("BTC", dec!(1000), dec!(0)).is_ok());
        guard.update_position("BTC", dec!(1000), dec!(0));

        // ETH 通过
        assert!(guard.pre_check("ETH", dec!(1000), dec!(0)).is_ok());
        guard.update_position("ETH", dec!(1000), dec!(0));

        // SOL 品种数已达上限，拒绝
        assert!(guard.pre_check("SOL", dec!(1000), dec!(0)).is_err());
    }
}
```

### 8.2 RateLimiter 测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter_per_symbol() {
        let limiter = RateLimiter::new(
            per_symbol_per_minute: 3,
            global_per_minute: 10,
        );
        let ts = 1000;

        // 前3笔订单通过
        assert!(limiter.check("BTC", ts).is_ok());
        limiter.record("BTC".to_string(), ts);
        assert!(limiter.check("BTC", ts).is_ok());
        limiter.record("BTC".to_string(), ts);
        assert!(limiter.check("BTC", ts).is_ok());
        limiter.record("BTC".to_string(), ts);

        // 第4笔拒绝
        assert!(limiter.check("BTC", ts).is_err());
    }
}
```

### 8.3 DynamicLeverageCalculator 测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_leverage_high_volatility() {
        let calc = DynamicLeverageCalculator::new(DynamicLeverageConfig::default());

        // 高波动 7% > 5% 阈值，应该降杠杆到 5x
        let leverage = calc.calculate_leverage(dec!(0.07), dec!(10));
        assert_eq!(leverage, dec!(5));
    }

    #[test]
    fn test_calculate_leverage_normal() {
        let calc = DynamicLeverageCalculator::new(DynamicLeverageConfig::default());

        // 正常波动 2% < 3% 阈值，使用基准杠杆 10x
        let leverage = calc.calculate_leverage(dec!(0.02), dec!(10));
        assert_eq!(leverage, dec!(10));
    }
}
```

---

## 9. 实现计划

| 步骤 | 任务 | 优先级 |
|------|------|--------|
| 1 | 新增 guard/ 目录和 mod.rs | P0 |
| 2 | 实现 SymbolLimitGuard | P1 |
| 3 | 实现 RateLimiter | P1 |
| 4 | 实现 DynamicLeverageCalculator | P2 |
| 5 | 实现 FundGuard (统一预占系统) | P0 |
| 6 | 修复 total_used_margin 更新问题 | P0 |
| 7 | OrderCheck 改用 FnvHashMap | P1 |
| 8 | PersistenceService 添加索引 | P1 |
| 9 | 更新 lib.rs 导出 | P0 |
| 10 | 编写单元测试 | P1 |
| 11 | 更新文档 | P2 |

---

## 10. 向后兼容性

- 新增的 `guard/` 模块不影响现有 API
- `FundGuard` 作为可选的统一预占系统，原 `OrderCheck` 保留但标记为 deprecated
- `SymbolLimitGuard`、`RateLimiter`、`DynamicLeverageCalculator` 均为新增，可选择性使用
