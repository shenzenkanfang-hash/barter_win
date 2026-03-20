# Pipeline Checkpoint 日志系统实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为交易系统添加 Pipeline Checkpoint 日志系统，实时可视化数据流转过程，便于调试。

**Architecture:**
- 定义 `CheckpointLogger` trait + `StageResult` 结构
- 实现 `ConsoleCheckpointLogger`（终端彩色）+ `TracingCheckpointLogger`（结构化）
- 在 `TradingEngine::on_tick()` 中集成 checkpoint 调用
- Pipeline 任一环节失败时清晰显示 BLOCKED

**Tech Stack:** tracing（已有）, chrono（已有）

---

## 文件结构

```
crates/engine/src/
├── checkpoint.rs          # 【新建】CheckpointLogger trait + StageResult + 实现
├── engine.rs             # 【修改】集成 CheckpointLogger
└── lib.rs               # 【修改】导出 checkpoint 模块
```

---

## Task 1: 定义 CheckpointLogger trait + StageResult

**Files:**
- Create: `crates/engine/src/checkpoint.rs`
- Modify: `crates/engine/src/lib.rs` (添加导出)
- Test: `crates/engine/src/checkpoint.rs` (添加 #[cfg(test)] 模块)

- [ ] **Step 1: 创建 checkpoint.rs 文件**

```rust
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;

/// Pipeline 环节
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// 指标计算
    Indicator,
    /// 策略判断
    Strategy,
    /// 风控预检
    RiskPre,
    /// 风控复核
    RiskRe,
    /// 订单执行
    Order,
}

impl std::fmt::Display for Stage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Stage::Indicator => write!(f, "指标"),
            Stage::Strategy => write!(f, "策略"),
            Stage::RiskPre => write!(f, "风控预检"),
            Stage::RiskRe => write!(f, "风控复核"),
            Stage::Order => write!(f, "订单"),
        }
    }
}

/// 单个环节的结果
#[derive(Debug, Clone)]
pub struct StageResult {
    /// 环节
    pub stage: Stage,
    /// 是否通过
    pub passed: bool,
    /// 详细信息
    pub details: String,
    /// 失败原因（如果有）
    pub blocked_reason: Option<String>,
}

impl StageResult {
    /// 通过的结果
    pub fn pass(stage: Stage, details: impl Into<String>) -> Self {
        Self {
            stage,
            passed: true,
            details: details.into(),
            blocked_reason: None,
        }
    }

    /// 失败的结果
    pub fn fail(stage: Stage, reason: impl Into<String>) -> Self {
        let reason_str = reason.into();
        Self {
            stage,
            passed: false,
            details: String::new(),
            blocked_reason: Some(reason_str),
        }
    }
}

/// Pipeline 各环节的 checkpoint 日志记录器
pub trait CheckpointLogger: Send + Sync {
    /// 记录环节开始
    fn log_start(&self, stage: Stage, symbol: &str);

    /// 记录环节完成（通过）
    fn log_pass(&self, stage: Stage, symbol: &str, details: &str);

    /// 记录环节失败（Pipeline 停止）
    fn log_blocked(&self, stage: Stage, symbol: &str, reason: &str);

    /// 记录完整 checkpoint（所有环节结果）
    fn log_checkpoint(&self, symbol: &str, results: &[StageResult], blocked_at: Option<Stage>);
}
```

- [ ] **Step 2: 添加 ConsoleCheckpointLogger 实现**

```rust
/// 控制台彩色输出 CheckpointLogger
pub struct ConsoleCheckpointLogger;

impl ConsoleCheckpointLogger {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ConsoleCheckpointLogger {
    fn default() -> Self {
        Self::new()
    }
}

impl CheckpointLogger for ConsoleCheckpointLogger {
    fn log_start(&self, stage: Stage, symbol: &str) {
        eprintln!("[{}] [{}] [▶ {} 开始]", chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"), symbol, stage);
    }

    fn log_pass(&self, stage: Stage, symbol: &str, details: &str) {
        eprintln!("[{}] [{}] [✔ {}] {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"), symbol, stage, details);
    }

    fn log_blocked(&self, stage: Stage, symbol: &str, reason: &str) {
        eprintln!("[{}] [{}] [✘ {}] {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"), symbol, stage, reason);
    }

    fn log_checkpoint(&self, symbol: &str, results: &[StageResult], blocked_at: Option<Stage>) {
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        eprintln!("[{}] [{}] [CHECKPOINT]", timestamp, symbol);

        for result in results {
            if result.passed {
                eprintln!("  ├─ [✔ {}] {}", result.stage, result.details);
            } else {
                eprintln!("  ├─ [✘ {}] {}", result.stage, result.blocked_reason.as_ref().unwrap_or(&result.details));
                break;
            }
        }

        if let Some(blocked) = blocked_at {
            eprintln!("  └─ [BLOCKED] 数据停止传递 - 原因: {}", blocked);
        } else {
            eprintln!("  └─ [COMPLETE] Pipeline 完成");
        }
    }
}
```

- [ ] **Step 3: 添加 TracingCheckpointLogger 实现**

```rust
use tracing::{info, warn, error};

/// 基于 tracing 的结构化日志 CheckpointLogger
pub struct TracingCheckpointLogger;

impl TracingCheckpointLogger {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TracingCheckpointLogger {
    fn default() -> Self {
        Self::new()
    }
}

impl CheckpointLogger for TracingCheckpointLogger {
    fn log_start(&self, stage: Stage, symbol: &str) {
        info!(stage = ?stage, symbol = symbol, "Pipeline stage started");
    }

    fn log_pass(&self, stage: Stage, symbol: &str, details: &str) {
        info!(stage = ?stage, symbol = symbol, details = details, "Pipeline stage passed");
    }

    fn log_blocked(&self, stage: Stage, symbol: &str, reason: &str) {
        warn!(stage = ?stage, symbol = symbol, reason = reason, "Pipeline stage blocked");
    }

    fn log_checkpoint(&self, symbol: &str, results: &[StageResult], blocked_at: Option<Stage>) {
        let stages: Vec<_> = results.iter().map(|r| {
            if r.passed {
                format!("{:?}=OK", r.stage)
            } else {
                format!("{:?}=BLOCKED", r.stage)
            }
        }).collect();

        if let Some(blocked) = blocked_at {
            warn!(
                symbol = symbol,
                stages = ?stages,
                blocked_at = ?blocked,
                "Pipeline blocked at stage"
            );
        } else {
            info!(
                symbol = symbol,
                stages = ?stages,
                "Pipeline completed successfully"
            );
        }
    }
}
```

- [ ] **Step 4: 添加复合 Logger 实现**

```rust
/// 组合多个 Logger
pub struct CompositeCheckpointLogger {
    loggers: Vec<Box<dyn CheckpointLogger>>,
}

impl CompositeCheckpointLogger {
    pub fn new() -> Self {
        Self { loggers: Vec::new() }
    }

    pub fn add<L: CheckpointLogger + 'static>(mut self, logger: L) -> Self {
        self.loggers.push(Box::new(logger));
        self
    }
}

impl Default for CompositeCheckpointLogger {
    fn default() -> Self {
        Self::new()
    }
}

impl CheckpointLogger for CompositeCheckpointLogger {
    fn log_start(&self, stage: Stage, symbol: &str) {
        for logger in &self.loggers {
            logger.log_start(stage, symbol);
        }
    }

    fn log_pass(&self, stage: Stage, symbol: &str, details: &str) {
        for logger in &self.loggers {
            logger.log_pass(stage, symbol, details);
        }
    }

    fn log_blocked(&self, stage: Stage, symbol: &str, reason: &str) {
        for logger in &self.loggers {
            logger.log_blocked(stage, symbol, reason);
        }
    }

    fn log_checkpoint(&self, symbol: &str, results: &[StageResult], blocked_at: Option<Stage>) {
        for logger in &self.loggers {
            logger.log_checkpoint(symbol, results, blocked_at);
        }
    }
}
```

- [ ] **Step 5: 添加单元测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stage_result_pass() {
        let result = StageResult::pass(Stage::Indicator, "EMA12=100");
        assert!(result.passed);
        assert_eq!(result.details, "EMA12=100");
        assert!(result.blocked_reason.is_none());
    }

    #[test]
    fn test_stage_result_fail() {
        let result = StageResult::fail(Stage::Strategy, "TR_RATIO < 1");
        assert!(!result.passed);
        assert!(result.blocked_reason.is_some());
        assert_eq!(result.blocked_reason.unwrap(), "TR_RATIO < 1");
    }

    #[test]
    fn test_console_logger() {
        let logger = ConsoleCheckpointLogger::new();
        logger.log_start(Stage::Indicator, "BTCUSDT");
        logger.log_pass(Stage::Indicator, "BTCUSDT", "EMA12=100 RSI=50");
        logger.log_blocked(Stage::Strategy, "BTCUSDT", "TR_RATIO < 1");
    }

    #[test]
    fn test_composite_logger() {
        let logger = CompositeCheckpointLogger::new()
            .add(ConsoleCheckpointLogger::new())
            .add(TracingCheckpointLogger::new());

        logger.log_pass(Stage::Indicator, "BTCUSDT", "test");
    }
}
```

- [ ] **Step 6: 更新 lib.rs 导出**

```rust
// 在 lib.rs 添加
pub mod checkpoint;
pub use checkpoint::{CheckpointLogger, Stage, StageResult};
```

- [ ] **Step 7: 提交**

```bash
git add crates/engine/src/checkpoint.rs crates/engine/src/lib.rs
git commit -m "feat(engine): 添加 Pipeline CheckpointLogger trait + 实现"
```

---

## Task 2: 在 TradingEngine 中集成 CheckpointLogger

**Files:**
- Modify: `crates/engine/src/engine.rs`

- [ ] **Step 1: 添加 checkpoint_logger 字段到 TradingEngine**

在 `TradingEngine` 结构体中添加:
```rust
use crate::checkpoint::{CheckpointLogger, ConsoleCheckpointLogger, Stage, StageResult};

pub struct TradingEngine {
    // ... 现有字段 ...

    // Checkpoint 日志记录器
    checkpoint_logger: Box<dyn CheckpointLogger>,
}
```

在 `TradingEngine::new()` 中添加初始化:
```rust
Self {
    // ... 现有字段初始化 ...
    checkpoint_logger: Box::new(ConsoleCheckpointLogger::new()),
}
```

- [ ] **Step 2: 在 on_tick 中集成 checkpoint 调用**

修改 `on_tick` 方法:

```rust
pub async fn on_tick(&mut self, tick: &Tick) {
    self.current_ts = tick.timestamp.timestamp();
    self.current_price = tick.price;

    // 收集所有阶段结果
    let mut stage_results: Vec<StageResult> = Vec::new();
    let mut blocked_at: Option<Stage> = None;

    // 1. 更新 K线
    let completed_1m = self.kline_1m.update(tick);
    let completed_1d = self.kline_1d.update(tick);

    // 2. 更新指标
    let indicator_result = self.update_indicators(tick.price);
    stage_results.push(indicator_result.clone());

    if !indicator_result.passed {
        blocked_at = Some(Stage::Indicator);
        self.checkpoint_logger.log_checkpoint(&self.symbol, &stage_results, blocked_at);
        return;
    }

    // 3. 风控预检 (锁外)
    let risk_pre_result = self.pre_trade_check(tick);
    stage_results.push(risk_pre_result.clone());

    if !risk_pre_result.passed {
        blocked_at = Some(Stage::RiskPre);
        self.checkpoint_logger.log_checkpoint(&self.symbol, &stage_results, blocked_at);
        return;
    }

    // 4. 如果有完成的 K线，生成信号
    if let Some(kline) = completed_1m {
        let signal_result = self.on_kline_completed(&kline);
        stage_results.push(signal_result.clone());

        if !signal_result.passed {
            blocked_at = Some(Stage::Strategy);
            self.checkpoint_logger.log_checkpoint(&self.symbol, &stage_results, blocked_at);
            return;
        }
    }

    // 5. 日线 K线完成处理
    if let Some(kline) = completed_1d {
        self.on_daily_kline_completed(&kline);
    }

    // 6. 打印状态（原有日志保留）
    self.print_status(tick);

    // 7. 记录完整 checkpoint
    self.checkpoint_logger.log_checkpoint(&self.symbol, &stage_results, None);
}
```

- [ ] **Step 3: 修改 update_indicators 返回 StageResult**

```rust
fn update_indicators(&mut self, price: Decimal) -> StageResult {
    // 更新 EMA
    let ema_f = self.ema_fast.update(price);
    let ema_s = self.ema_slow.update(price);

    // 更新 RSI
    let rsi_value = self.rsi.update(ema_f - ema_s);

    // 构建详情字符串
    let details = format!(
        "EMA12={} EMA26={} RSI={:.2}",
        ema_f.round_dp(2),
        ema_s.round_dp(2),
        rsi_value.round_dp(2)
    );

    StageResult::pass(Stage::Indicator, details)
}
```

- [ ] **Step 4: 修改 pre_trade_check 返回 StageResult**

```rust
fn pre_trade_check(&self, tick: &Tick) -> StageResult {
    let order_value = tick.price * tick.qty;

    // 检查账户是否可以交易
    if !self.account_pool.can_trade(order_value) {
        return StageResult::fail(Stage::RiskPre, "账户余额不足或熔断中");
    }

    // 检查策略是否可以开仓
    if !self.strategy_pool.can_open_position("main", order_value) {
        return StageResult::fail(Stage::RiskPre, "策略资金池余额不足");
    }

    StageResult::pass(Stage::RiskPre, "OK")
}
```

- [ ] **Step 5: 修改 on_kline_completed 返回 StageResult**

```rust
fn on_kline_completed(&mut self, kline: &market::types::KLine) -> StageResult {
    // 构建信号详情
    let details = format!(
        "K线完成 close={} high={} low={}",
        kline.close, kline.high, kline.low
    );

    StageResult::pass(Stage::Strategy, details)
}
```

- [ ] **Step 6: 提交**

```bash
git add crates/engine/src/engine.rs
git commit -m "feat(engine): 集成 CheckpointLogger 到 on_tick 流程"
```

---

## Task 3: 验证编译和测试

- [ ] **Step 1: 运行 cargo check**

```bash
cd D:\Rust项目\barter-rs-main
export PATH="/c/Users/char/.rustup/toolchains/stable-x86_64-pc-windows-msvc/bin:$PATH"
export RUSTC="/c/Users/char/.rustup/toolchains/stable-x86_64-pc-windows-msvc/bin/rustc.exe"
cargo check --all
```

预期: 编译通过，无 error

- [ ] **Step 2: 运行测试**

```bash
cargo test -p engine
```

预期: 所有测试通过

- [ ] **Step 3: 运行 data-printer 观察日志**

```bash
cargo run --bin data-printer --release
```

预期: 终端输出包含 `[CHECKPOINT]` 格式的日志

- [ ] **Step 4: 提交**

```bash
git add -A
git commit -m "test(engine): 验证 CheckpointLogger 集成"
```

---

## 验收标准

- [ ] `cargo check --all` 编译通过
- [ ] `cargo test -p engine` 测试通过
- [ ] 运行 `cargo run --release` 时能看到 checkpoint 日志
- [ ] Pipeline 任一环节失败时，日志清晰显示 `✘ [BLOCKED]`
- [ ] 日志格式统一，易于阅读

---

## 依赖

| 依赖 | 状态 |
|------|------|
| tracing | 已有 |
| chrono | 已有 |
| rust_decimal | 已有 |
