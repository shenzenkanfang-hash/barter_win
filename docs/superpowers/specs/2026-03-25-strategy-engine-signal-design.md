# 策略层→引擎层 信号通信 规范文档

**Author**: Droid
**Created**: 2026-03-25
**Status**: Approved
**Version**: V1.0

---

## 一、现有代码问题检查报告

### 1.1 核心问题汇总

| 层级 | 问题 | 严重度 |
|------|------|--------|
| **架构断层** | `TriggerEvent` 在 d_checktable 内部生成，**完全未传递给 f_engine** | 致命 |
| **策略层空转** | `execute_strategy()` 是空实现，返回 `no_action` | 致命 |
| **数量计算缺失** | 策略层 `DaySignalOutput` 只有 `bool`，引擎被迫自己算 | 致命 |
| **仓位ID缺失** | `LocalPosition` 无 `position_id`，无法指定平仓 | 致命 |
| **b_close 占位符** | 始终返回 `false`，平仓逻辑未实现 | 中等 |
| **c_hedge 占位符** | 始终返回 `false`，对冲逻辑未实现 | 中等 |
| **CheckTable 未调用** | 接口定义了 `check(price, position)` 但从未被调用 | 中等 |

### 1.2 当前数据流（断裂）

```
d_checktable 侧:
  run_check_chain() → TriggerEvent { symbol, CheckSignal }  ← 引擎拿不到

f_engine 侧:
  process_tick()
    → MinuteTrigger.check()      ← 只做波动率检查
    → execute_strategy()         ← 空实现，返回 no_action
    → pre_check() / lock_check()
    → 创建订单
```

---

## 二、策略层→引擎层 统一信号数据结构

### 2.1 新增文件：`x_data/src/trading/signal.rs`

```rust
//! x_data/src/trading/signal.rs
//! 策略层→引擎层 统一信号结构

#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use crate::position::{PositionSide, PositionDirection};

// ============================================================================
// 策略标识
// ============================================================================

/// 策略类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrategyType {
    Trend,   // 趋势策略
    Pin,     // Pin因子策略
    Grid,    // 网格策略
}

/// 策略层级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrategyLevel {
    Minute,  // 分钟级
    Day,     // 日线级
}

/// 策略唯一标识
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyId {
    pub strategy_type: StrategyType,
    pub instance_id: String,
    pub level: StrategyLevel,
}

impl StrategyId {
    pub fn new_trend_minute(instance_id: impl Into<String>) -> Self {
        Self {
            strategy_type: StrategyType::Trend,
            instance_id: instance_id.into(),
            level: StrategyLevel::Minute,
        }
    }

    pub fn new_trend_day(instance_id: impl Into<String>) -> Self {
        Self {
            strategy_type: StrategyType::Trend,
            instance_id: instance_id.into(),
            level: StrategyLevel::Day,
        }
    }
}

// ============================================================================
// 交易指令
// ============================================================================

/// 交易指令类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradeCommand {
    Open,           // 开仓
    Add,            // 加仓
    Reduce,         // 减仓（部分平仓）
    FlatAll,        // 全平
    FlatPosition,   // 指定仓位平仓
    HedgeOpen,      // 对冲开仓
    HedgeClose,     // 对冲平仓
}

// ============================================================================
// 仓位引用
// ============================================================================

/// 仓位引用（用于指定平仓）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionRef {
    /// 仓位唯一ID
    pub position_id: String,
    /// 关联的策略实例ID
    pub strategy_instance_id: String,
    /// 持仓方向
    pub side: PositionSide,
}

// ============================================================================
// 策略信号（核心输出结构）
// ============================================================================

/// 策略信号（策略层 → 引擎层）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategySignal {
    /// 交易指令
    pub command: TradeCommand,
    /// 交易方向
    pub direction: PositionSide,
    /// 交易数量（策略层计算）
    pub quantity: Decimal,
    /// 目标价格
    pub target_price: Decimal,
    /// 策略标识
    pub strategy_id: StrategyId,
    /// 仓位引用（加仓/平仓时必须）
    pub position_ref: Option<PositionRef>,
    /// 是否全平（true=全部平掉）
    pub full_close: bool,
    /// 止损价格（可选）
    pub stop_loss_price: Option<Decimal>,
    /// 止盈价格（可选）
    pub take_profit_price: Option<Decimal>,
    /// 执行原因
    pub reason: String,
    /// 置信度 0-100
    pub confidence: u8,
    /// 触发时间戳
    pub timestamp: i64,
}

impl StrategySignal {
    /// 创建开仓信号
    pub fn open(
        direction: PositionSide,
        quantity: Decimal,
        target_price: Decimal,
        strategy_id: StrategyId,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            command: TradeCommand::Open,
            direction,
            quantity,
            target_price,
            strategy_id,
            position_ref: None,
            full_close: false,
            stop_loss_price: None,
            take_profit_price: None,
            reason: reason.into(),
            confidence: 80,
            timestamp: chrono::Utc::now().timestamp(),
        }
    }

    /// 创建加仓信号
    pub fn add(
        direction: PositionSide,
        quantity: Decimal,
        target_price: Decimal,
        strategy_id: StrategyId,
        position_ref: PositionRef,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            command: TradeCommand::Add,
            direction,
            quantity,
            target_price,
            strategy_id,
            position_ref: Some(position_ref),
            full_close: false,
            stop_loss_price: None,
            take_profit_price: None,
            reason: reason.into(),
            confidence: 75,
            timestamp: chrono::Utc::now().timestamp(),
        }
    }

    /// 创建全平信号
    pub fn flat_all(
        strategy_id: StrategyId,
        position_ref: PositionRef,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            command: TradeCommand::FlatAll,
            direction: position_ref.side,
            quantity: Decimal::ZERO,
            target_price: Decimal::ZERO,
            strategy_id,
            position_ref: Some(position_ref),
            full_close: true,
            stop_loss_price: None,
            take_profit_price: None,
            reason: reason.into(),
            confidence: 90,
            timestamp: chrono::Utc::now().timestamp(),
        }
    }

    /// 创建指定仓位平仓信号
    pub fn flat_position(
        quantity: Decimal,
        strategy_id: StrategyId,
        position_ref: PositionRef,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            command: TradeCommand::FlatPosition,
            direction: position_ref.side,
            quantity,
            target_price: Decimal::ZERO,
            strategy_id,
            position_ref: Some(position_ref),
            full_close: false,
            stop_loss_price: None,
            take_profit_price: None,
            reason: reason.into(),
            confidence: 85,
            timestamp: chrono::Utc::now().timestamp(),
        }
    }

    /// 创建减仓信号
    pub fn reduce(
        quantity: Decimal,
        strategy_id: StrategyId,
        position_ref: PositionRef,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            command: TradeCommand::Reduce,
            direction: position_ref.side,
            quantity,
            target_price: Decimal::ZERO,
            strategy_id,
            position_ref: Some(position_ref),
            full_close: false,
            stop_loss_price: None,
            take_profit_price: None,
            reason: reason.into(),
            confidence: 80,
            timestamp: chrono::Utc::now().timestamp(),
        }
    }
}
```

### 2.2 更新 `x_data/src/lib.rs`

在 `x_data/src/lib.rs` 中添加：

```rust
pub mod trading;
// ...
pub use trading::signal::{StrategySignal, TradeCommand, StrategyId, StrategyType, StrategyLevel, PositionRef};
```

---

## 三、日线策略（l_1d）数量计算实现方案

### 3.1 新增：`d_checktable/src/l_1d/quantity_calculator.rs`

```rust
//! d_checktable/src/l_1d/quantity_calculator.rs
//! 日线策略数量计算器

#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use crate::types::{DaySignalInput, DaySignalOutput, VolatilityTier};
use x_data::trading::signal::{StrategySignal, StrategyId, PositionRef, TradeCommand, PositionSide};

/// 日线策略数量配置
#[derive(Debug, Clone)]
pub struct DayQuantityConfig {
    /// 基础开仓数量
    pub base_open_qty: Decimal,
    /// 最大持仓数量
    pub max_position_qty: Decimal,
    /// 加仓倍数
    pub add_multiplier: Decimal,
    /// 波动率调整启用
    pub vol_adjustment: bool,
}

impl Default for DayQuantityConfig {
    fn default() -> Self {
        Self {
            base_open_qty: dec!(0.1),
            max_position_qty: dec!(0.3),
            add_multiplier: dec!(1.5),
            vol_adjustment: true,
        }
    }
}

/// 日线策略数量计算器
pub struct DayQuantityCalculator {
    config: DayQuantityConfig,
}

impl DayQuantityCalculator {
    pub fn new(config: DayQuantityConfig) -> Self {
        Self { config }
    }

    pub fn with_default() -> Self {
        Self::new(DayQuantityConfig::default())
    }

    /// 计算开仓数量
    pub fn calc_open_quantity(&self, vol_tier: &VolatilityTier) -> Decimal {
        let base = self.config.base_open_qty;
        if !self.config.vol_adjustment {
            return base;
        }
        match vol_tier {
            VolatilityTier::Low => base * dec!(1.2),
            VolatilityTier::Medium => base,
            VolatilityTier::High => base * dec!(0.8),
            VolatilityTier::Extreme => base * dec!(0.5),
        }
    }

    /// 计算加仓数量
    pub fn calc_add_quantity(
        &self,
        current_position_qty: Decimal,
        vol_tier: &VolatilityTier,
    ) -> Decimal {
        let mut add_qty = self.config.base_open_qty * self.config.add_multiplier;
        let max_add = self.config.max_position_qty - current_position_qty;
        if add_qty > max_add {
            add_qty = max_add;
        }
        if !self.config.vol_adjustment {
            return add_qty;
        }
        match vol_tier {
            VolatilityTier::Low => add_qty * dec!(1.2),
            VolatilityTier::Medium => add_qty,
            VolatilityTier::High => add_qty * dec!(0.7),
            VolatilityTier::Extreme => Decimal::ZERO,
        }
    }

    /// 生成完整策略信号
    pub fn generate_signal(
        &self,
        input: &DaySignalInput,
        signal_output: &DaySignalOutput,
        current_position_qty: Decimal,
        vol_tier: &VolatilityTier,
        strategy_id: StrategyId,
        position_ref: Option<PositionRef>,
    ) -> Option<StrategySignal> {
        // Exit > Close > Hedge > Add > Open 优先级

        // 1. 退出信号
        if signal_output.long_exit {
            return Some(StrategySignal::flat_all(
                strategy_id,
                position_ref?,
                "多头退出".to_string(),
            ));
        }
        if signal_output.short_exit {
            return Some(StrategySignal::flat_all(
                strategy_id,
                position_ref?,
                "空头退出".to_string(),
            ));
        }

        // 2. 加仓/对冲信号
        if signal_output.long_hedge || signal_output.short_hedge {
            let direction = if signal_output.long_hedge {
                PositionSide::Long
            } else {
                PositionSide::Short
            };
            let qty = self.calc_add_quantity(current_position_qty, vol_tier);
            if qty > Decimal::ZERO {
                return Some(StrategySignal::add(
                    direction,
                    qty,
                    Decimal::ZERO,
                    strategy_id,
                    position_ref?,
                    "加仓信号".to_string(),
                ));
            }
        }

        // 3. 开仓信号
        if signal_output.long_entry || signal_output.short_entry {
            let direction = if signal_output.long_entry {
                PositionSide::Long
            } else {
                PositionSide::Short
            };
            let qty = self.calc_open_quantity(vol_tier);
            return Some(StrategySignal::open(
                direction,
                qty,
                Decimal::ZERO,
                strategy_id,
                "开仓信号".to_string(),
            ));
        }

        None
    }
}
```

### 3.2 更新 `d_checktable/src/l_1d/check/check_chain.rs`

```rust
//! d_checktable/src/l_1d/check/check_chain.rs

use crate::l_1d::check::{a_exit, b_close, c_hedge, d_add, e_open};
use crate::l_1d::quantity_calculator::DayQuantityCalculator;
use crate::l_1d::signal_generator::DaySignalGenerator;
use crate::l_1d::market_status_generator::DayMarketStatusGenerator;
use crate::types::DaySignalInput;
use x_data::trading::signal::{StrategySignal, StrategyId, PositionRef};

/// 检查链上下文
#[derive(Debug, Clone)]
pub struct CheckChainContext {
    pub current_position_qty: Decimal,
    pub strategy_id: StrategyId,
    pub position_ref: Option<PositionRef>,
}

/// 执行检查链，返回策略信号
pub fn run_check_chain(
    symbol: &str,
    input: &DaySignalInput,
    ctx: &CheckChainContext,
) -> Option<StrategySignal> {
    let calculator = DayQuantityCalculator::with_default();
    let generator = DaySignalGenerator::new();
    let status_gen = DayMarketStatusGenerator::new();
    let vol_tier = status_gen.determine_volatility_level_from_signal(input);
    let signal_output = generator.generate(input, &vol_tier);

    calculator.generate_signal(
        input,
        &signal_output,
        ctx.current_position_qty,
        &vol_tier,
        ctx.strategy_id.clone(),
        ctx.position_ref.clone(),
    )
}
```

---

## 四、平仓功能（全局/指定仓位）传递方案

### 4.1 平仓命令类型

| 命令 | 说明 | full_close | position_ref |
|------|------|------------|--------------|
| FlatAll | 全平所有持仓 | true | 必须 |
| FlatPosition | 指定仓位平仓 | false | 必须含 position_id |
| Reduce | 部分减仓 | false | 必须 |

### 4.2 LocalPosition 新增 position_id

```rust
//! x_data/src/position/types.rs

pub struct LocalPosition {
    pub symbol: String,
    // ========== 新增 ==========
    pub position_id: String,           // 唯一标识
    pub strategy_instance_id: String,  // 关联策略
    // ========== 新增结束 ==========
    pub direction: PositionDirection,
    pub qty: Decimal,
    pub avg_price: Decimal,
    pub open_time: i64,
    pub position_cost: Decimal,
    pub updated_at: DateTime<Utc>,
}

impl LocalPosition {
    pub fn new(
        symbol: String,
        direction: PositionDirection,
        qty: Decimal,
        avg_price: Decimal,
        strategy_instance_id: String,
    ) -> Self {
        let position_id = format!(
            "{}_{}_{}_{}",
            symbol.to_lowercase(),
            match direction {
                PositionDirection::Long => "long",
                PositionDirection::Short => "short",
                _ => "flat",
            },
            strategy_instance_id,
            Utc::now().timestamp()
        );
        Self {
            symbol,
            position_id,
            strategy_instance_id,
            direction,
            qty,
            avg_price,
            open_time: Utc::now().timestamp(),
            position_cost: Decimal::ZERO,
            updated_at: Utc::now(),
        }
    }
}
```

---

## 五、全局规范（铁律）

| # | 规范 | 级别 |
|---|------|------|
| 1 | 策略层（d_checktable）必须生成完整 `StrategySignal` | 必须 |
| 2 | `StrategySignal` 在 `x_data/trading/signal.rs` 统一维护 | 必须 |
| 3 | 引擎层**不计算**任何策略数量，只执行 | 必须 |
| 4 | 平仓必须支持全平/指定仓位两种模式 | 必须 |
| 5 | 每个仓位必须有唯一 `position_id` | 必须 |
| 6 | 检查链 `run_check_chain` 返回 `Option<StrategySignal>` | 必须 |
| 7 | 日线/分钟策略数量逻辑完全独立 | 必须 |

---

## 六、整改步骤

| Phase | 任务 | 文件 |
|-------|------|------|
| P1 | 新增 `x_data/src/trading/signal.rs` | `StrategySignal`, `TradeCommand`, `StrategyId`, `PositionRef` |
| P1 | 更新 `x_data/src/lib.rs` 导出 | 添加 `pub use trading::signal::*` |
| P1 | `LocalPosition` 新增 `position_id`, `strategy_instance_id` | `x_data/src/position/types.rs` |
| P2 | 新增 `d_checktable/src/l_1d/quantity_calculator.rs` | 日线数量计算 |
| P2 | 更新 `check_chain.rs` 返回 `Option<StrategySignal>` | `d_checktable/src/l_1d/check/check_chain.rs` |
| P2 | 实现 `b_close.rs` 关仓逻辑 | `d_checktable/src/l_1d/check/b_close.rs` |
| P2 | 实现 `c_hedge.rs` 对冲逻辑 | `d_checktable/src/l_1d/check/c_hedge.rs` |
| P3 | 修改 `f_engine` 接收 `StrategySignal` | `f_engine/src/core/engine_v2.rs` |
| P3 | 实现 `execute_strategy` 调用检查链 | `f_engine/src/core/execution.rs` |
| P4 | 分钟级策略（h_15m）同步改造 | 参考日线实现 |

---

## 七、新旧结构对比

| 旧结构 | 新结构 | 变化 |
|-------|-------|------|
| `TriggerEvent { symbol, signal }` | `StrategySignal { command, direction, quantity, ... }` | 全面扩展 |
| `CheckSignal { Exit, Close, ... }` | `TradeCommand { Open, Add, Reduce, FlatAll, ... }` | 语义更清晰 |
| 无 | `StrategyId { strategy_type, instance_id, level }` | 新增 |
| 无 | `PositionRef { position_id, strategy_instance_id, side }` | 新增 |
| `LocalPosition` 无 ID | `LocalPosition { position_id, strategy_instance_id }` | 新增 |
