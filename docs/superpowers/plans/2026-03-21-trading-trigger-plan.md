# TradingTrigger Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 indicator 模块中实现完整的交易触发逻辑，包括市场状态生成、信号生成、价格控制判断和交易触发器。

**Architecture:** 采用分层架构，min/ 和 day/ 目录各有一套独立的 MarketStatusGenerator、SignalGenerator、PriceControlGenerator，由 TradingTrigger 根据波动率调度。

**Tech Stack:** Rust (rust_decimal, thiserror, serde), 增量计算 O(1), 无锁设计

---

## 文件结构

```
crates/indicator/src/
├── lib.rs                           # 修改：导出新模块
├── types.rs                         # 新增：公共类型定义
├── trading_trigger.rs               # 新增：交易触发器
├── min/
│   ├── market_status_generator.rs  # 新增
│   ├── signal_generator.rs         # 新增
│   └── price_control_generator.rs  # 新增
└── day/
    ├── market_status_generator.rs   # 新增
    ├── signal_generator.rs          # 新增
    └── price_control_generator.rs   # 新增
```

---

## Task 1: 创建 types.rs 公共类型定义

**Files:**
- Create: `crates/indicator/src/types.rs`
- Modify: `crates/indicator/src/lib.rs`

- [ ] **Step 1: 创建 types.rs**

```rust
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

// ==================== 公共枚举 ====================

/// 市场状态枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarketStatus {
    TREND,   // 趋势状态
    RANGE,   // 震荡状态
    PIN,     // 插针状态
    INVALID, // 数据无效
}

/// 波动率等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VolatilityLevel {
    HIGH,    // 高波动 (15min TR > 13%)
    NORMAL,  // 正常波动
    LOW,     // 低波动
}

/// 策略层级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrategyLevel {
    MIN,  // 分钟级策略
    DAY,  // 日线级策略
}

/// 交易动作
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradingAction {
    Long,      // 做多
    Short,     // 做空
    Flat,      // 平仓
    Hedge,     // 对冲
    Wait,      // 等待
}

/// 持仓方向
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PositionSide {
    LONG,
    SHORT,
    NONE,
}

// ==================== min/ 输入输出类型 ====================

/// 分钟级市场状态输入
#[derive(Debug, Clone)]
pub struct MinMarketStatusInput {
    pub tr_ratio_10min: Decimal,
    pub tr_ratio_15min: Decimal,
    pub price_position: Decimal,
    pub zscore: Decimal,
    pub tr_base_60min: Decimal,
}

/// 分钟级市场状态输出
#[derive(Debug, Clone)]
pub struct MinMarketStatusOutput {
    pub status: MarketStatus,
    pub volatility_level: VolatilityLevel,
    pub high_volatility_reason: Option<String>,
}

/// 分钟级信号输入
#[derive(Debug, Clone, Default)]
pub struct MinSignalInput {
    pub tr_base_60min: Decimal,
    pub tr_ratio_15min: Decimal,
    pub zscore_14_1m: Decimal,
    pub zscore_1h_1m: Decimal,
    pub tr_ratio_60min_5h: Decimal,
    pub tr_ratio_10min_1h: Decimal,
    pub pos_norm_60: Decimal,
    pub acc_percentile_1h: Decimal,
    pub velocity_percentile_1h: Decimal,
    pub pine_bg_color: String,
    pub pine_bar_color: String,
    pub price_deviation: Decimal,
    pub price_deviation_horizontal_position: Decimal,
}

impl Default for MinSignalInput {
    fn default() -> Self {
        Self {
            tr_base_60min: Decimal::ZERO,
            tr_ratio_15min: Decimal::ZERO,
            zscore_14_1m: Decimal::ZERO,
            zscore_1h_1m: Decimal::ZERO,
            tr_ratio_60min_5h: Decimal::ZERO,
            tr_ratio_10min_1h: Decimal::ZERO,
            pos_norm_60: dec!(50),
            acc_percentile_1h: Decimal::ZERO,
            velocity_percentile_1h: Decimal::ZERO,
            pine_bg_color: String::new(),
            pine_bar_color: String::new(),
            price_deviation: Decimal::ZERO,
            price_deviation_horizontal_position: dec!(50),
        }
    }
}

/// 分钟级信号输出
#[derive(Debug, Clone, Default)]
pub struct MinSignalOutput {
    pub long_entry: bool,
    pub short_entry: bool,
    pub long_exit: bool,
    pub short_exit: bool,
    pub long_hedge: bool,
    pub short_hedge: bool,
    pub exit_high_volatility: bool,
}

// ==================== day/ 输入输出类型 ====================

/// 日线级市场状态输入
#[derive(Debug, Clone)]
pub struct DayMarketStatusInput {
    pub tr_ratio_5d_20d: Decimal,
    pub tr_ratio_20d_60d: Decimal,
    pub pine_color: String,
    pub ma5_in_20d_ma5_pos: Decimal,
    pub power_percentile: Decimal,
}

/// 日线级市场状态输出
#[derive(Debug, Clone)]
pub struct DayMarketStatusOutput {
    pub status: MarketStatus,
    pub volatility_level: VolatilityLevel,
}

/// 日线级信号输入
#[derive(Debug, Clone)]
pub struct DaySignalInput {
    pub pine_color_100_200: String,
    pub pine_color_20_50: String,
    pub pine_color_12_26: String,
    pub tr_ratio_5d_20d: Decimal,
    pub tr_ratio_20d_60d: Decimal,
    pub ma5_in_20d_ma5_pos: Decimal,
}

/// 日线级信号输出
#[derive(Debug, Clone, Default)]
pub struct DaySignalOutput {
    pub long_entry: bool,
    pub short_entry: bool,
    pub long_exit: bool,
    pub short_exit: bool,
    pub long_hedge: bool,
    pub short_hedge: bool,
}

// ==================== PriceControl 类型 ====================

/// 价格控制输入
#[derive(Debug, Clone)]
pub struct PriceControlInput {
    pub position_entry_price: Decimal,
    pub position_side: PositionSide,
    pub position_size: Decimal,
    pub current_price: Decimal,
    pub profit_threshold: Decimal,
    pub loss_threshold: Decimal,
    pub add_threshold: Decimal,
    pub move_stop_threshold: Decimal,
}

/// 价格控制输出
#[derive(Debug, Clone)]
pub struct PriceControlOutput {
    pub should_add: bool,
    pub should_stop: bool,
    pub should_take_profit: bool,
    pub should_move_stop: bool,
    pub profit_distance_pct: Decimal,
    pub stop_distance_pct: Decimal,
}

// ==================== TradingTrigger 类型 ====================

/// 仓位状态 (简化版 CheckList)
#[derive(Debug, Clone)]
pub struct CheckList {
    pub long_positions: Vec<PositionRecord>,
    pub short_positions: Vec<PositionRecord>,
}

#[derive(Debug, Clone)]
pub struct PositionRecord {
    pub entry_price: Decimal,
    pub qty: Decimal,
}

/// 交易触发器输入
#[derive(Debug, Clone)]
pub struct TradingTriggerInput {
    pub symbol: String,
    pub current_price: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub min_indicators: MinSignalInput,
    pub day_indicators: DaySignalInput,
    pub check_list: CheckList,
}

/// 交易决策
#[derive(Debug, Clone)]
pub struct TradingDecision {
    pub action: TradingAction,
    pub reason: String,
    pub confidence: u8,
    pub level: StrategyLevel,
}
```

- [ ] **Step 2: 更新 lib.rs 导出**

```rust
pub mod types;

pub mod indicator_1d;
pub mod indicator_1m;
pub mod pine_indicator_full;

pub mod min;
pub mod day;

pub mod trading_trigger;

pub use indicator_1d::{BigCycleCalculator, BigCycleConfig, BigCycleIndicators, PineColorBig, TRRatioSignal};
pub use indicator_1m::{Indicator1m, Indicator1mOutput};
pub use pine_indicator_full::{PineColorDetector as PineColorDetectorV5, colors, DominantCycleRSI, EMA, RMA};
pub use types::*;
```

- [ ] **Step 3: 提交**

```bash
git add crates/indicator/src/types.rs crates/indicator/src/lib.rs
git commit -m "feat(indicator): 添加公共类型定义 types.rs"
```

---

## Task 2: 实现 min/market_status_generator.rs

**Files:**
- Create: `crates/indicator/src/min/market_status_generator.rs`
- Modify: `crates/indicator/src/min/mod.rs`

- [ ] **Step 1: 创建 market_status_generator.rs**

```rust
#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use crate::types::{MarketStatus, VolatilityLevel, MinMarketStatusInput, MinMarketStatusOutput};

/// 分钟级市场状态生成器
pub struct MinMarketStatusGenerator {
    data_timeout_seconds: i64,
}

impl Default for MinMarketStatusGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl MinMarketStatusGenerator {
    pub fn new() -> Self {
        Self {
            data_timeout_seconds: 180,
        }
    }

    /// 检测市场状态
    pub fn detect(&self, input: &MinMarketStatusInput) -> MinMarketStatusOutput {
        // 1. 判断波动率等级
        let volatility_level = self.determine_volatility_level(input.tr_ratio_15min);

        // 2. 判断市场状态 (优先级: INVALID > PIN > RANGE > TREND)
        let status = self.determine_status(input, &volatility_level);

        MinMarketStatusOutput {
            status,
            volatility_level,
            high_volatility_reason: None,
        }
    }

    /// 判断波动率等级
    fn determine_volatility_level(&self, tr_15min: Decimal) -> VolatilityLevel {
        if tr_15min > dec!(0.13) {
            VolatilityLevel::HIGH
        } else if tr_15min < dec!(0.03) {
            VolatilityLevel::LOW
        } else {
            VolatilityLevel::NORMAL
        }
    }

    /// 判断市场状态
    fn determine_status(&self, input: &MinMarketStatusInput, vol_level: &VolatilityLevel) -> MarketStatus {
        // PIN 条件检测 (前置: tr_base_60min > 15%)
        if input.tr_base_60min > dec!(0.15) {
            if self.is_pin_conditions_met(input) {
                return MarketStatus::PIN;
            }
        }

        // RANGE 条件
        if vol_level == &VolatilityLevel::LOW && input.tr_ratio_15min < dec!(1.0) {
            let zscore_near_zero = input.zscore.abs() < dec!(0.5);
            if zscore_near_zero {
                return MarketStatus::RANGE;
            }
        }

        MarketStatus::TREND
    }

    /// 检测插针条件是否满足 (7个条件满足 >= 4)
    fn is_pin_conditions_met(&self, input: &MinMarketStatusInput) -> bool {
        let mut satisfied: u8 = 0;

        // 1. extreme_z: |zscore_14_1m| > 2 或 |zscore_1h_1m| > 2
        if input.zscore_14_1m.abs() > dec!(2) || input.zscore_1h_1m.abs() > dec!(2) {
            satisfied += 1;
        }

        // 2. extreme_vol: tr_ratio_60min_5h > 1 或 tr_ratio_10min_1h > 1
        if input.tr_ratio_60min_5h > dec!(1) || input.tr_ratio_10min_1h > dec!(1) {
            satisfied += 1;
        }

        // 3. extreme_pos: pos_norm_60 > 90 或 < 10
        if input.pos_norm_60 > dec!(90) || input.pos_norm_60 < dec!(10) {
            satisfied += 1;
        }

        // 4. extreme_speed: acc_percentile_1h > 90
        if input.acc_percentile_1h > dec!(90) {
            satisfied += 1;
        }

        // 5. extreme_bg_color: "纯绿" 或 "纯红"
        if input.pine_bg_color == "纯绿" || input.pine_bg_color == "纯红" {
            satisfied += 1;
        }

        // 6. extreme_bar_color: "纯绿" 或 "纯红"
        if input.pine_bar_color == "纯绿" || input.pine_bar_color == "纯红" {
            satisfied += 1;
        }

        // 7. price_deviation_extreme: |horizontal_position| == 100
        if input.price_deviation_horizontal_position.abs() == dec!(100) {
            satisfied += 1;
        }

        satisfied >= 4
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_volatility_level_high() {
        let gen = MinMarketStatusGenerator::new();
        let input = MinMarketStatusInput {
            tr_ratio_15min: dec!(0.15),
            ..Default::default()
        };
        let output = gen.detect(&input);
        assert_eq!(output.volatility_level, VolatilityLevel::HIGH);
    }

    #[test]
    fn test_volatility_level_normal() {
        let gen = MinMarketStatusGenerator::new();
        let input = MinMarketStatusInput {
            tr_ratio_15min: dec!(0.08),
            ..Default::default()
        };
        let output = gen.detect(&input);
        assert_eq!(output.volatility_level, VolatilityLevel::NORMAL);
    }

    #[test]
    fn test_volatility_level_low() {
        let gen = MinMarketStatusGenerator::new();
        let input = MinMarketStatusInput {
            tr_ratio_15min: dec!(0.02),
            ..Default::default()
        };
        let output = gen.detect(&input);
        assert_eq!(output.volatility_level, VolatilityLevel::LOW);
    }
}
```

- [ ] **Step 2: 创建 min/mod.rs**

```rust
#![forbid(unsafe_code)]

pub mod market_status_generator;
pub mod signal_generator;
pub mod price_control_generator;

pub use market_status_generator::MinMarketStatusGenerator;
pub use signal_generator::MinSignalGenerator;
pub use price_control_generator::MinPriceControlGenerator;
```

- [ ] **Step 3: 提交**

```bash
git add crates/indicator/src/min/market_status_generator.rs crates/indicator/src/min/mod.rs
git commit -m "feat(indicator): 实现 min/MarketStatusGenerator"
```

---

## Task 3: 实现 min/signal_generator.rs

**Files:**
- Create: `crates/indicator/src/min/signal_generator.rs`

- [ ] **Step 1: 创建 signal_generator.rs**

```rust
#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use crate::types::{MinSignalInput, MinSignalOutput, VolatilityLevel};

/// 分钟级信号生成器
pub struct MinSignalGenerator;

impl MinSignalGenerator {
    pub fn new() -> Self {
        Self
    }

    /// 生成交易信号
    pub fn generate(&self, input: &MinSignalInput, vol_level: &VolatilityLevel) -> MinSignalOutput {
        // 前置条件: tr_base_60min > 15%
        if input.tr_base_60min <= dec!(0.15) {
            return MinSignalOutput::default();
        }

        // 检测插针条件
        let pin_satisfied = self.count_pin_conditions(input);

        // 入场信号 (前置: tr_base_60min > 15%)
        let long_entry = self.check_long_entry(input, pin_satisfied);
        let short_entry = self.check_short_entry(input, pin_satisfied);

        // 对冲信号 (前置: tr_base_60min < 15%)
        let long_hedge = self.check_long_hedge(input);
        let short_hedge = self.check_short_hedge(input);

        // 退出高波动
        let exit_high_volatility = self.check_exit_high_volatility(input);

        MinSignalOutput {
            long_entry,
            short_entry,
            long_exit: false, // 由 PriceControl 判断
            short_exit: false,
            long_hedge,
            short_hedge,
            exit_high_volatility,
        }
    }

    /// 统计满足的插针条件数量
    fn count_pin_conditions(&self, input: &MinSignalInput) -> u8 {
        let mut count: u8 = 0;

        if input.zscore_14_1m.abs() > dec!(2) || input.zscore_1h_1m.abs() > dec!(2) {
            count += 1;
        }
        if input.tr_ratio_60min_5h > dec!(1) || input.tr_ratio_10min_1h > dec!(1) {
            count += 1;
        }
        if input.pos_norm_60 > dec!(90) || input.pos_norm_60 < dec!(10) {
            count += 1;
        }
        if input.acc_percentile_1h > dec!(90) {
            count += 1;
        }
        if input.pine_bg_color == "纯绿" || input.pine_bg_color == "纯红" {
            count += 1;
        }
        if input.pine_bar_color == "纯绿" || input.pine_bar_color == "纯红" {
            count += 1;
        }
        if input.price_deviation_horizontal_position.abs() == dec!(100) {
            count += 1;
        }

        count
    }

    /// 检查做多入场条件
    fn check_long_entry(&self, input: &MinSignalInput, pin_satisfied: u8) -> bool {
        // 价格偏离方向: 向下
        if input.price_deviation >= dec!(0) {
            return false;
        }
        // 7个条件满足 >= 4
        pin_satisfied >= 4
    }

    /// 检查做空入场条件
    fn check_short_entry(&self, input: &MinSignalInput, pin_satisfied: u8) -> bool {
        // 价格偏离方向: 向上
        if input.price_deviation <= dec!(0) {
            return false;
        }
        // 7个条件满足 >= 4
        pin_satisfied >= 4
    }

    /// 检查多头对冲条件
    fn check_long_hedge(&self, input: &MinSignalInput) -> bool {
        // 前置: tr_base_60min < 15%
        if input.tr_base_60min >= dec!(0.15) {
            return false;
        }
        // 价格偏离向下
        if input.price_deviation >= dec!(0) {
            return false;
        }

        let mut conditions: u8 = 0;

        if input.tr_ratio_60min_5h > dec!(2) || input.tr_ratio_10min_1h > dec!(1) {
            conditions += 1;
        }
        if input.pos_norm_60 < dec!(90) {
            conditions += 1;
        }
        if input.acc_percentile_1h < dec!(10) && input.velocity_percentile_1h < dec!(10) {
            conditions += 1;
        }
        if input.pine_bg_color != "纯绿" {
            conditions += 1;
        }
        if input.pine_bar_color != "纯绿" {
            conditions += 1;
        }
        if dec!(10) < input.price_deviation_horizontal_position.abs() && input.price_deviation_horizontal_position.abs() <= dec!(90) {
            conditions += 1;
        }

        conditions >= 4
    }

    /// 检查空头对冲条件
    fn check_short_hedge(&self, input: &MinSignalInput) -> bool {
        if input.tr_base_60min >= dec!(0.15) {
            return false;
        }
        if input.price_deviation <= dec!(0) {
            return false;
        }

        let mut conditions: u8 = 0;

        if input.tr_ratio_60min_5h > dec!(2) || input.tr_ratio_10min_1h > dec!(1) {
            conditions += 1;
        }
        if input.pos_norm_60 > dec!(10) {
            conditions += 1;
        }
        if input.acc_percentile_1h > dec!(90) && input.velocity_percentile_1h > dec!(90) {
            conditions += 1;
        }
        if input.pine_bg_color != "纯红" {
            conditions += 1;
        }
        if input.pine_bar_color != "纯红" {
            conditions += 1;
        }
        if input.price_deviation_horizontal_position >= dec!(10) {
            conditions += 1;
        }

        conditions >= 4
    }

    /// 检查退出高波动条件
    fn check_exit_high_volatility(&self, input: &MinSignalInput) -> bool {
        if input.tr_base_60min >= dec!(0.15) {
            return false;
        }

        let cond1 = input.tr_ratio_60min_5h < dec!(1) && input.tr_ratio_10min_1h < dec!(1);
        let cond2 = input.pos_norm_60 > dec!(20) && input.pos_norm_60 < dec!(80);
        let cond3 = dec!(10) < input.price_deviation_horizontal_position.abs()
            && input.price_deviation_horizontal_position.abs() <= dec!(90);

        let satisfied = [cond1, cond2, cond3].iter().filter(|&&x| x).count();
        satisfied >= 2
    }
}

impl Default for MinSignalOutput {
    fn default() -> Self {
        Self {
            long_entry: false,
            short_entry: false,
            long_exit: false,
            short_exit: false,
            long_hedge: false,
            short_hedge: false,
            exit_high_volatility: false,
        }
    }
}
```

- [ ] **Step 2: 提交**

```bash
git add crates/indicator/src/min/signal_generator.rs
git commit -m "feat(indicator): 实现 min/SignalGenerator"
```

---

## Task 4: 实现 min/price_control_generator.rs

**Files:**
- Create: `crates/indicator/src/min/price_control_generator.rs`

- [ ] **Step 1: 创建 price_control_generator.rs**

```rust
#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use crate::types::{PriceControlInput, PriceControlOutput, PositionSide};

/// 分钟级价格控制器
pub struct MinPriceControlGenerator {
    default_profit_threshold: Decimal,
    default_loss_threshold: Decimal,
    default_add_threshold: Decimal,
    default_move_stop_threshold: Decimal,
}

impl Default for MinPriceControlGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl MinPriceControlGenerator {
    pub fn new() -> Self {
        Self {
            default_profit_threshold: dec!(0.01),   // 1%
            default_loss_threshold: dec!(0.02),    // 2%
            default_add_threshold: dec!(0.04),     // 4%
            default_move_stop_threshold: dec!(0.02), // 2%
        }
    }

    /// 检查价格控制条件
    pub fn check(&self, input: &PriceControlInput) -> PriceControlOutput {
        let (profit_distance_pct, stop_distance_pct) = self.calculate_distances(input);

        let should_stop = self.check_stop(input, stop_distance_pct);
        let should_take_profit = self.check_take_profit(input, profit_distance_pct);
        let should_add = self.check_add(input, profit_distance_pct);
        let should_move_stop = self.check_move_stop(input, profit_distance_pct);

        PriceControlOutput {
            should_add,
            should_stop,
            should_take_profit,
            should_move_stop,
            profit_distance_pct,
            stop_distance_pct,
        }
    }

    /// 计算盈亏距离
    fn calculate_distances(&self, input: &PriceControlInput) -> (Decimal, Decimal) {
        if input.position_size <= Decimal::ZERO {
            return (Decimal::ZERO, Decimal::ZERO);
        }

        let entry = input.position_entry_price;
        let current = input.current_price;

        if entry <= Decimal::ZERO {
            return (Decimal::ZERO, Decimal::ZERO);
        }

        match input.position_side {
            PositionSide::LONG => {
                let profit = (current - entry) / entry;
                let loss = (entry - current) / entry;
                (profit, loss)
            }
            PositionSide::SHORT => {
                let profit = (entry - current) / entry;
                let loss = (current - entry) / entry;
                (profit, loss)
            }
            PositionSide::NONE => (Decimal::ZERO, Decimal::ZERO),
        }
    }

    /// 检查止损
    fn check_stop(&self, input: &PriceControlInput, stop_distance: Decimal) -> bool {
        if input.position_size <= Decimal::ZERO {
            return false;
        }
        stop_distance >= input.loss_threshold
    }

    /// 检查止盈
    fn check_take_profit(&self, input: &PriceControlInput, profit_distance: Decimal) -> bool {
        if input.position_size <= Decimal::ZERO {
            return false;
        }
        profit_distance >= input.profit_threshold
    }

    /// 检查加仓
    fn check_add(&self, input: &PriceControlInput, profit_distance: Decimal) -> bool {
        if input.position_size <= Decimal::ZERO {
            return false;
        }
        profit_distance >= input.add_threshold
    }

    /// 检查移动止损
    fn check_move_stop(&self, input: &PriceControlInput, profit_distance: Decimal) -> bool {
        if input.position_size <= Decimal::ZERO {
            return false;
        }
        profit_distance >= input.move_stop_threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profit_distance_long() {
        let gen = MinPriceControlGenerator::new();
        let input = PriceControlInput {
            position_entry_price: dec!(100),
            position_side: PositionSide::LONG,
            position_size: dec!(1),
            current_price: dec!(102),
            profit_threshold: dec!(0.01),
            loss_threshold: dec!(0.02),
            add_threshold: dec!(0.04),
            move_stop_threshold: dec!(0.02),
        };

        let output = gen.check(&input);
        assert_eq!(output.profit_distance_pct, dec!(0.02));
        assert!(output.should_take_profit); // 2% > 1% 阈值
    }

    #[test]
    fn test_loss_distance_short() {
        let gen = MinPriceControlGenerator::new();
        let input = PriceControlInput {
            position_entry_price: dec!(100),
            position_side: PositionSide::SHORT,
            position_size: dec!(1),
            current_price: dec!(103),
            profit_threshold: dec!(0.01),
            loss_threshold: dec!(0.02),
            add_threshold: dec!(0.04),
            move_stop_threshold: dec!(0.02),
        };

        let output = gen.check(&input);
        assert_eq!(output.stop_distance_pct, dec!(0.03));
        assert!(output.should_stop); // 3% > 2% 止损阈值
    }

    #[test]
    fn test_no_position() {
        let gen = MinPriceControlGenerator::new();
        let input = PriceControlInput {
            position_entry_price: dec!(100),
            position_side: PositionSide::NONE,
            position_size: dec!(0),
            current_price: dec!(102),
            profit_threshold: dec!(0.01),
            loss_threshold: dec!(0.02),
            add_threshold: dec!(0.04),
            move_stop_threshold: dec!(0.02),
        };

        let output = gen.check(&input);
        assert!(!output.should_stop);
        assert!(!output.should_take_profit);
        assert!(!output.should_add);
    }
}
```

- [ ] **Step 2: 提交**

```bash
git add crates/indicator/src/min/price_control_generator.rs
git commit -m "feat(indicator): 实现 min/PriceControlGenerator"
```

---

## Task 5: 实现 day/market_status_generator.rs

**Files:**
- Create: `crates/indicator/src/day/market_status_generator.rs`
- Create: `crates/indicator/src/day/mod.rs`

- [ ] **Step 1: 创建 market_status_generator.rs**

```rust
#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use crate::types::{MarketStatus, VolatilityLevel, DayMarketStatusInput, DayMarketStatusOutput};

/// 日线级市场状态生成器
pub struct DayMarketStatusGenerator;

impl Default for DayMarketStatusGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl DayMarketStatusGenerator {
    pub fn new() -> Self {
        Self
    }

    /// 检测市场状态
    pub fn detect(&self, input: &DayMarketStatusInput) -> DayMarketStatusOutput {
        // 1. 判断波动率等级
        let volatility_level = self.determine_volatility_level(input);

        // 2. 判断市场状态
        let status = self.determine_status(input, &volatility_level);

        DayMarketStatusOutput {
            status,
            volatility_level,
        }
    }

    /// 判断波动率等级
    fn determine_volatility_level(&self, input: &DayMarketStatusInput) -> VolatilityLevel {
        // 日线: TR 极端判定
        if input.tr_ratio_5d_20d > dec!(2.0) || input.tr_ratio_20d_60d > dec!(2.0) {
            VolatilityLevel::HIGH
        } else if input.tr_ratio_5d_20d < dec!(0.5) && input.tr_ratio_20d_60d < dec!(0.5) {
            VolatilityLevel::LOW
        } else {
            VolatilityLevel::NORMAL
        }
    }

    /// 判断市场状态
    fn determine_status(&self, input: &DayMarketStatusInput, vol_level: &VolatilityLevel) -> MarketStatus {
        // PIN: 日线 PineColor + TR 极端
        if vol_level == &VolatilityLevel::HIGH {
            return MarketStatus::PIN;
        }

        // RANGE: 低 TR + 无强趋势颜色 + 动能适中
        if input.tr_ratio_20d_60d < dec!(0.8) {
            let color_is_weak = input.pine_color == "浅绿" || input.pine_color == "浅红";
            let power_moderate = input.power_percentile > dec!(20) && input.power_percentile < dec!(80);
            if color_is_weak && power_moderate {
                return MarketStatus::RANGE;
            }
        }

        MarketStatus::TREND
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trend_status() {
        let gen = DayMarketStatusGenerator::new();
        let input = DayMarketStatusInput {
            tr_ratio_5d_20d: dec!(1.0),
            tr_ratio_20d_60d: dec!(1.0),
            pine_color: "纯绿".to_string(),
            ma5_in_20d_ma5_pos: dec!(50),
            power_percentile: dec!(50),
        };

        let output = gen.detect(&input);
        assert_eq!(output.status, MarketStatus::TREND);
    }

    #[test]
    fn test_range_status() {
        let gen = DayMarketStatusGenerator::new();
        let input = DayMarketStatusInput {
            tr_ratio_5d_20d: dec!(0.5),
            tr_ratio_20d_60d: dec!(0.5),
            pine_color: "浅绿".to_string(),
            ma5_in_20d_ma5_pos: dec!(50),
            power_percentile: dec!(50),
        };

        let output = gen.detect(&input);
        assert_eq!(output.status, MarketStatus::RANGE);
    }
}
```

- [ ] **Step 2: 创建 day/mod.rs**

```rust
#![forbid(unsafe_code)]

pub mod market_status_generator;
pub mod signal_generator;
pub mod price_control_generator;

pub use market_status_generator::DayMarketStatusGenerator;
pub use signal_generator::DaySignalGenerator;
pub use price_control_generator::DayPriceControlGenerator;
```

- [ ] **Step 3: 提交**

```bash
git add crates/indicator/src/day/market_status_generator.rs crates/indicator/src/day/mod.rs
git commit -m "feat(indicator): 实现 day/MarketStatusGenerator"
```

---

## Task 6: 实现 day/signal_generator.rs

**Files:**
- Create: `crates/indicator/src/day/signal_generator.rs`

- [ ] **Step 1: 创建 signal_generator.rs**

```rust
#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use crate::types::{DaySignalInput, DaySignalOutput};

/// 日线级信号生成器
pub struct DaySignalGenerator;

impl DaySignalGenerator {
    pub fn new() -> Self {
        Self
    }

    /// 生成交易信号
    pub fn generate(&self, input: &DaySignalInput) -> DaySignalOutput {
        // Pine颜色分组验证
        let valid_groups = self.validate_pine_color_groups(input);
        if valid_groups.is_empty() {
            return DaySignalOutput::default();
        }

        let max_valid_period = self.get_max_valid_period(&valid_groups);

        // 入场信号
        let long_entry = self.check_long_entry(input, &valid_groups);
        let short_entry = self.check_short_entry(input, &valid_groups);

        // 平仓信号 (使用最大有效周期)
        let long_exit = self.check_long_exit(input, max_valid_period);
        let short_exit = self.check_short_exit(input, max_valid_period);

        // 对冲信号
        let long_hedge = self.check_long_hedge(input, max_valid_period);
        let short_hedge = self.check_short_hedge(input, max_valid_period);

        DaySignalOutput {
            long_entry,
            short_entry,
            long_exit,
            short_exit,
            long_hedge,
            short_hedge,
        }
    }

    /// 验证 Pine 颜色分组 (返回有效组列表)
    fn validate_pine_color_groups(&self, input: &DaySignalInput) -> Vec<&'static str> {
        let mut valid = Vec::new();

        // 12_26 组
        if !input.pine_color_12_26.is_empty() {
            valid.push("12_26");
        }
        // 20_50 组
        if !input.pine_color_20_50.is_empty() {
            valid.push("20_50");
        }
        // 100_200 组
        if !input.pine_color_100_200.is_empty() {
            valid.push("100_200");
        }

        valid
    }

    /// 获取最大有效周期 (优先级: 100_200 > 20_50 > 12_26)
    fn get_max_valid_period(&self, valid_groups: &[&str]) -> Option<&'static str> {
        for period in ["100_200", "20_50", "12_26"] {
            if valid_groups.contains(&period) {
                return Some(period);
            }
        }
        None
    }

    /// 检查做多入场条件
    fn check_long_entry(&self, input: &DaySignalInput, valid_groups: &[&str]) -> bool {
        // 检查最小周期 12_26 (如果有效必须纯绿)
        if valid_groups.contains(&"12_26") && input.pine_color_12_26 != "纯绿" {
            return false;
        }

        // 所有有效组必须为纯绿
        for &group in valid_groups {
            let color = match group {
                "12_26" => &input.pine_color_12_26,
                "20_50" => &input.pine_color_20_50,
                "100_200" => &input.pine_color_100_200,
                _ => continue,
            };
            if *color != "纯绿" {
                return false;
            }
        }

        // TR > 1
        let tr_condition = input.tr_ratio_5d_20d > dec!(1) || input.tr_ratio_20d_60d > dec!(1);
        // MA5 位置 > 70
        let pos_condition = input.ma5_in_20d_ma5_pos > dec!(70);

        tr_condition && pos_condition
    }

    /// 检查做空入场条件
    fn check_short_entry(&self, input: &DaySignalInput, valid_groups: &[&str]) -> bool {
        // 检查最小周期 12_26 (如果有效必须是紫色或纯红)
        if valid_groups.contains(&"12_26") {
            let color = &input.pine_color_12_26;
            if *color != "紫色" && *color != "纯红" {
                return false;
            }
        }

        // 所有有效组必须是紫色或纯红
        for &group in valid_groups {
            let color = match group {
                "12_26" => &input.pine_color_12_26,
                "20_50" => &input.pine_color_20_50,
                "100_200" => &input.pine_color_100_200,
                _ => continue,
            };
            if *color != "紫色" && *color != "纯红" {
                return false;
            }
        }

        let tr_condition = input.tr_ratio_5d_20d > dec!(1) || input.tr_ratio_20d_60d > dec!(1);
        let pos_condition = input.ma5_in_20d_ma5_pos < dec!(30);

        tr_condition && pos_condition
    }

    /// 检查做多平仓条件
    fn check_long_exit(&self, input: &DaySignalInput, max_period: Option<&str>) -> bool {
        let Some(period) = max_period else { return false; };

        let bg_color = match period {
            "100_200" => &input.pine_color_100_200,
            "20_50" => &input.pine_color_20_50,
            "12_26" => &input.pine_color_12_26,
            _ => return false,
        };

        let color_invalid = *bg_color != "纯绿";
        let pos_condition = input.ma5_in_20d_ma5_pos > dec!(50);

        color_invalid && pos_condition
    }

    /// 检查做空平仓条件
    fn check_short_exit(&self, input: &DaySignalInput, max_period: Option<&str>) -> bool {
        let Some(period) = max_period else { return false; };

        let bg_color = match period {
            "100_200" => &input.pine_color_100_200,
            "20_50" => &input.pine_color_20_50,
            "12_26" => &input.pine_color_12_26,
            _ => return false,
        };

        let color_invalid = *bg_color != "纯红";
        let pos_condition = input.ma5_in_20d_ma5_pos < dec!(50);

        color_invalid && pos_condition
    }

    /// 检查多头对冲条件
    fn check_long_hedge(&self, input: &DaySignalInput, max_period: Option<&str>) -> bool {
        let Some(period) = max_period else { return false; };

        let bg_color = match period {
            "100_200" => &input.pine_color_100_200,
            "20_50" => &input.pine_color_20_50,
            "12_26" => &input.pine_color_12_26,
            _ => return false,
        };

        bg_color == "淡绿" && input.ma5_in_20d_ma5_pos > dec!(50)
    }

    /// 检查空头对冲条件
    fn check_short_hedge(&self, input: &DaySignalInput, max_period: Option<&str>) -> bool {
        let Some(period) = max_period else { return false; };

        let bg_color = match period {
            "100_200" => &input.pine_color_100_200,
            "20_50" => &input.pine_color_20_50,
            "12_26" => &input.pine_color_12_26,
            _ => return false,
        };

        bg_color == "淡红" && input.ma5_in_20d_ma5_pos < dec!(50)
    }
}

impl Default for DaySignalOutput {
    fn default() -> Self {
        Self {
            long_entry: false,
            short_entry: false,
            long_exit: false,
            short_exit: false,
            long_hedge: false,
            short_hedge: false,
        }
    }
}
```

- [ ] **Step 2: 提交**

```bash
git add crates/indicator/src/day/signal_generator.rs
git commit -m "feat(indicator): 实现 day/SignalGenerator"
```

---

## Task 7: 实现 day/price_control_generator.rs

**Files:**
- Create: `crates/indicator/src/day/price_control_generator.rs`

- [ ] **Step 1: 创建 price_control_generator.rs**

```rust
#![forbid(unsafe_code)]

use crate::types::{PriceControlInput, PriceControlOutput};
use min::price_control_generator::MinPriceControlGenerator;

/// 日线级价格控制器 (复用分钟级实现)
pub type DayPriceControlGenerator = MinPriceControlGenerator;
```

- [ ] **Step 2: 提交**

```bash
git add crates/indicator/src/day/price_control_generator.rs
git commit -m "feat(indicator): 实现 day/PriceControlGenerator (复用 min 实现)"
```

---

## Task 8: 实现 trading_trigger.rs

**Files:**
- Create: `crates/indicator/src/trading_trigger.rs`

- [ ] **Step 1: 创建 trading_trigger.rs**

```rust
#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use crate::types::{
    TradingTriggerInput, TradingDecision, TradingAction, StrategyLevel, VolatilityLevel,
    MinSignalOutput, DaySignalOutput, PriceControlOutput,
};
use crate::min::{
    MinMarketStatusGenerator, MinSignalGenerator, MinPriceControlGenerator,
};
use crate::day::{
    DayMarketStatusGenerator, DaySignalGenerator, DayPriceControlGenerator,
};

/// 交易触发器
pub struct TradingTrigger {
    min_status_gen: MinMarketStatusGenerator,
    min_signal_gen: MinSignalGenerator,
    min_price_ctrl: MinPriceControlGenerator,

    day_status_gen: DayMarketStatusGenerator,
    day_signal_gen: DaySignalGenerator,
    day_price_ctrl: DayPriceControlGenerator,
}

impl Default for TradingTrigger {
    fn default() -> Self {
        Self::new()
    }
}

impl TradingTrigger {
    pub fn new() -> Self {
        Self {
            min_status_gen: MinMarketStatusGenerator::new(),
            min_signal_gen: MinSignalGenerator::new(),
            min_price_ctrl: MinPriceControlGenerator::new(),
            day_status_gen: DayMarketStatusGenerator::new(),
            day_signal_gen: DaySignalGenerator::new(),
            day_price_ctrl: DayPriceControlGenerator::new(),
        }
    }

    /// 执行交易决策
    pub fn run(&mut self, input: &TradingTriggerInput) -> TradingDecision {
        // 1. 计算波动率等级
        let vol_level = self.calculate_volatility_level(input);

        // 2. 根据波动率选择策略
        match vol_level {
            VolatilityLevel::HIGH => self.run_min_strategy(input),
            _ => self.run_day_strategy(input),
        }
    }

    /// 计算波动率等级
    fn calculate_volatility_level(&self, input: &TradingTriggerInput) -> VolatilityLevel {
        let tr_15min = input.min_indicators.tr_ratio_15min;

        if tr_15min > dec!(0.13) {
            VolatilityLevel::HIGH
        } else if tr_15min < dec!(0.03) {
            VolatilityLevel::LOW
        } else {
            VolatilityLevel::NORMAL
        }
    }

    /// 运行分钟级策略
    fn run_min_strategy(&self, input: &TradingTriggerInput) -> TradingDecision {
        // 1. SignalGenerator
        let signal = self.min_signal_gen.generate(&input.min_indicators, &VolatilityLevel::HIGH);

        // 2. PriceControlGenerator
        let price_ctrl = self.make_price_control_input(input);
        let price_ctrl_output = self.min_price_ctrl.check(&price_ctrl);

        // 3. 综合决策
        self.make_decision_min(&signal, &price_ctrl_output)
    }

    /// 运行日线级策略
    fn run_day_strategy(&self, input: &TradingTriggerInput) -> TradingDecision {
        // 1. SignalGenerator
        let signal = self.day_signal_gen.generate(&input.day_indicators);

        // 2. PriceControlGenerator
        let price_ctrl = self.make_price_control_input(input);
        let price_ctrl_output = self.day_price_ctrl.check(&price_ctrl);

        // 3. 综合决策
        self.make_decision_day(&signal, &price_ctrl_output)
    }

    /// 构建价格控制输入
    fn make_price_control_input(&self, input: &TradingTriggerInput) -> PriceControlInput {
        // 从 check_list 获取最新持仓
        let (entry_price, side, size) = if !input.check_list.long_positions.is_empty() {
            let pos = &input.check_list.long_positions[0];
            (pos.entry_price, crate::types::PositionSide::LONG, pos.qty)
        } else if !input.check_list.short_positions.is_empty() {
            let pos = &input.check_list.short_positions[0];
            (pos.entry_price, crate::types::PositionSide::SHORT, pos.qty)
        } else {
            (Decimal::ZERO, crate::types::PositionSide::NONE, Decimal::ZERO)
        };

        PriceControlInput {
            position_entry_price: entry_price,
            position_side: side,
            position_size: size,
            current_price: input.current_price,
            profit_threshold: dec!(0.01),
            loss_threshold: dec!(0.02),
            add_threshold: dec!(0.04),
            move_stop_threshold: dec!(0.02),
        }
    }

    /// 综合分钟级决策
    fn make_decision_min(&self, signal: &MinSignalOutput, price_ctrl: &PriceControlOutput) -> TradingDecision {
        // 优先级: 止损 > 止盈 > 对冲 > 开仓 > 等待

        if price_ctrl.should_stop {
            return TradingDecision {
                action: TradingAction::Flat,
                reason: "min_stop_loss".to_string(),
                confidence: 100,
                level: StrategyLevel::MIN,
            };
        }

        if price_ctrl.should_take_profit {
            return TradingDecision {
                action: TradingAction::Flat,
                reason: "min_take_profit".to_string(),
                confidence: 95,
                level: StrategyLevel::MIN,
            };
        }

        if signal.long_hedge {
            return TradingDecision {
                action: TradingAction::Hedge,
                reason: "min_long_hedge".to_string(),
                confidence: 80,
                level: StrategyLevel::MIN,
            };
        }

        if signal.short_hedge {
            return TradingDecision {
                action: TradingAction::Hedge,
                reason: "min_short_hedge".to_string(),
                confidence: 80,
                level: StrategyLevel::MIN,
            };
        }

        if signal.long_entry {
            return TradingDecision {
                action: TradingAction::Long,
                reason: "min_long_entry".to_string(),
                confidence: 75,
                level: StrategyLevel::MIN,
            };
        }

        if signal.short_entry {
            return TradingDecision {
                action: TradingAction::Short,
                reason: "min_short_entry".to_string(),
                confidence: 75,
                level: StrategyLevel::MIN,
            };
        }

        TradingDecision {
            action: TradingAction::Wait,
            reason: "min_no_signal".to_string(),
            confidence: 0,
            level: StrategyLevel::MIN,
        }
    }

    /// 综合日线级决策
    fn make_decision_day(&self, signal: &DaySignalOutput, price_ctrl: &PriceControlOutput) -> TradingDecision {
        if price_ctrl.should_stop {
            return TradingDecision {
                action: TradingAction::Flat,
                reason: "day_stop_loss".to_string(),
                confidence: 100,
                level: StrategyLevel::DAY,
            };
        }

        if price_ctrl.should_take_profit {
            return TradingDecision {
                action: TradingAction::Flat,
                reason: "day_take_profit".to_string(),
                confidence: 95,
                level: StrategyLevel::DAY,
            };
        }

        if signal.long_exit {
            return TradingDecision {
                action: TradingAction::Flat,
                reason: "day_long_exit".to_string(),
                confidence: 85,
                level: StrategyLevel::DAY,
            };
        }

        if signal.short_exit {
            return TradingDecision {
                action: TradingAction::Flat,
                reason: "day_short_exit".to_string(),
                confidence: 85,
                level: StrategyLevel::DAY,
            };
        }

        if signal.long_hedge {
            return TradingDecision {
                action: TradingAction::Hedge,
                reason: "day_long_hedge".to_string(),
                confidence: 80,
                level: StrategyLevel::DAY,
            };
        }

        if signal.short_hedge {
            return TradingDecision {
                action: TradingAction::Hedge,
                reason: "day_short_hedge".to_string(),
                confidence: 80,
                level: StrategyLevel::DAY,
            };
        }

        if signal.long_entry {
            return TradingDecision {
                action: TradingAction::Long,
                reason: "day_long_entry".to_string(),
                confidence: 70,
                level: StrategyLevel::DAY,
            };
        }

        if signal.short_entry {
            return TradingDecision {
                action: TradingAction::Short,
                reason: "day_short_entry".to_string(),
                confidence: 70,
                level: StrategyLevel::DAY,
            };
        }

        TradingDecision {
            action: TradingAction::Wait,
            reason: "day_no_signal".to_string(),
            confidence: 0,
            level: StrategyLevel::DAY,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_input() -> TradingTriggerInput {
        TradingTriggerInput {
            symbol: "BTCUSDT".to_string(),
            current_price: dec!(50000),
            high: dec!(50500),
            low: dec!(49500),
            close: dec!(50000),
            min_indicators: crate::types::MinSignalInput {
                tr_base_60min: dec!(0.16),
                tr_ratio_15min: dec!(0.15),
                zscore_14_1m: dec!(2.5),
                zscore_1h_1m: dec!(1.0),
                tr_ratio_60min_5h: dec!(1.2),
                tr_ratio_10min_1h: dec!(0.8),
                pos_norm_60: dec!(95),
                acc_percentile_1h: dec!(92),
                pine_bg_color: "纯绿".to_string(),
                pine_bar_color: "纯绿".to_string(),
                price_deviation: dec!(-0.02),
                price_deviation_horizontal_position: dec!(100),
                velocity_percentile_1h: dec!(95),
            },
            day_indicators: crate::types::DaySignalInput {
                pine_color_100_200: "纯绿".to_string(),
                pine_color_20_50: "纯绿".to_string(),
                pine_color_12_26: "纯绿".to_string(),
                tr_ratio_5d_20d: dec!(1.5),
                tr_ratio_20d_60d: dec!(1.2),
                ma5_in_20d_ma5_pos: dec!(75),
            },
            check_list: crate::types::CheckList {
                long_positions: vec![crate::types::PositionRecord {
                    entry_price: dec!(49000),
                    qty: dec!(0.1),
                }],
                short_positions: vec![],
            },
        }
    }

    #[test]
    fn test_high_volatility_triggers_min_strategy() {
        let mut trigger = TradingTrigger::new();
        let input = create_test_input();

        let decision = trigger.run(&input);

        assert_eq!(decision.level, StrategyLevel::MIN);
        assert_eq!(decision.action, TradingAction::Long); // long_entry signal
    }

    #[test]
    fn test_stop_loss_priority() {
        let mut trigger = TradingTrigger::new();
        let mut input = create_test_input();

        // 设置亏损超过阈值
        input.current_price = dec!(47000); // 从 49000 跌到 47000, 亏损 > 4%

        let decision = trigger.run(&input);

        assert_eq!(decision.action, TradingAction::Flat);
        assert!(decision.reason.contains("stop"));
    }
}
```

- [ ] **Step 2: 提交**

```bash
git add crates/indicator/src/trading_trigger.rs
git commit -m "feat(indicator): 实现 TradingTrigger 交易触发器"
```

---

## Task 9: 更新 lib.rs 导出

**Files:**
- Modify: `crates/indicator/src/lib.rs`

- [ ] **Step 1: 更新 lib.rs**

```rust
#![forbid(unsafe_code)]

pub mod types;
pub mod trading_trigger;

pub mod indicator_1d;
pub mod indicator_1m;
pub mod pine_indicator_full;

pub mod min;
pub mod day;

// Re-export common types
pub use types::*;
pub use trading_trigger::TradingTrigger;

// Re-export generators
pub use min::{
    MinMarketStatusGenerator,
    MinSignalGenerator,
    MinPriceControlGenerator,
};
pub use day::{
    DayMarketStatusGenerator,
    DaySignalGenerator,
    DayPriceControlGenerator,
};
```

- [ ] **Step 2: 提交**

```bash
git add crates/indicator/src/lib.rs
git commit -m "feat(indicator): 更新 lib.rs 导出新模块"
```

---

## Task 10: 编译验证

**Files:**
- None (编译验证)

- [ ] **Step 1: 运行 cargo check**

```bash
cd crates/indicator && cargo check --all-features
```

Expected: 无编译错误

- [ ] **Step 2: 运行测试**

```bash
cd crates/indicator && cargo test --lib
```

Expected: 所有测试通过

- [ ] **Step 3: 提交**

```bash
git add -A
git commit -m "test(indicator): 编译验证通过"
```

---

## 实施顺序

1. Task 1: types.rs 公共类型定义
2. Task 2: min/market_status_generator.rs
3. Task 3: min/signal_generator.rs
4. Task 4: min/price_control_generator.rs
5. Task 5: day/market_status_generator.rs
6. Task 6: day/signal_generator.rs
7. Task 7: day/price_control_generator.rs
8. Task 8: trading_trigger.rs
9. Task 9: 更新 lib.rs
10. Task 10: 编译验证
