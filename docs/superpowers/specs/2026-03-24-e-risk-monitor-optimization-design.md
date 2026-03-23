# e_risk_monitor 全面优化设计文档

---
Author: 软件架构师
Created: 2026-03-24
GSD-Phase: e-risk-optimization
Status: reviewed
---

## 1. 概述

### 1.1 优化目标

1. **资金安全**：统一预占与冻结系统，解决双系统数据不一致
2. **品种限制**：趋势策略单品种最大持仓限额，防止过度集中
3. **交易频率**：防止过度交易，保护账户
4. **性能优化**：HashMap/Vec 查询优化，提升高频性能
5. **动态杠杆**：插针高波动策略高波动时自动降杠杆，降低风险

### 1.2 风控类型划分

```
┌─────────────────────────────────────────────────────────────┐
│                      公共风控 (所有策略)                      │
├─────────────────────────────────────────────────────────────┤
│  RiskPreChecker  │  RiskReChecker  │  AccountPool          │
│  OrderCheck      │  MarketStatusDetector                   │
│  ThresholdConstants │ MarginPoolConfig                       │
│  RateLimiter (通用频率限制)                                  │
└─────────────────────────────────────────────────────────────┘
                            │
              ┌─────────────┴─────────────┐
              ▼                           ▼
┌─────────────────────────┐   ┌─────────────────────────┐
│   Trend 策略风控         │   │   Pin 策略风控       │
├─────────────────────────┤   ├─────────────────────────┤
│  TrendRiskLimitGuard    │   │  PinRiskLeverageGuard  │
│  (品种限额)            │   │  (动态杠杆)            │
│  ⚠️Pin禁用            │   │  ⚠️Trend禁用        │
└─────────────────────────┘   └─────────────────────────┘
```

| 风控组件 | 公共 | Trend专用 | Pin专用 | 说明 |
|---------|------|---------|---------|------|
| RiskPreChecker | ✅ | - | - | 品种注册、波动率模式、资金检查 |
| RiskReChecker | ✅ | - | - | 锁内复核、价格偏离 |
| AccountPool | ✅ | - | - | 资金管理、熔断 |
| ThresholdConstants | ✅ | - | - | 阈值常量 |
| MarginPoolConfig | ✅ | - | - | 保证金配置 |
| TrendRiskLimitGuard | - | ✅ | ❌ | 趋势策略专用 |
| RateLimiter | ✅ | ✅ | ✅ | 所有策略通用 |
| PinRiskLeverageGuard | - | ❌ | ✅ | 插针高波动专用 |
| MarketStatusDetector | ✅ | - | - | 市场状态检测 |
| OrderCheck | ✅ | - | - | 订单预占 |

### 1.3 现状问题

| 问题 | 严重度 | 说明 |
|------|--------|------|
| 预占与冻结系统分离 | P0 | OrderCheck 和 AccountPool 各自管理冻结资金，可能不一致 |
| total_used_margin 从不更新 | P0 | AccountPool.get_account_margin 读取但从未写入 |
| 缺少品种限额 | P1 | 趋势策略无法限制单品种最大持仓 |
| 缺少频率限制 | P1 | 无法防止过度交易 |
| Vec/HashMap 性能 | P1 | 交易记录查询效率低 |
| 缺少动态杠杆 | P2 | 插针高波动策略高波动时无法自动降杠杆 |

---

## 2. 架构设计

### 2.1 模块目录结构

```
e_risk_monitor/src/
├── risk/                        # 风控层
│   ├── common/                  # 公共风控 (所有策略共享)
│   │   ├── mod.rs
│   │   ├── risk.rs             # RiskPreChecker
│   │   ├── risk_rechecker.rs   # RiskReChecker
│   │   ├── order_check.rs       # OrderCheck
│   │   └── thresholds.rs        # ThresholdConstants
│   │
│   ├── pin/                    # 插针高波动策略风控
│   │   ├── mod.rs
│   │   └── pin_risk_limit.rs    # 插针风控限额 (动态杠杆)
│   │
│   └── trend/                  # 趋势策略风控
│       ├── mod.rs
│       └── trend_risk_limit.rs  # 趋势风控限额 (品种限额)
│
├── position/                    # 现有
│   └── ...
│
└── persistence/                # 现有
    └── ...
```

### 2.2 risk 模块职责

```
┌─────────────────────────────────────────────────────────────────┐
│                        risk/ 风控层                              │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │  common/ (公共风控 - 所有策略共享)                      │   │
│  │  RiskPreChecker │ RiskReChecker │ OrderCheck           │   │
│  └─────────────────────────────────────────────────────────┘   │
│                              │                                 │
│         ┌────────────────────┴────────────────────┐            │
│         ▼                                         ▼            │
│  ┌───────────────────┐              ┌───────────────────┐     │
│  │  pin/             │              │  trend/           │     │
│  │  插针风控限额     │              │  趋势风控限额     │     │
│  │  ⚠️Trend禁用    │              │  ⚠️Pin禁用      │     │
│  └───────────────────┘              └───────────────────┘     │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘

====================================================================
策略类型与风控组件对照:

| 策略类型 | RiskPreChecker | RiskReChecker | OrderCheck | Pin风控 | Trend风控 |
|----------|---------------|---------------|------------|---------|-----------|
| Trend    | ✅             | ✅             | ✅          | ❌      | ✅        |
| Pin      | ✅             | ✅             | ✅          | ✅      | ❌        |
```

---

## 3. 详细设计

### 3.1 risk/common/ — 公共风控

#### 3.1.1 risk/common/risk.rs — RiskPreChecker

> **文件**: `risk/common/risk.rs`
> **策略类型**: 公共 (所有策略共享)

```rust
/// 风险预检器
///
/// 检查项目:
/// 1. 资金是否足够 (最低保留金额)
/// 2. 持仓比例是否超限
/// 3. 品种是否已注册
/// 4. 波动率模式是否允许交易
#[derive(Debug, Clone)]
pub struct RiskPreChecker {
    max_position_ratio: Decimal,
    min_reserve_balance: Decimal,
    registered_symbols: HashSet<String>,
    volatility_mode: VolatilityMode,
}
```

#### 3.1.2 risk/common/risk_rechecker.rs — RiskReChecker

> **文件**: `risk/common/risk_rechecker.rs`
> **策略类型**: 公共

```rust
/// 风控复核器 - 锁内复核
///
/// 在获取全局锁之后再次核对，确保并发安全。
pub struct RiskReChecker {
    volatility_threshold: Decimal,
    price_deviation_threshold: Decimal,
}
```

#### 3.1.3 risk/common/order_check.rs — OrderCheck

> **文件**: `risk/common/order_check.rs`
> **策略类型**: 公共

```rust
/// 订单检查器
///
/// 支持:
/// - 订单预占 (冻结保证金)
/// - 持仓比例检查
/// - 名义价值检查
pub struct OrderCheck {
    max_position_ratio: RwLock<Decimal>,
    min_order_notional: RwLock<Decimal>,
    reservations: RwLock<FnvHashMap<String, OrderReservation>>,
    total_frozen: RwLock<Decimal>,
}
```

#### 3.1.4 risk/common/thresholds.rs — ThresholdConstants

> **文件**: `risk/common/thresholds.rs`
> **策略类型**: 公共

```rust
/// 阈值常量模块
///
/// 集中管理所有策略阈值常量，避免硬编码。
pub struct ThresholdConstants {
    // 盈亏相关
    pub profit_threshold: Decimal,        // 1%
    pub stop_loss_threshold: Decimal,     // 5%
    // ... 更多阈值
}
```

---

### 3.2 risk/pin/ — 插针高波动策略风控

#### 3.2.1 risk/pin/pin_risk_limit.rs — 插针风控限额

> **文件**: `risk/pin/pin_risk_limit.rs`
> **策略类型**: **Pin (插针高波动策略) 专用**
> **注意**: 趋势策略禁用此模块

```rust
/// 波动级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PinVolatilityLevel {
    /// 低波动
    Low,
    /// 正常
    Normal,
    /// 高波动
    High,
    /// 极端波动
    Extreme,
}

/// 插针动态杠杆配置
///
/// 使用数组而非 HashMap，提高性能。
#[derive(Debug, Clone)]
pub struct PinLeverageConfig {
    /// 各波动级别对应的杠杆倍数 [Low, Normal, High, Extreme]
    leverage_by_level: [Decimal; 4],
    // 阈值配置...
}

impl Default for PinLeverageConfig {
    fn default() -> Self {
        Self {
            leverage_by_level: [dec!(15), dec!(10), dec!(5), dec!(2)],
            // ...
        }
    }
}

/// 插针风控杠杆计算器 (Pin 策略专用)
///
/// **警告**: 趋势策略不应使用此模块！
///
/// 接口命名明确: PinRiskLeverageGuard / PinRiskLeverageCalculator
#[derive(Debug, Clone)]
pub struct PinRiskLeverageGuard {
    config: PinLeverageConfig,
}

impl PinRiskLeverageGuard {
    /// 创建插针风控杠杆守卫 (Pin 策略专用)
    pub fn new(config: PinLeverageConfig) -> Self {
        Self { config }
    }

    /// 计算当前应该使用的杠杆 (Pin 策略)
    ///
    /// # 参数
    /// - `current_volatility`: 当前波动率 (TR比率)
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
        let level_leverage = self.config.get_leverage(level);
        level_leverage.min(base_leverage)
    }

    /// 获取波动级别
    pub fn get_volatility_level(&self, volatility: Decimal) -> PinVolatilityLevel {
        // ...
    }

    /// 判断是否需要降杠杆
    pub fn should_reduce_leverage(&self, current_volatility: Decimal) -> bool {
        current_volatility >= self.config.high_volatility_threshold
    }
}
```

---

### 3.3 risk/trend/ — 趋势策略风控

#### 3.3.1 risk/trend/trend_risk_limit.rs — 趋势风控限额

> **文件**: `risk/trend/trend_risk_limit.rs`
> **策略类型**: **Trend (趋势策略) 专用**
> **注意**: Pin 策略禁用此模块

```rust
/// 单品种持仓限制 (趋势策略专用)
#[derive(Debug, Clone)]
pub struct TrendSymbolLimit {
    /// 单品种最大名义价值 (默认 5000 USDT)
    pub max_notional: Decimal,
    /// 单品种最大数量
    pub max_qty: Decimal,
}

/// 全局持仓限制 (趋势策略专用)
#[derive(Debug, Clone)]
pub struct TrendGlobalLimit {
    /// 全局最大名义价值
    pub max_total_notional: Decimal,
    /// 最大交易品种数
    pub max_symbol_count: u32,
}

/// 趋势风控限额守卫 (Trend 策略专用)
///
/// **警告**: Pin 策略不应使用此模块！
///
/// 接口命名明确: TrendRiskLimitGuard
#[derive(Debug, Clone)]
pub struct TrendRiskLimitGuard {
    /// 单品种限制
    symbol_limit: TrendSymbolLimit,
    /// 全局限制
    global_limit: TrendGlobalLimit,
    /// 当前持仓名义价值
    current_notionals: FnvHashMap<String, Decimal>,
    /// 当前持仓数量
    current_quantities: FnvHashMap<String, Decimal>,
}

impl TrendRiskLimitGuard {
    /// 创建趋势风控限额守卫 (Trend 策略专用)
    pub fn new(symbol_limit: TrendSymbolLimit, global_limit: TrendGlobalLimit) -> Self {
        Self {
            symbol_limit,
            global_limit,
            current_notionals: FnvHashMap::default(),
            current_quantities: FnvHashMap::default(),
        }
    }

    /// 预检订单是否允许 (Trend 策略专用)
    ///
    /// # 参数
    /// - `symbol`: 交易品种
    /// - `order_notional`: 订单名义价值
    /// - `order_qty`: 订单数量
    ///
    /// # 返回
    /// - Ok: 检查通过
    /// - Err: 超过限额
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
        if self.symbol_limit.max_notional > dec!(0) && new_notional > self.symbol_limit.max_notional {
            return Err(EngineError::PositionLimitExceeded(format!(
                "Trend {} 名义价值 {} 超过限额 {}",
                symbol, new_notional, self.symbol_limit.max_notional
            )));
        }

        // 2. 检查全局限额
        let total_notional: Decimal = self.current_notionals.values().sum();
        let new_total_notional = total_notional + order_notional;

        if self.global_limit.max_total_notional > dec!(0) && new_total_notional > self.global_limit.max_total_notional {
            return Err(EngineError::PositionLimitExceeded(format!(
                "全局名义价值 {} 超过限额 {}",
                new_total_notional, self.global_limit.max_total_notional
            )));
        }

        // 检查全局品种数限额
        let current_symbols = self.current_notionals.len() as u32;
        if !self.current_notionals.contains_key(symbol) && current_symbols >= self.global_limit.max_symbol_count {
            return Err(EngineError::PositionLimitExceeded(format!(
                "品种数 {} 达上限 {}",
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
        // ...
    }
}
```

---

### 3.4 模块导出设计

#### 3.4.1 risk/pin/mod.rs

```rust
pub mod pin_risk_limit;

pub use pin_risk_limit::{PinVolatilityLevel, PinLeverageConfig, PinRiskLeverageGuard};
```

#### 3.4.2 risk/trend/mod.rs

```rust
pub mod trend_risk_limit;

pub use trend_risk_limit::{TrendSymbolLimit, TrendGlobalLimit, TrendRiskLimitGuard};
```

#### 3.4.3 risk/common/mod.rs

```rust
pub mod risk;
pub mod risk_rechecker;
pub mod order_check;
pub mod thresholds;

pub use risk::{RiskPreChecker, VolatilityMode};
pub use risk_rechecker::RiskReChecker;
pub use order_check::{OrderCheck, OrderCheckResult, OrderReservation};
pub use thresholds::ThresholdConstants;
```

---

### 3.5 接口使用示例

```rust
use e_risk_monitor::risk::{TrendRiskLimitGuard, PinRiskLeverageGuard};

// ========== Trend 策略初始化 ==========
fn init_trend_guard() -> TrendRiskLimitGuard {
    TrendRiskLimitGuard::new(
        TrendSymbolLimit { max_notional: dec!(5000), max_qty: dec!(0) },
        TrendGlobalLimit { max_total_notional: dec!(50000), max_symbol_count: 10 },
    )
}

// ========== Pin 策略初始化 ==========
fn init_pin_guard() -> PinRiskLeverageGuard {
    PinRiskLeverageGuard::new(PinLeverageConfig::default())
}

// ========== 调用示例 ==========

// Trend 策略 - 使用 TrendRiskLimitGuard
let trend_guard = init_trend_guard();
trend_guard.pre_check("BTC", dec!(3000), dec!(0))?;

// Pin 策略 - 使用 PinRiskLeverageGuard
let pin_guard = init_pin_guard();
let leverage = pin_guard.calculate_leverage(dec!(0.07), dec!(10)); // 波动7%时降杠杆
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
use std::sync::Arc;

/// 资金守护
///
/// 统一的资金管理层，确保预占、冻结、扣除的一致性。
/// 替代原有的 OrderCheck 预占系统。
///
/// 线程安全：所有操作使用原子锁保护。
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
    /// 创建资金守护
    ///
    /// # 参数
    /// - `account_pool`: 账户池 Arc 引用
    pub fn new(account_pool: Arc<AccountPool>) -> Self {
        Self {
            account_pool,
            total_reserved: RwLock::new(dec!(0)),
            reservations: RwLock::new(FnvHashMap::default()),
        }
    }

    /// 预检并预占资金
    ///
    /// 原子操作：在同一锁内完成资金检查、冻结、预占记录。
    pub fn pre_reserve(
        &self,
        order_id: &str,
        amount: Decimal,
    ) -> Result<Decimal, EngineError> {
        // 1. 资金充足性检查
        let available = self.account_pool.available();
        if available < amount {
            return Err(EngineError::InsufficientFund(format!(
                "可用资金 {} 不足预占 {}",
                available, amount
            )));
        }

        // 2. 冻结资金 (写锁)
        self.account_pool.freeze(amount)?;

        // 3. 原子写入预占记录和总额
        let mut reservations = self.reservations.write();
        let mut total = self.total_reserved.write();

        // 双重检查是否已存在预占
        if reservations.contains_key(order_id) {
            drop(reservations);
            drop(total);
            // 回滚冻结
            self.account_pool.unfreeze(amount);
            return Err(EngineError::RiskCheckFailed(format!(
                "订单 {} 已有预占记录",
                order_id
            )));
        }

        reservations.insert(order_id.to_string(), amount);
        *total += amount;

        Ok(amount)
    }

    /// 确认预占 (扣除保证金)
    pub fn confirm(&self, order_id: &str) -> Result<(), EngineError> {
        let amount = {
            let mut reservations = self.reservations.write();
            let mut total = self.total_reserved.write();

            let amount = reservations.remove(order_id)
                .ok_or_else(|| EngineError::RiskCheckFailed(format!(
                    "订单 {} 没有预占记录",
                    order_id
                )))?;

            *total -= amount;
            amount
        };

        // 从冻结转为已用
        self.account_pool.deduct_margin(amount)?;
        Ok(())
    }

    /// 取消预占 (释放冻结)
    pub fn cancel(&self, order_id: &str) -> Result<Decimal, EngineError> {
        let amount = {
            let mut reservations = self.reservations.write();
            let mut total = self.total_reserved.write();

            let amount = reservations.remove(order_id)
                .ok_or_else(|| EngineError::RiskCheckFailed(format!(
                    "订单 {} 没有预占记录",
                    order_id
                )))?;

            *total -= amount;
            amount
        };

        // 释放冻结资金
        self.account_pool.unfreeze(amount);
        Ok(amount)
    }

    /// 获取总预占金额
    pub fn total_reserved(&self) -> Decimal {
        *self.total_reserved.read()
    }

    /// 检查预占是否存在
    pub fn has_reservation(&self, order_id: &str) -> bool {
        self.reservations.read().contains_key(order_id)
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

pub mod risk;
pub mod position;
pub mod persistence;
pub mod shared;

// ========== risk 模块导出 ==========
pub mod risk;

// risk/common/ 公共风控
pub use risk::common::{
    RiskPreChecker, RiskReChecker, OrderCheck, OrderCheckResult, OrderReservation,
    ThresholdConstants,
};

// risk/pin/ 插针高波动策略风控 (Pin 专用)
pub use risk::pin::{
    PinRiskLeverageGuard, PinLeverageConfig, PinVolatilityLevel,
};

// risk/trend/ 趋势策略风控 (Trend 专用)
pub use risk::trend::{
    TrendRiskLimitGuard, TrendSymbolLimit, TrendGlobalLimit,
};

// ========== position 模块导出 ==========
pub use position::{
    Direction, LocalPosition, LocalPositionManager,
    PositionStats, PositionDirection, PositionExclusionChecker, PositionInfo,
};

// ========== persistence 模块导出 ==========
pub use persistence::{
    PersistenceService, KLineCache, KLineData, TradeRecord,
    PositionSnapshot, PersistenceConfig, PersistenceStats,
};

// ========== shared 模块导出 ==========
pub use shared::{
    AccountInfo, AccountMargin, AccountPool, CircuitBreakerState,
    GlobalMarginConfig, MarginPoolConfig, MinuteOpenConfig, StrategyLevel,
    MarketStatus, MarketStatusDetector, PinIntensity, PinDetection,
    PnlManager, RoundGuard, RoundGuardScope,
    MemoryBackup, memory_backup_dir,
};
```

---

## 8. 测试设计

### 8.1 TrendRiskLimitGuard 测试

> **注意**: Trend 策略使用 `TrendRiskLimitGuard`，Pin 策略使用 `PinRiskLeverageGuard`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trend_symbol_limit_basic() {
        let guard = TrendRiskLimitGuard::new(
            TrendSymbolLimit { max_notional: dec!(5000), max_qty: dec!(0) },
            TrendGlobalLimit { max_total_notional: dec!(50000), max_symbol_count: 10 },
        );

        // 首笔订单 3000，通过 (趋势策略)
        assert!(guard.pre_check("BTC", dec!(3000), dec!(0)).is_ok());

        // 第二笔订单 3000，总额 6000 > 5000，拒绝 (趋势策略)
        assert!(guard.pre_check("BTC", dec!(3000), dec!(0)).is_err());
    }

    #[test]
    fn test_trend_global_symbol_count_limit() {
        let guard = TrendRiskLimitGuard::new(
            TrendSymbolLimit::default(),
            TrendGlobalLimit { max_total_notional: dec!(50000), max_symbol_count: 2 },
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

### 8.2 PinRiskLeverageGuard 测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pin_calculate_leverage_high_volatility() {
        let guard = PinRiskLeverageGuard::new(PinLeverageConfig::default());

        // 高波动 7% > 5% 阈值，应该降杠杆
        let leverage = guard.calculate_leverage(dec!(0.07), dec!(10));
        assert_eq!(leverage, dec!(5)); // 5x (高波动级别)
    }

    #[test]
    fn test_pin_calculate_leverage_normal() {
        let guard = PinRiskLeverageGuard::new(PinLeverageConfig::default());

        // 正常波动 2% < 3% 阈值，使用基准杠杆 10x
        let leverage = guard.calculate_leverage(dec!(0.02), dec!(10));
        assert_eq!(leverage, dec!(10)); // 10x (正常级别)
    }

    #[test]
    fn test_pin_should_reduce_leverage() {
        let guard = PinRiskLeverageGuard::new(PinLeverageConfig::default());

        // 高波动应该降杠杆
        assert!(guard.should_reduce_leverage(dec!(0.07)));

        // 正常波动不需要降杠杆
        assert!(!guard.should_reduce_leverage(dec!(0.02)));
    }
}
```

---

## 9. 实现计划

| 步骤 | 任务 | 优先级 | 适用策略 |
|------|------|--------|----------|
| 1 | 新增 risk/common/ 目录 | P0 | - |
| 2 | 新增 risk/pin/ 目录 | P0 | - |
| 3 | 新增 risk/trend/ 目录 | P0 | - |
| 4 | 实现 RiskPreChecker | P0 | 全部 |
| 5 | 实现 RiskReChecker | P0 | 全部 |
| 6 | 实现 OrderCheck | P0 | 全部 |
| 7 | 实现 TrendRiskLimitGuard | P1 | 仅 Trend |
| 8 | 实现 PinRiskLeverageGuard | P2 | 仅 Pin |
| 9 | 修复 total_used_margin 更新问题 | P0 | 全部 |
| 10 | OrderCheck 改用 FnvHashMap | P1 | 全部 |
| 11 | PersistenceService 添加索引 | P1 | 全部 |
| 12 | 更新 lib.rs 导出 | P0 | - |
| 13 | 编写单元测试 | P1 | - |
| 14 | 更新文档 | P2 | - |

### 9.1 策略类型使用指南

```rust
use e_risk_monitor::{
    TrendRiskLimitGuard, TrendSymbolLimit, TrendGlobalLimit,
    PinRiskLeverageGuard, PinLeverageConfig,
    RiskPreChecker, RiskReChecker, OrderCheck,
};

// ========== Trend 策略风控初始化 ==========
fn init_trend_risk() -> TrendRiskLimitGuard {
    TrendRiskLimitGuard::new(
        TrendSymbolLimit { max_notional: dec!(5000), max_qty: dec!(0) },
        TrendGlobalLimit { max_total_notional: dec!(50000), max_symbol_count: 10 },
    )
}

// ========== Pin 策略风控初始化 ==========
fn init_pin_risk() -> PinRiskLeverageGuard {
    PinRiskLeverageGuard::new(PinLeverageConfig::default())
}

// ========== 调用示例 ==========

// Trend 策略 - 使用 TrendRiskLimitGuard
let trend_guard = init_trend_risk();
trend_guard.pre_check("BTC", dec!(3000), dec!(0))?;
trend_guard.update_position("BTC", dec!(3000), dec!(0));

// Pin 策略 - 使用 PinRiskLeverageGuard
let pin_guard = init_pin_risk();
let leverage = pin_guard.calculate_leverage(dec!(0.07), dec!(10)); // 波动7%时降杠杆
if pin_guard.should_reduce_leverage(dec!(0.07)) {
    // 需要降杠杆
}
```

---

## 10. 向后兼容性

- 新增的 `risk/common/`、`risk/pin/`、`risk/trend/` 模块不影响现有 API
- `RiskPreChecker`、`RiskReChecker`、`OrderCheck` 保持不变
- `TrendRiskLimitGuard`、`PinRiskLeverageGuard` 均为新增，可选择性使用
