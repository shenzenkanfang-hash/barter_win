# Pipeline Checkpoint 日志系统实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为交易系统添加 Pipeline Checkpoint 日志系统，使用 Pipeline 封装实现清晰的流程控制和调试日志。

**Architecture:**
- 使用 Pipeline 对象封装完整的交易流程
- CheckTable 作为数据存储在各 Processor 间传递
- CheckpointLogger trait 实现日志输出（观察者模式）
- 清晰的流程控制：任一环节失败则停止

**Tech Stack:** tracing（已有）, chrono（已有）, rust_decimal（已有）

---

## 文件结构

```
crates/engine/src/
├── pipeline.rs              # 【新建】Pipeline 封装 + Processor trait
├── checkpoint.rs           # 【新建】CheckpointLogger trait + StageResult
├── engine.rs              # 【修改】简化为持有 Pipeline
└── lib.rs                 # 【修改】导出新模块
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
        tracing::info!(stage = ?stage, symbol = symbol, "Pipeline stage started");
    }

    fn log_pass(&self, stage: Stage, symbol: &str, details: &str) {
        tracing::info!(stage = ?stage, symbol = symbol, details = details, "Pipeline stage passed");
    }

    fn log_blocked(&self, stage: Stage, symbol: &str, reason: &str) {
        tracing::warn!(stage = ?stage, symbol = symbol, reason = reason, "Pipeline stage blocked");
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
            tracing::warn!(
                symbol = symbol,
                stages = ?stages,
                blocked_at = ?blocked,
                "Pipeline blocked at stage"
            );
        } else {
            tracing::info!(
                symbol = symbol,
                stages = ?stages,
                "Pipeline completed successfully"
            );
        }
    }
}

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

- [ ] **Step 2: 更新 lib.rs 导出**

```rust
// 在 lib.rs 添加
pub mod checkpoint;
pub use checkpoint::{CheckpointLogger, Stage, StageResult};
```

- [ ] **Step 3: 编译验证**

```bash
cd D:\Rust项目\barter-rs-main
export PATH="/c/Users/char/.rustup/toolchains/stable-x86_64-pc-windows-msvc/bin:$PATH"
export RUSTC="/c/Users/char/.rustup/toolchains/stable-x86_64-pc-windows-msvc/bin/rustc.exe"
cargo check -p engine
```

- [ ] **Step 4: 提交**

```bash
git add crates/engine/src/checkpoint.rs crates/engine/src/lib.rs
git commit -m "feat(engine): 添加 Pipeline CheckpointLogger trait + StageResult"
```

---

## Task 2: 创建 Pipeline 封装

**Files:**
- Create: `crates/engine/src/pipeline.rs`
- Modify: `crates/engine/src/engine.rs`
- Modify: `crates/engine/src/lib.rs` (添加导出)

- [ ] **Step 1: 创建 pipeline.rs 文件**

```rust
use crate::checkpoint::{CheckpointLogger, Stage, StageResult};
use crate::check_table::CheckTable;
use crate::error::EngineError;
use market::types::Tick;
use rust_decimal::Decimal;
use strategy::types::{OrderRequest, Side, Signal};
use std::sync::Arc;

/// Pipeline Processor trait - 所有阶段处理器都实现这个接口
pub trait Processor: Send + Sync {
    /// 处理 tick，返回阶段结果
    fn process(&mut self, check_table: &mut CheckTable, tick: &Tick) -> StageResult;
}

/// Pipeline - 封装完整的交易流程
pub struct Pipeline {
    /// CheckTable 数据存储
    check_table: CheckTable,
    /// Checkpoint 日志记录器
    logger: Box<dyn CheckpointLogger>,
    /// 指标处理器
    indicator_processor: Box<dyn Processor>,
    /// 策略处理器
    strategy_processor: Box<dyn Processor>,
    /// 风控处理器
    risk_processor: Box<dyn Processor>,
    /// 当前品种
    symbol: String,
}

impl Pipeline {
    /// 创建 Pipeline
    pub fn new(
        symbol: String,
        logger: Box<dyn CheckpointLogger>,
        indicator_processor: Box<dyn Processor>,
        strategy_processor: Box<dyn Processor>,
        risk_processor: Box<dyn Processor>,
    ) -> Self {
        Self {
            check_table: CheckTable::new(),
            logger,
            indicator_processor,
            strategy_processor,
            risk_processor,
            symbol,
        }
    }

    /// 处理单个 Tick - 明确的流程控制
    pub fn process(&mut self, tick: &Tick) -> Option<OrderRequest> {
        let mut stage_results: Vec<StageResult> = Vec::new();
        let mut blocked_at: Option<Stage> = None;

        // 1. 指标计算阶段
        let indicator_result = self.indicator_processor.process(&mut self.check_table, tick);
        self.logger.log_pass(Stage::Indicator, &self.symbol, &indicator_result.details);
        stage_results.push(indicator_result.clone());

        if !indicator_result.passed {
            blocked_at = Some(Stage::Indicator);
            self.logger.log_blocked(Stage::Indicator, &self.symbol,
                indicator_result.blocked_reason.as_deref().unwrap_or("指标计算失败"));
            self.logger.log_checkpoint(&self.symbol, &stage_results, blocked_at);
            return None;
        }

        // 2. 策略判断阶段
        let strategy_result = self.strategy_processor.process(&mut self.check_table, tick);
        self.logger.log_pass(Stage::Strategy, &self.symbol, &strategy_result.details);
        stage_results.push(strategy_result.clone());

        if !strategy_result.passed {
            blocked_at = Some(Stage::Strategy);
            self.logger.log_blocked(Stage::Strategy, &self.symbol,
                strategy_result.blocked_reason.as_deref().unwrap_or("策略判断失败"));
            self.logger.log_checkpoint(&self.symbol, &stage_results, blocked_at);
            return None;
        }

        // 3. 风控预检阶段
        let risk_result = self.risk_processor.process(&mut self.check_table, tick);
        self.logger.log_pass(Stage::RiskPre, &self.symbol, &risk_result.details);
        stage_results.push(risk_result.clone());

        if !risk_result.passed {
            blocked_at = Some(Stage::RiskPre);
            self.logger.log_blocked(Stage::RiskPre, &self.symbol,
                risk_result.blocked_reason.as_deref().unwrap_or("风控预检失败"));
            self.logger.log_checkpoint(&self.symbol, &stage_results, blocked_at);
            return None;
        }

        // 4. 全部通过，获取交易决策
        self.logger.log_checkpoint(&self.symbol, &stage_results, None);
        self.build_order_request()
    }

    /// 从 CheckTable 构建订单请求
    fn build_order_request(&self) -> Option<OrderRequest> {
        let entry = self.check_table.get(&self.symbol, "main", "1m")?;

        if !matches!(entry.final_signal, Signal::LongEntry | Signal::ShortEntry) {
            return None;
        }

        Some(OrderRequest {
            symbol: self.symbol.clone(),
            side: match entry.final_signal {
                Signal::LongEntry | Signal::LongHedge => Side::Long,
                _ => Side::Short,
            },
            order_type: strategy::types::OrderType::Market,
            price: Some(entry.target_price),
            qty: entry.quantity,
        })
    }

    /// 获取 CheckTable 引用（只读）
    pub fn check_table(&self) -> &CheckTable {
        &self.check_table
    }
}

/// 默认的指标处理器（实际实现应调用真实的指标计算）
pub struct DefaultIndicatorProcessor {
    // 这里会集成真实的 EMA、RSI 等指标
}

impl DefaultIndicatorProcessor {
    pub fn new() -> Self {
        Self {}
    }
}

impl Processor for DefaultIndicatorProcessor {
    fn process(&self, check_table: &mut CheckTable, tick: &Tick) -> StageResult {
        // TODO: 调用真实的指标计算
        let details = format!(
            "EMA12={:.2} EMA26={:.2} RSI={:.2}",
            tick.price * dec!(0.99),  // 模拟值
            tick.price,
            dec!(50)
        );
        StageResult::pass(Stage::Indicator, details)
    }
}

/// 默认的策略处理器
pub struct DefaultStrategyProcessor;

impl DefaultStrategyProcessor {
    pub fn new() -> Self {
        Self {}
    }
}

impl Processor for DefaultStrategyProcessor {
    fn process(&self, check_table: &mut CheckTable, tick: &Tick) -> StageResult {
        // TODO: 调用真实的策略判断
        let details = format!("Signal=BUY confidence=80");
        StageResult::pass(Stage::Strategy, details)
    }
}

/// 默认的风控处理器
pub struct DefaultRiskProcessor;

impl DefaultRiskProcessor {
    pub fn new() -> Self {
        Self {}
    }
}

impl Processor for DefaultRiskProcessor {
    fn process(&self, check_table: &mut CheckTable, tick: &Tick) -> StageResult {
        // TODO: 调用真实的风控检查
        StageResult::pass(Stage::RiskPre, "OK")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_basic() {
        // 创建带 NoOp logger 的 Pipeline
        struct NoOpLogger;
        impl CheckpointLogger for NoOpLogger {
            fn log_start(&self, _: Stage, _: &str) {}
            fn log_pass(&self, _: Stage, _: &str, _: &str) {}
            fn log_blocked(&self, _: Stage, _: &str, _: &str) {}
            fn log_checkpoint(&self, _: &str, _: &[StageResult], _: Option<Stage>) {}
        }

        let mut pipeline = Pipeline::new(
            "BTCUSDT".to_string(),
            Box::new(NoOpLogger),
            Box::new(DefaultIndicatorProcessor::new()),
            Box::new(DefaultStrategyProcessor::new()),
            Box::new(DefaultRiskProcessor::new()),
        );

        let tick = Tick {
            symbol: "BTCUSDT".to_string(),
            price: dec!(50000),
            qty: dec!(1.0),
            timestamp: chrono::Utc::now(),
            kline_1m: None,
            kline_15m: None,
            kline_1d: None,
        };

        // Pipeline.process() 应该返回 None（因为没有真实实现）
        let result = pipeline.process(&tick);
        // TODO: 真实实现后应该返回 Some(OrderRequest)
        assert!(result.is_none());
    }
}
```

- [ ] **Step 2: 更新 lib.rs 导出**

```rust
// 添加
pub mod pipeline;
pub use pipeline::{Pipeline, Processor};
```

- [ ] **Step 3: 简化 engine.rs 使用 Pipeline**

在 `TradingEngine` 中持有 Pipeline：

```rust
pub struct TradingEngine {
    // 替换原来的多个组件为一个 Pipeline
    pipeline: Pipeline,
    // ...
}

impl TradingEngine {
    pub fn new(...) -> Self {
        // 创建 Pipeline
        let pipeline = Pipeline::new(
            symbol.clone(),
            Box::new(ConsoleCheckpointLogger::new()),
            Box::new(DefaultIndicatorProcessor::new()),
            Box::new(DefaultStrategyProcessor::new()),
            Box::new(DefaultRiskProcessor::new()),
        );

        Self {
            pipeline,
            // ... 其他必要字段
        }
    }

    pub async fn on_tick(&mut self, tick: &Tick) {
        // 委托给 Pipeline 处理
        if let Some(order) = self.pipeline.process(tick) {
            // 执行订单
            self.execute_order(order).await;
        }
    }
}
```

- [ ] **Step 4: 编译验证**

```bash
cargo check -p engine
```

- [ ] **Step 5: 提交**

```bash
git add crates/engine/src/pipeline.rs crates/engine/src/engine.rs crates/engine/src/lib.rs
git commit -m "feat(engine): 添加 Pipeline 封装，清晰的流程控制"
```

---

## Task 3: 验证编译和测试

- [ ] **Step 1: 运行 cargo check**

```bash
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
git commit -m "test(engine): 验证 Pipeline CheckpointLogger 集成"
```

---

## 验收标准

- [ ] `cargo check --all` 编译通过
- [ ] `cargo test -p engine` 测试通过
- [ ] 运行 `cargo run --release` 时能看到 checkpoint 日志
- [ ] Pipeline 任一环节失败时，日志清晰显示 `✘ [BLOCKED]`
- [ ] 日志格式统一，易于阅读
- [ ] 代码结构清晰：Pipeline 封装 + Processor trait

---

## 依赖

| 依赖 | 状态 |
|------|------|
| tracing | 已有 |
| chrono | 已有 |
| rust_decimal | 已有 |
