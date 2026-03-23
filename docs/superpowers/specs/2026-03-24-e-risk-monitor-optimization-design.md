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
6. **浮亏拯救**：高波动盈利覆盖低波动浮亏，释放保证金

### 1.2 策略类型映射

> **重要说明**：代码中 `StrategyLevel` (Minute/Hour) 与业务层 `Pin/Trend` 的映射关系：

| StrategyLevel | 业务策略 | 说明 |
|---------------|----------|------|
| Minute | Pin (插针高波动) | 高杠杆插针，翻倍加仓 1-1-2-4-8 |
| Hour | Trend (趋势) | 日线趋势跟随，不加仓 |

### 1.3 风控类型划分

```
┌─────────────────────────────────────────────────────────────┐
│                      公共风控 (所有策略)                      │
├─────────────────────────────────────────────────────────────┤
│  RiskPreChecker  │  RiskReChecker  │  AccountPool          │
│  OrderCheck      │  MarketStatusDetector                   │
│  ThresholdConstants │ MarginPoolConfig                       │
│  RateLimiter (通用频率限制)                                  │
│  PnlManager (盈亏管理)                                       │
└─────────────────────────────────────────────────────────────┘
                            │
              ┌─────────────┴─────────────┐
              ▼                           ▼
┌─────────────────────────┐   ┌─────────────────────────┐
│   Pin 策略风控           │   │   Trend 策略风控          │
│   (Minute Level)        │   │   (Hour Level)          │
├─────────────────────────┤   ├─────────────────────────┤
│  PinRiskLeverageGuard   │   │  TrendRiskLimitGuard    │
│  (动态杠杆)              │   │  (品种限额)              │
│  ⚠️Trend禁用           │   │  ⚠️Pin禁用             │
└─────────────────────────┘   └─────────────────────────┘
```

| 风控组件 | 公共 | Pin专用 | Trend专用 | 说明 |
|---------|------|---------|---------|------|
| RiskPreChecker | ✅ | - | - | 品种注册、波动率模式、资金检查 |
| RiskReChecker | ✅ | - | - | 锁内复核、价格偏离 |
| AccountPool | ✅ | - | - | 资金管理、熔断 |
| ThresholdConstants | ✅ | - | - | 阈值常量 |
| MarginPoolConfig | ✅ | - | - | 保证金配置 |
| RateLimiter | ✅ | ✅ | ✅ | 所有策略通用 |
| PinRiskLeverageGuard | - | ✅ | ❌ | 动态杠杆 |
| TrendRiskLimitGuard | - | ❌ | ✅ | 品种限额 |
| MarketStatusDetector | ✅ | - | - | 市场状态检测 |
| OrderCheck | ✅ | - | - | 订单预占 |
| PnlManager | ✅ | - | - | 盈亏管理、浮亏拯救 |

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
│   │   ├── order_check.rs      # OrderCheck
│   │   └── thresholds.rs      # ThresholdConstants
│   │
│   ├── pin/                    # Pin策略风控 (插针高波动)
│   │   ├── mod.rs
│   │   └── pin_risk_limit.rs   # PinRiskLeverageGuard
│   │
│   └── trend/                  # Trend策略风控 (趋势)
│       ├── mod.rs
│       └── trend_risk_limit.rs  # TrendRiskLimitGuard
│
├── position/                    # 持仓管理
│   └── ...
│
├── persistence/                # 持久化
│   └── ...
│
└── shared/                     # 共享基础 (现有)
    ├── account_pool.rs         # AccountPool
    ├── margin_config.rs        # MarginPoolConfig
    ├── market_status.rs        # MarketStatusDetector
    ├── pnl_manager.rs          # PnlManager (盈亏+拯救)
    └── ...
```

### 2.2 策略参数对照

| 策略 | 代码层 | 品种上限 | 最小名义 | 保底阈值 | 初始比例 | 触发线 |
|------|--------|---------|---------|---------|---------|--------|
| Pin | Minute | 30 | 5 USDT | 150 USDT | 15% | 15 |
| Trend | Hour | 10 | 5 USDT | 50 USDT | 30% | 5 |

---

## 3. 详细设计

### 3.1 risk/common/ — 公共风控

#### 3.1.1 risk_prechecker — 风险预检器

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

#### 3.1.2 risk_rechecker — 风控复核器

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

#### 3.1.3 order_check — 订单检查器

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

#### 3.1.4 thresholds — 阈值常量

> **文件**: `risk/common/thresholds.rs`
> **策略类型**: 公共

```rust
/// 阈值常量模块
///
/// 集中管理所有策略阈值常量，避免硬编码。
pub struct ThresholdConstants {
    pub profit_threshold: Decimal,        // 1%
    pub stop_loss_threshold: Decimal,     // 5%
}
```

---

### 3.2 risk/pin/ — Pin策略风控

#### 3.2.1 pin_risk_limit — 动态杠杆

> **文件**: `risk/pin/pin_risk_limit.rs`
> **策略类型**: **Pin (插针高波动) 专用**
> **注意**: Trend 策略禁用此模块

```rust
/// 波动级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
}

impl Default for PinLeverageConfig {
    fn default() -> Self {
        Self {
            leverage_by_level: [dec!(15), dec!(10), dec!(5), dec!(2)],
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
    pub fn new(config: PinLeverageConfig) -> Self {
        Self { config }
    }

    /// 计算当前应该使用的杠杆
    pub fn calculate_leverage(
        &self,
        current_volatility: Decimal,
        base_leverage: Decimal,
    ) -> Decimal {
        let level = self.get_volatility_level(current_volatility);
        let level_leverage = self.config.get_leverage(level);
        level_leverage.min(base_leverage)
    }

    pub fn get_volatility_level(&self, volatility: Decimal) -> PinVolatilityLevel {
        // ...
    }

    pub fn should_reduce_leverage(&self, current_volatility: Decimal) -> bool {
        current_volatility >= self.config.high_volatility_threshold
    }
}
```

---

### 3.3 risk/trend/ — Trend策略风控

#### 3.3.1 trend_risk_limit — 品种限额

> **文件**: `risk/trend/trend_risk_limit.rs`
> **策略类型**: **Trend (趋势策略) 专用**
> **注意**: Pin 策略禁用此模块

```rust
/// 单品种持仓限制 (Trend 专用)
#[derive(Debug, Clone)]
pub struct TrendSymbolLimit {
    pub max_notional: Decimal,
    pub max_qty: Decimal,
}

/// 全局持仓限制 (Trend 专用)
#[derive(Debug, Clone)]
pub struct TrendGlobalLimit {
    pub max_total_notional: Decimal,
    pub max_symbol_count: u32,
}

/// Trend策略限额守卫 (Trend 专用)
///
/// **警告**: Pin 策略不应使用此模块！
#[derive(Debug, Clone)]
pub struct TrendRiskLimitGuard {
    symbol_limit: TrendSymbolLimit,
    global_limit: TrendGlobalLimit,
    current_notionals: FnvHashMap<String, Decimal>,
    current_quantities: FnvHashMap<String, Decimal>,
}

impl TrendRiskLimitGuard {
    pub fn new(symbol_limit: TrendSymbolLimit, global_limit: TrendGlobalLimit) -> Self {
        Self {
            symbol_limit,
            global_limit,
            current_notionals: FnvHashMap::default(),
            current_quantities: FnvHashMap::default(),
        }
    }

    pub fn pre_check(
        &self,
        symbol: &str,
        order_notional: Decimal,
        order_qty: Decimal,
    ) -> Result<(), EngineError> {
        // 1. 检查单品种限额
        let current_notional = self.current_notionals.get(symbol).copied().unwrap_or(dec!(0));
        let new_notional = current_notional + order_notional;

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

        // 3. 检查品种数限额
        let current_symbols = self.current_notionals.len() as u32;
        if !self.current_notionals.contains_key(symbol) && current_symbols >= self.global_limit.max_symbol_count {
            return Err(EngineError::PositionLimitExceeded(format!(
                "品种数 {} 达上限 {}",
                current_symbols, self.global_limit.max_symbol_count
            )));
        }

        Ok(())
    }

    pub fn update_position(&mut self, symbol: &str, notional: Decimal, qty: Decimal) {
        self.current_notionals.insert(symbol.to_string(), notional);
        self.current_quantities.insert(symbol.to_string(), qty);
    }

    pub fn reduce_position(&mut self, symbol: &str, notional: Decimal, qty: Decimal) {
        // ...
    }
}
```

---

### 3.4 模块导出设计

#### risk/pin/mod.rs

```rust
pub mod pin_risk_limit;

pub use pin_risk_limit::{PinVolatilityLevel, PinLeverageConfig, PinRiskLeverageGuard};
```

#### risk/trend/mod.rs

```rust
pub mod trend_risk_limit;

pub use trend_risk_limit::{TrendSymbolLimit, TrendGlobalLimit, TrendRiskLimitGuard};
```

#### risk/common/mod.rs

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

## 4. 保证金计算

### 4.1 单品种初始名义价值计算

> **核心原则**: 保证金是根本，名义价值只是业务表达层。必须留够机会成本给其他品种。

### 4.1.1 计算公式

```
单品种初始名义价值 = 总权益 × 80% × 40% × 初始开仓比例 × 1/品种数 × 杠杆
```

**分解计算**：
```
总权益 × 80%      = 全局可用保证金
全局可用 × 40%    = 策略分配保证金 (Pin 40%, Trend 40%)
策略分配 × 比例   = 策略初始开仓保证金 (Pin 15%, Trend 30%)
÷ 品种数          = 单品种基础保证金
× 杠杆            = 单品种名义价值
```

### 4.1.2 保底机制

```
品种保底保证金 = 品种数 × 最小名义 / 杠杆
实际保证金 = min(公式计算值, 保底值)
```

### 4.1.3 策略参数

| 策略 | 品种数 | 最小名义 | 保底阈值 | 初始比例 |
|------|--------|---------|---------|---------|
| Pin | 30 | 5 USDT | 150 USDT | 15% |
| Trend | 10 | 5 USDT | 50 USDT | 30% |

### 4.1.4 代码设计

```rust
pub fn calculate_initial_notional(
    total_equity: Decimal,
    leverage: Decimal,
    symbol_count: u32,
    min_notional: Decimal,
    initial_ratio: Decimal,    // Pin: 0.15, Trend: 0.30
) -> Decimal {
    let available = total_equity * dec!(0.8);
    let strategy_alloc = available * dec!(0.4);
    let initial_margin = strategy_alloc * initial_ratio;
    let per_symbol_margin = initial_margin / Decimal::from(symbol_count);
    let guarantee_margin = Decimal::from(symbol_count) * min_notional / leverage;
    let actual_margin = per_symbol_margin.min(guarantee_margin);
    actual_margin * leverage
}
```

---

## 5. 浮亏拯救机制

### 5.1 核心概念

```
┌─────────────────────────────────────────────────────────┐
│                   Pin 策略 (高波动)                        │
│  - 高杠杆插针，翻倍加仓 1-1-2-4-8                       │
│  - 选对 → 快速盈利                                     │
│  - 选错 → 转 Trend 趋势跟随 (不加仓)                   │
│  - 盈利用于 cover 低波动浮亏                            │
└─────────────────────────────────────────────────────────┘
                          ↑
                    盈利 cover
                          ↑
┌─────────────────────────────────────────────────────────┐
│                   低波动 (被拯救者)                        │
│  - 入场后波动不够高，产生浮亏                            │
│  - 被 Pin 盈利覆盖后出清                               │
│  - 释放保证金，寻找下一个机会                           │
└─────────────────────────────────────────────────────────┘
```

### 5.2 两个品种库

PnLManager 维护两个 Hash 结构：

```
低波动品种库: Hash<品种名, 实时盈亏>
    - 盈亏实时计算 (赚赔都算)
    - 低波动品种的浮亏需要被 cover

高波动品种库: Hash<品种名, 实时盈亏>
    - 只有结算时才计入盈亏
    - 高波动是拯救者
```

### 5.3 触发条件

**条件一：基础条件**
```
已实现盈利 + 累计盈利 ≥ 低波动品种总浮亏
```

**条件二：贪心 vs 见好就收**

```
品种数 < 品种上限/2 → 贪心，争取更大利润
品种数 ≥ 品种上限/2 → 见好就收，快速释放保证金
```

| 策略 | 品种上限 | 触发线 (上限/2) |
|------|---------|-----------------|
| Pin | 30 | 15 |
| Trend | 10 | 5 |

### 5.4 拯救流程

```
1. Pin 策略盈利时（必须已实现）
2. 检查 品种数 是否 ≥ 品种上限/2
   - 品种少 → 继续贪心持有，不触发
   - 品种多 → 进入第3步
3. 检查 盈利 + 累计盈利 是否 ≥ 低波动品种总浮亏
   - 不能 cover → 不操作
   - 能 cover → 全部平仓
4. 全部平仓后：
   - 删除所有低波动品种记录
   - 从策略池移除这些品种
   - 剩余盈利累加到累计盈利
```

### 5.5 代码设计

```rust
/// 盈亏覆盖检查结果
#[derive(Debug, Clone)]
pub struct PnlCoverageResult {
    pub can_cover: bool,
    pub total_loss: Decimal,
    pub current_profit: Decimal,
    pub accumulated_profit: Decimal,
    pub net_profit: Decimal,
    pub symbol_count: u32,
}

/// 解救结果
#[derive(Debug, Clone)]
pub struct RescueResult {
    pub success: bool,
    pub can_rescue: bool,
    pub total_loss: Decimal,
    pub total_available_profit: Decimal,
    pub rescued_symbols: Vec<String>,
    pub remaining_profit: Decimal,
}

impl PnlManager {
    /// 检查盈亏覆盖
    pub fn check_pnl_coverage(
        &self,
        high_vol_profit: Decimal,
        is_realized: bool,
    ) -> PnlCoverageResult {
        let accumulated = self.get_accumulated_profit();
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
            symbol_count: self.get_low_vol_count(),
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

        let rescued = self.clear_low_volatility_symbols();
        let remaining = coverage.net_profit;
        self.set_accumulated_profit(remaining);

        RescueResult {
            success: true,
            can_rescue: true,
            total_loss: coverage.total_loss,
            total_available_profit: coverage.current_profit + coverage.accumulated_profit,
            rescued_symbols: rescued,
            remaining_profit: remaining,
        }
    }
}
```

---

## 6. 性能优化

### 6.1 OrderCheck 使用 FnvHashMap

```rust
// order_check.rs

reservations: RwLock<FnvHashMap<String, OrderReservation>>,
```

### 6.2 PersistenceService 添加索引

```rust
pub struct PersistenceService {
    trade_records: Vec<TradeRecord>,
    trade_index_by_symbol: FnvHashMap<String, Vec<usize>>,
    trade_index_by_strategy: FnvHashMap<String, Vec<usize>>,
}
```

---

## 7. 修复问题

### 7.1 修复 total_used_margin 更新问题

```rust
pub fn deduct_margin(&self, amount: Decimal) -> Result<(), String> {
    let mut account = self.account.write();
    account.frozen -= amount;
    account.margin_used += amount;
    drop(account);
    *self.total_used_margin.write() += amount;
    Ok(())
}

pub fn release_margin(&self, amount: Decimal) {
    let mut account = self.account.write();
    let to_release = amount.min(account.margin_used);
    account.margin_used -= to_release;
    account.available += to_release;
    drop(account);
    *self.total_used_margin.write() -= to_release;
}
```

---

## 8. lib.rs 导出

```rust
// ========== risk 模块导出 ==========
pub use risk::common::{
    RiskPreChecker, RiskReChecker, OrderCheck, OrderCheckResult, OrderReservation,
    ThresholdConstants,
};

pub use risk::pin::{
    PinRiskLeverageGuard, PinLeverageConfig, PinVolatilityLevel,
};

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

## 9. 实现计划

| 步骤 | 任务 | 优先级 |
|------|------|--------|
| 1 | 新增 risk/common/ 目录 | P0 |
| 2 | 新增 risk/pin/ 目录 | P0 |
| 3 | 新增 risk/trend/ 目录 | P0 |
| 4 | 实现 RiskPreChecker | P0 |
| 5 | 实现 RiskReChecker | P0 |
| 6 | 实现 OrderCheck | P0 |
| 7 | 实现 TrendRiskLimitGuard | P1 |
| 8 | 实现 PinRiskLeverageGuard | P1 |
| 9 | 实现 PnlManager 拯救机制 | P1 |
| 10 | 修复 total_used_margin | P0 |
| 11 | FnvHashMap 优化 | P1 |
| 12 | 编写单元测试 | P1 |
| 13 | 更新文档 | P2 |

---

## 10. 向后兼容性

- 新增的 `risk/common/`、`risk/pin/`、`risk/trend/` 模块不影响现有 API
- `StrategyLevel` (Minute/Hour) 保持不变，映射到 Pin/Trend
- `TrendRiskLimitGuard`、`PinRiskLeverageGuard` 均为新增，可选择性使用
