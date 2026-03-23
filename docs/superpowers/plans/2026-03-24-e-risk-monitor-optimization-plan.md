# e_risk_monitor 全面优化实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 优化 e_risk_monitor 架构，实现 Pin/Trend 策略分离的风控模块，新增动态杠杆和品种限额功能，完善盈亏拯救机制

**Architecture:** 将 risk/ 目录从扁平结构重组为 `common/pin/trend` 三层目录，策略类型通过 `StrategyLevel` (Minute→Pin, Hour→Trend) 映射

**Tech Stack:** Rust, parking_lot::RwLock, rust_decimal, FnvHashMap

---

## 文件结构

### 重组前 (当前)
```
e_risk_monitor/src/risk/
├── mod.rs (扁平导出)
├── risk.rs (RiskPreChecker)
├── risk_rechecker.rs
├── order_check.rs
├── thresholds.rs
└── minute_risk.rs
```

### 重组后 (目标)
```
e_risk_monitor/src/risk/
├── common/                  # 公共风控 (从 flat 迁移)
│   ├── mod.rs
│   ├── risk.rs            # RiskPreChecker
│   ├── risk_rechecker.rs
│   ├── order_check.rs
│   └── thresholds.rs
├── pin/                    # Pin 策略风控 (新增)
│   ├── mod.rs
│   └── pin_risk_limit.rs  # PinRiskLeverageGuard
├── trend/                  # Trend 策略风控 (新增)
│   ├── mod.rs
│   └── trend_risk_limit.rs # TrendRiskLimitGuard
└── mod.rs (更新导出)
```

### 需要修改的文件

| 文件 | 操作 | 说明 |
|------|------|------|
| `risk/mod.rs` | 重写 | 目录结构重组 |
| `risk/common/` | 新建目录 | 迁移现有公共风控 |
| `risk/pin/` | 新建目录 | Pin 策略风控 |
| `risk/trend/` | 新建目录 | Trend 策略风控 |
| `shared/account_pool.rs` | 修改 | 修复 total_used_margin |
| `shared/pnl_manager.rs` | 修改 | 完善拯救机制 |
| `lib.rs` | 修改 | 更新导出 |

---

## 任务清单

### Task 1: 创建 risk/common/ 目录结构

**Files:**
- Create: `crates/e_risk_monitor/src/risk/common/mod.rs`
- Modify: `crates/e_risk_monitor/src/risk/mod.rs` (更新导入)

- [ ] **Step 1: 创建 common/mod.rs**

```rust
#![forbid(unsafe_code)]

pub mod risk;
pub mod risk_rechecker;
pub mod order_check;
pub mod thresholds;

pub use risk::{RiskPreChecker, VolatilityMode};
pub use risk_rechecker::RiskReChecker;
pub use order_check::{OrderCheck, OrderCheckResult, OrderReservation};
pub use thresholds::ThresholdConstants;
```

- [ ] **Step 2: 更新 risk/mod.rs**

```rust
#![forbid(unsafe_code)]

pub mod common;
pub mod pin;
pub mod trend;
pub mod minute_risk; // 保留分钟级计算函数

// Re-exports from common
pub use common::{
    RiskPreChecker, RiskReChecker, OrderCheck, OrderCheckResult, OrderReservation,
    ThresholdConstants, VolatilityMode,
};

// Re-exports from pin
pub use pin::{PinRiskLeverageGuard, PinLeverageConfig, PinVolatilityLevel};

// Re-exports from trend
pub use trend::{TrendRiskLimitGuard, TrendSymbolLimit, TrendGlobalLimit};

// Re-exports from minute_risk
pub use minute_risk::{
    calculate_hour_open_notional, calculate_minute_open_notional,
    calculate_open_qty_from_notional, MinuteOpenResult,
};
```

- [ ] **Step 3: 提交**

```bash
git add crates/e_risk_monitor/src/risk/common/ crates/e_risk_monitor/src/risk/mod.rs
git commit -m "refactor(e_risk): 创建 risk/common/ 目录结构"
```

---

### Task 2: 创建 risk/pin/ 目录和 PinRiskLeverageGuard

**Files:**
- Create: `crates/e_risk_monitor/src/risk/pin/mod.rs`
- Create: `crates/e_risk_monitor/src/risk/pin/pin_risk_limit.rs`

- [ ] **Step 1: 创建 pin/mod.rs**

```rust
#![forbid(unsafe_code)]

pub mod pin_risk_limit;

pub use pin_risk_limit::{PinRiskLeverageGuard, PinLeverageConfig, PinVolatilityLevel};
```

- [ ] **Step 2: 创建 pin/pin_risk_limit.rs**

```rust
//! Pin 策略动态杠杆模块
//!
//! **警告**: Trend 策略禁用此模块！

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

/// 波动级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PinVolatilityLevel {
    Low,
    Normal,
    High,
    Extreme,
}

/// 插针动态杠杆配置
///
/// 使用数组而非 HashMap，提高性能。
#[derive(Debug, Clone)]
pub struct PinLeverageConfig {
    /// 各波动级别对应的杠杆倍数 [Low, Normal, High, Extreme]
    leverage_by_level: [Decimal; 4],
    /// 高波动阈值 (超过此值认为是高波动)
    high_volatility_threshold: Decimal,
}

impl Default for PinLeverageConfig {
    fn default() -> Self {
        Self {
            // [Low=15x, Normal=10x, High=5x, Extreme=2x]
            leverage_by_level: [dec!(15), dec!(10), dec!(5), dec!(2)],
            high_volatility_threshold: dec!(0.03), // 3%
        }
    }
}

impl PinLeverageConfig {
    /// 获取指定波动级别的杠杆
    pub fn get_leverage(&self, level: PinVolatilityLevel) -> Decimal {
        match level {
            PinVolatilityLevel::Low => self.leverage_by_level[0],
            PinVolatilityLevel::Normal => self.leverage_by_level[1],
            PinVolatilityLevel::High => self.leverage_by_level[2],
            PinVolatilityLevel::Extreme => self.leverage_by_level[3],
        }
    }
}

/// Pin策略杠杆守卫 (Pin 专用)
///
/// **警告**: Trend 策略不应使用此模块！
#[derive(Debug, Clone)]
pub struct PinRiskLeverageGuard {
    config: PinLeverageConfig,
}

impl PinRiskLeverageGuard {
    /// 创建 Pin 杠杆守卫
    pub fn new(config: PinLeverageConfig) -> Self {
        Self { config }
    }

    /// 获取波动级别
    pub fn get_volatility_level(&self, volatility: Decimal) -> PinVolatilityLevel {
        if volatility >= self.config.high_volatility_threshold * dec!(2) {
            PinVolatilityLevel::Extreme
        } else if volatility >= self.config.high_volatility_threshold * dec!(1.5) {
            PinVolatilityLevel::High
        } else if volatility >= self.config.high_volatility_threshold {
            PinVolatilityLevel::Normal
        } else {
            PinVolatilityLevel::Low
        }
    }

    /// 计算当前应该使用的杠杆
    ///
    /// 返回 min(级别杠杆, 基础杠杆)
    pub fn calculate_leverage(
        &self,
        current_volatility: Decimal,
        base_leverage: Decimal,
    ) -> Decimal {
        let level = self.get_volatility_level(current_volatility);
        let level_leverage = self.config.get_leverage(level);
        level_leverage.min(base_leverage)
    }

    /// 是否应该降杠杆
    pub fn should_reduce_leverage(&self, current_volatility: Decimal) -> bool {
        current_volatility >= self.config.high_volatility_threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_volatility_level() {
        let guard = PinRiskLeverageGuard::new(PinLeverageConfig::default());

        // Low: < 3%
        assert_eq!(guard.get_volatility_level(dec!(0.01)), PinVolatilityLevel::Low);
        assert_eq!(guard.get_volatility_level(dec!(0.02)), PinVolatilityLevel::Low);

        // Normal: >= 3%
        assert_eq!(guard.get_volatility_level(dec!(0.03)), PinVolatilityLevel::Normal);
        assert_eq!(guard.get_volatility_level(dec!(0.04)), PinVolatilityLevel::Normal);

        // High: >= 4.5% (3% * 1.5)
        assert_eq!(guard.get_volatility_level(dec!(0.05)), PinVolatilityLevel::High);

        // Extreme: >= 6% (3% * 2)
        assert_eq!(guard.get_volatility_level(dec!(0.07)), PinVolatilityLevel::Extreme);
    }

    #[test]
    fn test_calculate_leverage() {
        let guard = PinRiskLeverageGuard::new(PinLeverageConfig::default());

        // 低波动: 使用 15x (vs base 10x = 10x)
        assert_eq!(guard.calculate_leverage(dec!(0.01), dec!(10)), dec!(10));

        // 高波动: 使用 5x (vs base 10x = 5x)
        assert_eq!(guard.calculate_leverage(dec!(0.05), dec!(10)), dec!(5));

        // 基础杠杆 3x，高波动 5x，取小值 = 3x
        assert_eq!(guard.calculate_leverage(dec!(0.05), dec!(3)), dec!(3));
    }

    #[test]
    fn test_should_reduce_leverage() {
        let guard = PinRiskLeverageGuard::new(PinLeverageConfig::default());

        assert!(!guard.should_reduce_leverage(dec!(0.01)));
        assert!(guard.should_reduce_leverage(dec!(0.03)));
    }
}
```

- [ ] **Step 3: 提交**

```bash
git add crates/e_risk_monitor/src/risk/pin/
git commit -m "feat(e_risk): 添加 PinRiskLeverageGuard 动态杠杆模块"
```

---

### Task 3: 创建 risk/trend/ 目录和 TrendRiskLimitGuard

**Files:**
- Create: `crates/e_risk_monitor/src/risk/trend/mod.rs`
- Create: `crates/e_risk_monitor/src/risk/trend/trend_risk_limit.rs`

- [ ] **Step 1: 创建 trend/mod.rs**

```rust
#![forbid(unsafe_code)]

pub mod trend_risk_limit;

pub use trend_risk_limit::{TrendRiskLimitGuard, TrendSymbolLimit, TrendGlobalLimit};
```

- [ ] **Step 2: 创建 trend/trend_risk_limit.rs**

```rust
//! Trend 策略品种限额模块
//!
//! **警告**: Pin 策略禁用此模块！

use fnv::FnvHashMap;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

/// 单品种持仓限制 (Trend 专用)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendSymbolLimit {
    /// 最大名义价值 (0 = 不限制)
    pub max_notional: Decimal,
    /// 最大数量 (0 = 不限制)
    pub max_qty: Decimal,
}

impl Default for TrendSymbolLimit {
    fn default() -> Self {
        Self {
            max_notional: dec!(5000), // 默认 5000 USDT
            max_qty: dec!(0),
        }
    }
}

/// 全局持仓限制 (Trend 专用)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendGlobalLimit {
    /// 全局最大名义价值 (0 = 不限制)
    pub max_total_notional: Decimal,
    /// 最大品种数 (0 = 不限制)
    pub max_symbol_count: u32,
}

impl Default for TrendGlobalLimit {
    fn default() -> Self {
        Self {
            max_total_notional: dec!(50000), // 默认 50000 USDT
            max_symbol_count: 10,
        }
    }
}

/// Trend策略限额守卫 (Trend 专用)
///
/// **警告**: Pin 策略不应使用此模块！
#[derive(Debug, Clone)]
pub struct TrendRiskLimitGuard {
    /// 单品种限制
    symbol_limit: TrendSymbolLimit,
    /// 全局限制
    global_limit: TrendGlobalLimit,
    /// 当前各品种名义价值 (FnvHashMap 优化)
    current_notionals: FnvHashMap<String, Decimal>,
    /// 当前各品种数量
    current_quantities: FnvHashMap<String, Decimal>,
}

impl TrendRiskLimitGuard {
    /// 创建 Trend 限额守卫
    pub fn new(symbol_limit: TrendSymbolLimit, global_limit: TrendGlobalLimit) -> Self {
        Self {
            symbol_limit,
            global_limit,
            current_notionals: FnvHashMap::default(),
            current_quantities: FnvHashMap::default(),
        }
    }

    /// 预检订单
    pub fn pre_check(
        &self,
        symbol: &str,
        order_notional: Decimal,
        _order_qty: Decimal,
    ) -> Result<(), String> {
        // 1. 检查单品种限额
        if self.symbol_limit.max_notional > dec!(0) {
            let current_notional = self.current_notionals.get(symbol).copied().unwrap_or(dec!(0));
            let new_notional = current_notional + order_notional;
            if new_notional > self.symbol_limit.max_notional {
                return Err(format!(
                    "Trend {} 名义价值 {} 超过单品种限额 {}",
                    symbol, new_notional, self.symbol_limit.max_notional
                ));
            }
        }

        // 2. 检查全局限额
        if self.global_limit.max_total_notional > dec!(0) {
            let total_notional: Decimal = self.current_notionals.values().sum();
            let new_total_notional = total_notional + order_notional;
            if new_total_notional > self.global_limit.max_total_notional {
                return Err(format!(
                    "全局名义价值 {} 超过限额 {}",
                    new_total_notional, self.global_limit.max_total_notional
                ));
            }
        }

        // 3. 检查品种数限额
        if self.global_limit.max_symbol_count > 0 {
            let current_symbols = self.current_notionals.len() as u32;
            if !self.current_notionals.contains_key(symbol) && current_symbols >= self.global_limit.max_symbol_count {
                return Err(format!(
                    "品种数 {} 达上限 {}",
                    current_symbols, self.global_limit.max_symbol_count
                ));
            }
        }

        Ok(())
    }

    /// 更新持仓
    pub fn update_position(&mut self, symbol: &str, notional: Decimal, qty: Decimal) {
        self.current_notionals.insert(symbol.to_string(), notional);
        self.current_quantities.insert(symbol.to_string(), qty);
    }

    /// 减少持仓
    pub fn reduce_position(&mut self, symbol: &str, notional: Decimal, qty: Decimal) {
        if let Some(current_notional) = self.current_notionals.get(symbol) {
            let new_notional = (*current_notional - notional).max(dec!(0));
            if new_notional <= dec!(0) {
                self.current_notionals.remove(symbol);
                self.current_quantities.remove(symbol);
            } else {
                self.current_notionals.insert(symbol.to_string(), new_notional);
                if let Some(current_qty) = self.current_quantities.get(symbol) {
                    let new_qty = (*current_qty - qty).max(dec!(0));
                    self.current_quantities.insert(symbol.to_string(), new_qty);
                }
            }
        }
    }

    /// 获取当前品种数
    pub fn symbol_count(&self) -> usize {
        self.current_notionals.len()
    }

    /// 清空所有持仓
    pub fn clear(&mut self) {
        self.current_notionals.clear();
        self.current_quantities.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pre_check_pass() {
        let guard = TrendRiskLimitGuard::new(
            TrendSymbolLimit::default(),
            TrendGlobalLimit::default(),
        );

        assert!(guard.pre_check("BTC", dec!(1000), dec!(0)).is_ok());
    }

    #[test]
    fn test_pre_check_single_symbol_limit() {
        let mut guard = TrendRiskLimitGuard::new(
            TrendSymbolLimit {
                max_notional: dec!(5000),
                max_qty: dec!(0),
            },
            TrendGlobalLimit::default(),
        );

        // 首次下单 3000，通过
        assert!(guard.pre_check("BTC", dec!(3000), dec!(0)).is_ok());
        guard.update_position("BTC", dec!(3000), dec!(0));

        // 再次下单 3000，总共 6000 > 5000，拒绝
        assert!(guard.pre_check("BTC", dec!(3000), dec!(0)).is_err());
    }

    #[test]
    fn test_pre_check_symbol_count_limit() {
        let guard = TrendRiskLimitGuard::new(
            TrendSymbolLimit::default(),
            TrendGlobalLimit {
                max_total_notional: dec!(0),
                max_symbol_count: 2,
            },
        );

        // 品种1 通过
        assert!(guard.pre_check("BTC", dec!(1000), dec!(0)).is_ok());

        // 品种2 通过
        assert!(guard.pre_check("ETH", dec!(1000), dec!(0)).is_ok());

        // 品种3 超过上限，拒绝
        assert!(guard.pre_check("SOL", dec!(1000), dec!(0)).is_err());
    }

    #[test]
    fn test_update_and_reduce_position() {
        let mut guard = TrendRiskLimitGuard::new(
            TrendSymbolLimit::default(),
            TrendGlobalLimit::default(),
        );

        guard.update_position("BTC", dec!(5000), dec!(0));
        assert_eq!(guard.symbol_count(), 1);

        guard.reduce_position("BTC", dec!(3000), dec!(0));
        assert_eq!(guard.current_notionals.get("BTC"), Some(&dec!(2000)));

        guard.reduce_position("BTC", dec!(2000), dec!(0));
        assert!(!guard.current_notionals.contains_key("BTC"));
    }
}
```

- [ ] **Step 3: 提交**

```bash
git add crates/e_risk_monitor/src/risk/trend/
git commit -m "feat(e_risk): 添加 TrendRiskLimitGuard 品种限额模块"
```

---

### Task 4: 修复 total_used_margin 更新问题

**Files:**
- Modify: `crates/e_risk_monitor/src/shared/account_pool.rs:239-248`

- [ ] **Step 1: 修复 deduct_margin**

当前代码 (行 239-248):
```rust
pub fn deduct_margin(&self, amount: Decimal) -> Result<(), String> {
    let mut account = self.account.write();
    if amount > account.frozen {
        return Err("冻结资金不足".to_string());
    }
    account.frozen -= amount;
    account.margin_used += amount;
    Ok(())
}
```

修复后:
```rust
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
```

- [ ] **Step 2: 修复 release_margin**

当前代码 (行 251-256):
```rust
pub fn release_margin(&self, amount: Decimal) {
    let mut account = self.account.write();
    let to_release = amount.min(account.margin_used);
    account.margin_used -= to_release;
    account.available += to_release;
}
```

修复后:
```rust
pub fn release_margin(&self, amount: Decimal) {
    let mut account = self.account.write();
    let to_release = amount.min(account.margin_used);
    account.margin_used -= to_release;
    account.available += to_release;
    drop(account);
    *self.total_used_margin.write() -= to_release;
}
```

- [ ] **Step 3: 添加测试验证**

```rust
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
    pool.release_margin(dec!(1000)).unwrap();
    assert_eq!(pool.margin_used(), dec!(0));
}
```

- [ ] **Step 4: 提交**

```bash
git add crates/e_risk_monitor/src/shared/account_pool.rs
git commit -m "fix(e_risk): 同步更新 total_used_margin"
```

---

### Task 5: 完善 PnlManager 拯救机制

**Files:**
- Modify: `crates/e_risk_monitor/src/shared/pnl_manager.rs`

- [ ] **Step 1: 添加拯救机制相关结构体**

在 `PnlManager` 之前添加:

```rust
/// 盈亏覆盖检查结果
#[derive(Debug, Clone)]
pub struct PnlCoverageResult {
    /// 是否可以覆盖
    pub can_cover: bool,
    /// 总浮亏
    pub total_loss: Decimal,
    /// 当前盈利
    pub current_profit: Decimal,
    /// 累计盈利
    pub accumulated_profit: Decimal,
    /// 净盈利
    pub net_profit: Decimal,
    /// 低波动品种数
    pub symbol_count: u32,
}

/// 解救结果
#[derive(Debug, Clone)]
pub struct RescueResult {
    /// 是否成功
    pub success: bool,
    /// 是否可以解救
    pub can_rescue: bool,
    /// 总浮亏
    pub total_loss: Decimal,
    /// 可用盈利总额
    pub total_available_profit: Decimal,
    /// 被解救的品种列表
    pub rescued_symbols: Vec<String>,
    /// 剩余盈利
    pub remaining_profit: Decimal,
}
```

- [ ] **Step 2: 添加拯救机制方法**

在 `impl PnlManager` 末尾添加:

```rust
/// 计算低波动品种总浮亏
pub fn calculate_low_volatility_total_loss(&self) -> Decimal {
    let low_vol = self.low_volatility_symbols.read();
    low_vol
        .iter()
        .map(|s| self.get_unrealized_pnl(s).max(dec!(0)))
        .sum()
}

/// 检查盈亏覆盖
pub fn check_pnl_coverage(
    &self,
    high_vol_profit: Decimal,
    is_realized: bool,
) -> PnlCoverageResult {
    let accumulated = self.get_cumulative_profit();
    let total_available = if is_realized {
        high_vol_profit + accumulated
    } else {
        accumulated
    };

    let total_loss = self.calculate_low_volatility_total_loss();
    let can_cover = total_available >= total_loss;
    let net_profit = if can_cover { total_available - total_loss } else { dec!(0) };

    PnlCoverageResult {
        can_cover,
        total_loss,
        current_profit: high_vol_profit,
        accumulated_profit: accumulated,
        net_profit,
        symbol_count: self.low_volatility_symbols.read().len() as u32,
    }
}

/// 解救低波动品种
pub fn rescue_low_volatility(&mut self, high_vol_profit: Decimal) -> RescueResult {
    let coverage = self.check_pnl_coverage(high_vol_profit, true);

    if !coverage.can_cover {
        return RescueResult {
            success: false,
            can_rescue: false,
            total_loss: dec!(0),
            total_available_profit: dec!(0),
            rescued_symbols: vec![],
            remaining_profit: dec!(0),
        };
    }

    let rescued_symbols: Vec<String> = self.low_volatility_symbols.read().iter().cloned().collect();

    // 清空低波动品种
    {
        let mut low_vol = self.low_volatility_symbols.write();
        low_vol.clear();
    }
    {
        let mut unrealized = self.unrealized_pnl.write();
        for sym in &rescued_symbols {
            unrealized.remove(sym);
        }
    }

    // 更新累计盈利
    *self.cumulative_profit.write() = coverage.net_profit;

    RescueResult {
        success: true,
        can_rescue: true,
        total_loss: coverage.total_loss,
        total_available_profit: coverage.current_profit + coverage.accumulated_profit,
        rescued_symbols,
        remaining_profit: coverage.net_profit,
    }
}

/// 获取低波动品种数
pub fn get_low_vol_count(&self) -> u32 {
    self.low_volatility_symbols.read().len() as u32
}
```

- [ ] **Step 3: 添加测试**

```rust
#[test]
fn test_rescue_mechanism() {
    let mut manager = PnlManager::new();

    // 添加低波动品种和浮亏
    manager.add_low_volatility_symbol("BTC".to_string());
    manager.update_unrealized_pnl("BTC", dec!(-500)); // 浮亏 500

    // 无盈利，无法解救
    let result = manager.rescue_low_volatility(dec!(0));
    assert!(!result.can_rescue);

    // 有盈利但不足
    manager.update_cumulative_profit(dec!(300)); // 只有 300
    let result = manager.rescue_low_volatility(dec!(0));
    assert!(!result.can_rescue);

    // 盈利足够，解救
    manager.update_cumulative_profit(dec!(300)); // 现在累计 600
    let result = manager.rescue_low_volatility(dec!(0));
    assert!(result.can_rescue);
    assert_eq!(result.rescued_symbols, vec!["BTC"]);
}
```

- [ ] **Step 4: 提交**

```bash
git add crates/e_risk_monitor/src/shared/pnl_manager.rs
git commit -m "feat(e_risk): 完善 PnlManager 拯救机制"
```

---

### Task 6: 更新 lib.rs 导出

**Files:**
- Modify: `crates/e_risk_monitor/src/lib.rs`

- [ ] **Step 1: 更新导出**

当前导出:
```rust
pub use risk::{risk::{RiskPreChecker, VolatilityMode}, risk_rechecker::RiskReChecker, order_check::{OrderCheck, OrderCheckResult, OrderReservation}, thresholds::ThresholdConstants, minute_risk::{calculate_hour_open_notional, calculate_minute_open_notional, calculate_open_qty_from_notional, MinuteOpenResult}};
```

更新为:
```rust
// Risk 模块导出 (common/pin/trend)
pub use risk::{
    // Common
    RiskPreChecker, RiskReChecker, OrderCheck, OrderCheckResult, OrderReservation,
    ThresholdConstants, VolatilityMode,
    // Pin
    PinRiskLeverageGuard, PinLeverageConfig, PinVolatilityLevel,
    // Trend
    TrendRiskLimitGuard, TrendSymbolLimit, TrendGlobalLimit,
    // Minute risk functions
    calculate_hour_open_notional, calculate_minute_open_notional,
    calculate_open_qty_from_notional, MinuteOpenResult,
};
```

同时更新 mod 声明，从 flat 改为目录结构。

- [ ] **Step 2: 提交**

```bash
git add crates/e_risk_monitor/src/lib.rs
git commit -m "refactor(e_risk): 更新 lib.rs 导出新的风险模块结构"
```

---

### Task 7: FnvHashMap 优化 (可选)

**Files:**
- Modify: `crates/e_risk_monitor/src/risk/order_check.rs`

- [ ] **Step 1: 将 HashMap 改为 FnvHashMap**

```rust
// 导入
use fnv::FnvHashMap;

// 结构体中
reservations: RwLock<FnvHashMap<String, OrderReservation>>,
```

- [ ] **Step 2: 提交**

```bash
git add crates/e_risk_monitor/src/risk/order_check.rs
git commit -m "perf(e_risk): OrderCheck 使用 FnvHashMap 优化"
```

---

## 验证步骤

所有任务完成后，执行以下验证:

```bash
# 编译检查
cargo check -p e_risk_monitor

# 运行测试
cargo test -p e_risk_monitor

# 检查导出
cargo doc -p e_risk_monitor --no-deps
```

---

## 依赖

- `fnv` crate (如未添加)
- 现有依赖: `rust_decimal`, `parking_lot`, `serde`, `chrono`
