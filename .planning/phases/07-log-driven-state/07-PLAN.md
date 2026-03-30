---
Author: 软件架构师
Created: 2026-03-30
Stage: plan
Status: draft
Next: 开发者 (Wave 1 实现)
---

# Plan: 07-log-driven-monitoring

## 目标 (Goal)

设计并实现纯日志驱动的系统状态研判方案，替换当前 StateCenter 的详细状态输出。

- StateCenter 保留（EngineManager 复活检测用）
- 所有详细运行状态通过 JSON Lines 日志输出 (`./logs/trading.YYYYMMDD.jsonl`)
- 分层监控：数据层1h健康摘要 + 引擎层关键操作详细记录
- 单条日志写入 < 1ms，异步非阻塞

## 依赖 (depends_on)

- Phase 6 完成（协程自驱动架构已就绪）

## 自主性 (autonomous)

Wave 1 可独立实现，Wave 2 依赖 Wave 1 成果。

## 文件变更 (files_modified)

- `crates/a_common/src/logs/checkpoint.rs` — 扩展 CheckpointLogger trait + 新增 TradingLogEvent
- `crates/a_common/src/logs/mod.rs` — 导出新增类型
- `crates/a_common/src/lib.rs` — 导出新增类型
- `src/actors.rs` — RiskActor 日志插桩
- `src/components.rs` — ComponentHealth 集成
- `src/event_bus.rs` — TradingLogEvent 集成

---

## Wave 1: 基础设施（基础组件，可独立验证）

### Task 1: 扩展 CheckpointLogger trait

**read_first:**
- `crates/a_common/src/logs/checkpoint.rs` (CheckpointLogger trait 定义)
- `crates/a_common/src/logs/mod.rs` (模块导出)

**action:**
在 `crates/a_common/src/logs/checkpoint.rs` 中：

1. 在 `CheckpointLogger` trait 末尾添加新方法：
```rust
/// 记录通用事件
fn log_event(&self, component: &str, event: &str, symbol: Option<&str>, data: &str);
```

2. 在 `ConsoleCheckpointLogger` 实现中添加：
```rust
fn log_event(&self, component: &str, event: &str, symbol: Option<&str>, data: &str) {
    eprintln!("[{}] [{}] [{}] {} {:?}",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
        component,
        event,
        data,
        symbol
    );
}
```

3. 在 `TracingCheckpointLogger` 实现中添加（输出到 tracing，JSON Lines 由 WriterService 处理）：
```rust
fn log_event(&self, component: &str, event: &str, symbol: Option<&str>, data: &str) {
    tracing::info!(
        component = component,
        event = event,
        symbol = symbol,
        data = data,
        "trading_event"
    );
}
```

4. 在 `CompositeCheckpointLogger` 实现中添加：
```rust
fn log_event(&self, component: &str, event: &str, symbol: Option<&str>, data: &str) {
    for logger in &self.loggers {
        logger.log_event(component, event, symbol, data);
    }
}
```

**acceptance_criteria:**
- `grep -n "fn log_event" crates/a_common/src/logs/checkpoint.rs` 返回 4 处实现
- `grep -n "log_event" crates/a_common/src/logs/checkpoint.rs | wc -l` 返回 4
- `cargo check -p a_common` 无编译错误

---

### Task 2: 添加 ComponentHealth struct

**read_first:**
- `crates/a_common/src/logs/checkpoint.rs` (同模块，参考现有 struct 风格)
- `crates/a_common/src/logs/mod.rs` (模块导出)

**action:**
在 `crates/a_common/src/logs/checkpoint.rs` 文件末尾（在 `#[cfg(test)]` 之前）添加：

```rust
/// 组件健康状态（用于数据层/指标层1小时间隔健康摘要）
#[derive(Debug, Clone, Default)]
pub struct ComponentHealth {
    /// 最后处理K线时间戳（毫秒，Unix epoch）
    pub last_tick_timestamp_ms: i64,
    /// 已处理K线数
    pub processed_kline_count: u64,
    /// 上次计算延迟（毫秒）
    pub last_compute_latency_ms: u64,
    /// 累计错误数
    pub error_count: u32,
}

impl ComponentHealth {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update_tick(&mut self, timestamp_ms: i64, latency_ms: u64) {
        self.last_tick_timestamp_ms = timestamp_ms;
        self.last_compute_latency_ms = latency_ms;
        self.processed_kline_count += 1;
    }

    pub fn add_error(&mut self) {
        self.error_count += 1;
    }
}
```

**acceptance_criteria:**
- `grep -n "struct ComponentHealth" crates/a_common/src/logs/checkpoint.rs` 找到定义
- `grep -n "pub struct ComponentHealth" crates/a_common/src/logs/checkpoint.rs` 确认 pub
- `cargo check -p a_common` 无编译错误

---

### Task 3: 实现 ComponentHealthLogger（定时输出 health.summary）

**read_first:**
- `crates/a_common/src/logs/checkpoint.rs` (CheckpointLogger trait)
- `crates/a_common/src/logs/mod.rs` (导出格式)

**action:**
在 `crates/a_common/src/logs/checkpoint.rs` 中，`#[cfg(test)]` 之前添加：

```rust
use std::sync::atomic::{AtomicU64, AtomicI64, AtomicU32, Ordering};
use std::sync::Arc;

/// Thread-safe 健康状态累加器（无锁设计）
pub struct HealthAccumulator {
    last_tick_timestamp_ms: AtomicI64,
    processed_kline_count: AtomicU64,
    last_compute_latency_ms: AtomicU64,
    error_count: AtomicU32,
}

impl HealthAccumulator {
    pub fn new() -> Self {
        Self {
            last_tick_timestamp_ms: AtomicI64::new(0),
            processed_kline_count: AtomicU64::new(0),
            last_compute_latency_ms: AtomicU64::new(0),
            error_count: AtomicU32::new(0),
        }
    }

    pub fn update_tick(&self, timestamp_ms: i64, latency_ms: u64) {
        self.last_tick_timestamp_ms.store(timestamp_ms, Ordering::SeqCst);
        self.last_compute_latency_ms.store(latency_ms, Ordering::SeqCst);
        self.processed_kline_count.fetch_add(1, Ordering::SeqCst);
    }

    pub fn add_error(&self) {
        self.error_count.fetch_add(1, Ordering::SeqCst);
    }

    pub fn snapshot(&self) -> ComponentHealth {
        ComponentHealth {
            last_tick_timestamp_ms: self.last_tick_timestamp_ms.load(Ordering::SeqCst),
            processed_kline_count: self.processed_kline_count.load(Ordering::SeqCst),
            last_compute_latency_ms: self.last_compute_latency_ms.load(Ordering::SeqCst),
            error_count: self.error_count.load(Ordering::SeqCst),
        }
    }
}

impl Default for HealthAccumulator {
    fn default() -> Self {
        Self::new()
    }
}

/// ComponentHealthLogger - 定时输出 health.summary 日志
///
/// 每小时输出一次组件健康摘要，使用 tokio 定时器。
/// 输出格式：JSON Lines，字段对齐 ComponentHealth。
pub struct ComponentHealthLogger {
    component: String,
    accumulator: Arc<HealthAccumulator>,
    interval_secs: u64,
}

impl ComponentHealthLogger {
    pub fn new(component: &str, interval_secs: u64) -> Self {
        Self {
            component: component.to_string(),
            accumulator: Arc::new(HealthAccumulator::new()),
            interval_secs,
        }
    }

    pub fn accumulator(&self) -> Arc<HealthAccumulator> {
        Arc::clone(&self.accumulator)
    }

    /// 启动后台定时日志任务（tokio::spawn）
    pub fn start_background_logger(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(self.interval_secs));
            loop {
                interval.tick().await;
                let health = self.accumulator.snapshot();
                tracing::info!(
                    component = %self.component,
                    event = "health.summary",
                    last_tick_ms = health.last_tick_timestamp_ms,
                    processed = health.processed_kline_count,
                    latency_ms = health.last_compute_latency_ms,
                    errors = health.error_count,
                    "health_summary"
                );
            }
        })
    }
}
```

同时在 `mod.rs` 导出：
```rust
pub use checkpoint::{
    CheckpointLogger, CompositeCheckpointLogger, ConsoleCheckpointLogger, Stage, StageResult,
    TracingCheckpointLogger, ComponentHealth, HealthAccumulator, ComponentHealthLogger,
};
```

**acceptance_criteria:**
- `grep -n "struct ComponentHealthLogger" crates/a_common/src/logs/checkpoint.rs` 找到
- `grep -n "struct HealthAccumulator" crates/a_common/src/logs/checkpoint.rs` 找到
- `grep -n "ComponentHealthLogger" crates/a_common/src/logs/mod.rs` 确认导出
- `cargo check -p a_common` 无编译错误

---

### Task 4: 实现 JSON Lines WriterService（非阻塞异步）

**read_first:**
- `crates/a_common/src/logs/checkpoint.rs` (现有 TracingCheckpointLogger 参考)
- 项目根目录 `Cargo.toml` (依赖检查)

**action:**
新建文件 `crates/a_common/src/logs/writer.rs`：

```rust
//! JSON Lines 日志写入服务
//!
//! 使用 tracing_subscriber + nonblocking Writer 实现异步非阻塞日志写入。

use std::path::PathBuf;
use std::fs::{OpenOptions, File};
use std::io::BufWriter;
use std::sync::OnceLock;
use chrono::Local;
use tokio::sync::mpsc;
use tokio::io::AsyncWriteExt;

/// 日志文件目录
static LOG_DIR: OnceLock<PathBuf> = OnceLock::new();

/// 初始化日志目录
pub fn init_log_dir(dir: PathBuf) {
    LOG_DIR.set(dir).ok();
}

/// 获取今日日志文件路径
fn today_log_path() -> PathBuf {
    let dir = LOG_DIR.get_or_init(PathBuf::from("./logs"));
    std::fs::create_dir_all(dir).ok();
    let today = Local::now().format("%Y%m%d");
    dir.join(format!("trading.{}.jsonl", today))
}

/// JSON Lines Writer Service（后台任务）
///
/// 接收 JSON 字符串，写入今日日志文件。
/// 使用 tokio 异步文件 I/O，非阻塞。
pub struct JsonLinesWriter {
    tx: mpsc::Sender<String>,
}

impl JsonLinesWriter {
    /// 创建新的 Writer Service
    pub fn new() -> Self {
        let (tx, mut rx) = mpsc::channel::<String>(10000);
        let path = today_log_path();

        // 后台写入任务
        tokio::spawn(async move {
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .await
                .expect("failed to open log file");

            let mut writer = BufWriter::new(file);
            let mut file = writer.into_inner();

            while let Some(line) = rx.recv().await {
                if let Err(e) = file.write_all(line.as_bytes()).await {
                    tracing::error!("failed to write log: {}", e);
                }
                if let Err(e) = file.write_all(b"\n").await {
                    tracing::error!("failed to write newline: {}", e);
                }
            }
        });

        Self { tx }
    }

    /// 异步发送日志行（< 1ms，非阻塞）
    pub async fn write(&self, line: String) {
        // 使用 try_send 而非 send，超限时丢弃而非阻塞
        let _ = self.tx.try_send(line);
    }
}

impl Default for JsonLinesWriter {
    fn default() -> Self {
        Self::new()
    }
}

/// tracing layer，把事件桥接到 JsonLinesWriter
pub fn json_lines_layer() -> impl tracing_subscriber::Layer<tracing_subscriber::Registry> {
    use tracing_subscriber::fmt::format::FmtSpan;
    use tracing_subscriber::prelude::*;

    let writer = JsonLinesWriter::new();

    tracing_subscriber::fmt::layer()
        .with_writer(std::sync::Mutex::new(writer))
        .with_span_events(FmtSpan::CLOSE)
        .with_ansi(false)
        .json()
}
```

在 `crates/a_common/src/logs/mod.rs` 添加：
```rust
pub mod writer;
pub use writer::{JsonLinesWriter, init_log_dir, json_lines_layer};
```

**acceptance_criteria:**
- `grep -n "JsonLinesWriter" crates/a_common/src/logs/writer.rs` 找到
- `grep -n "nonblocking\|BufWriter\|mpsc::channel" crates/a_common/src/logs/writer.rs` 确认异步实现
- `cargo check -p a_common` 无编译错误

---

### Task 5: 创建 TradingLogEvent 枚举

**read_first:**
- `crates/a_common/src/logs/checkpoint.rs` (Stage 枚举风格参考)
- `crates/a_common/src/logs/mod.rs` (导出)

**action:**
在 `crates/a_common/src/logs/checkpoint.rs` 中，`#[cfg(test)]` 之前添加：

```rust
/// 交易系统日志事件类型
///
/// 对齐 07-CONTEXT.md 中定义的所有事件类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradingLogEventType {
    // 组件生命周期
    ComponentStarted,
    ComponentStopped,
    HealthSummary,

    // 数据层
    DataReceived,
    IndicatorComputed,

    // 策略层
    StrategySignal,

    // 风控层
    RiskCheck,
    RiskCheckSkipped,

    // 交易执行
    TradeLockAcquired,
    TradeLockSkipped,
    OrderSubmitted,
    OrderFilled,
    OrderRejected,
    OrderCancelled,

    // 仓位
    PositionOpened,
    PositionClosed,

    // 异常
    StaleDetected,
    Error,
}

impl std::fmt::Display for TradingLogEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TradingLogEventType::ComponentStarted => write!(f, "component.started"),
            TradingLogEventType::ComponentStopped => write!(f, "component.stopped"),
            TradingLogEventType::HealthSummary => write!(f, "health.summary"),
            TradingLogEventType::DataReceived => write!(f, "data.received"),
            TradingLogEventType::IndicatorComputed => write!(f, "indicator.computed"),
            TradingLogEventType::StrategySignal => write!(f, "strategy.signal"),
            TradingLogEventType::RiskCheck => write!(f, "risk.check"),
            TradingLogEventType::RiskCheckSkipped => write!(f, "risk.check.skipped"),
            TradingLogEventType::TradeLockAcquired => write!(f, "trade.lock.acquired"),
            TradingLogEventType::TradeLockSkipped => write!(f, "trade.lock.skipped"),
            TradingLogEventType::OrderSubmitted => write!(f, "order.submitted"),
            TradingLogEventType::OrderFilled => write!(f, "order.filled"),
            TradingLogEventType::OrderRejected => write!(f, "order.rejected"),
            TradingLogEventType::OrderCancelled => write!(f, "order.cancelled"),
            TradingLogEventType::PositionOpened => write!(f, "position.opened"),
            TradingLogEventType::PositionClosed => write!(f, "position.closed"),
            TradingLogEventType::StaleDetected => write!(f, "stale.detected"),
            TradingLogEventType::Error => write!(f, "error"),
        }
    }
}
```

在 `mod.rs` 导出：
```rust
pub use checkpoint::{
    CheckpointLogger, CompositeCheckpointLogger, ConsoleCheckpointLogger, Stage, StageResult,
    TracingCheckpointLogger, ComponentHealth, HealthAccumulator, ComponentHealthLogger,
    TradingLogEventType,
};
```

**acceptance_criteria:**
- `grep -n "TradingLogEventType" crates/a_common/src/logs/checkpoint.rs` 找到枚举定义
- `grep -n "ComponentStarted\|ComponentStopped\|HealthSummary\|DataReceived\|IndicatorComputed\|StrategySignal\|RiskCheck\|RiskCheckSkipped\|TradeLockAcquired\|TradeLockSkipped\|OrderSubmitted\|OrderFilled\|OrderRejected\|OrderCancelled\|PositionOpened\|PositionClosed\|StaleDetected\|Error" crates/a_common/src/logs/checkpoint.rs` 返回 18 行（每个事件类型一行 match arm）

---

## Wave 2: 组件集成（依赖 Wave 1）

### Task 6: 集成 ComponentHealth 到 H15mStrategyService

**read_first:**
- `src/components.rs` (SystemComponents 结构体)
- `src/actors.rs` (run_strategy_actor 函数)

**action:**
在 `src/components.rs` 的 `SystemComponents` 中添加 `ComponentHealthLogger`：

1. 添加字段：
```rust
pub health_logger: Arc<ComponentHealthLogger>,
```

2. 在 `SystemComponents::new()` 中初始化：
```rust
let health_logger = Arc::new(ComponentHealthLogger::new("h15m_strategy", 3600));
health_logger.start_background_logger();
```

3. 在 `actors.rs` 的 `run_strategy_actor` 中使用 accumulator：
- 在拉取数据成功后调用 `accumulator.update_tick(kline.open_time_ms, elapsed_ms)`
- 在解析失败时调用 `accumulator.add_error()`

**acceptance_criteria:**
- `grep -n "health_logger" src/components.rs` 找到字段和初始化
- `grep -n "accumulator" src/actors.rs` 找到使用点
- `grep -n "update_tick\|add_error" src/actors.rs` 确认插桩

---

### Task 7: 集成 TradingLogEvent 到 RiskService

**read_first:**
- `src/actors.rs` (RiskActor run_risk_actor 函数)
- `crates/a_common/src/logs/checkpoint.rs` (TradingLogEventType)

**action:**
在 `src/actors.rs` 的 `run_risk_actor` 中添加日志：

1. 在 pre_check 成功后记录 `risk.check` 事件：
```rust
tracing::info!(
    component = "risk",
    event = "risk.check",
    symbol = SYMBOL,
    order_id = %order_id,
    balance_passed = balance_passed,
    order_passed = order_passed,
    "risk_check"
);
```

2. 在 pre_check 失败时记录 `risk.check.skipped`：
```rust
tracing::info!(
    component = "risk",
    event = "risk.check.skipped",
    symbol = SYMBOL,
    order_id = %order_id,
    reason = %format!("balance={} order={}", balance_passed, order_passed),
    "risk_check_skipped"
);
```

3. 订单事件（submitted/filled/rejected/cancelled）参照 Context 定义。

**acceptance_criteria:**
- `grep -n "risk.check\|risk.check.skipped" src/actors.rs` 返回至少 2 处
- `grep -n "order.submitted\|order.filled\|order.rejected\|order.cancelled" src/actors.rs` 返回至少 4 处

---

### Task 8: 集成 TradingLogEvent 到 PipelineBus（strategy.signal）

**read_first:**
- `src/event_bus.rs` (PipelineBus 定义)

**action:**
在 `src/event_bus.rs` 的 `PipelineBus` 结构体中添加日志记录器，或者在 `actors.rs` 中发送信号时记录。

在 `run_strategy_actor` 的信号发送处添加：
```rust
tracing::info!(
    component = "strategy",
    event = "strategy.signal",
    symbol = %signal.symbol,
    tick_id = signal.tick_id,
    decision = ?signal.decision,
    qty = signal.qty,
    reason = %signal.reason,
    "strategy_signal"
);
```

**acceptance_criteria:**
- `grep -n "strategy.signal" src/actors.rs` 找到日志语句
- `grep -n "strategy_signal" src/actors.rs` 确认 span 名称

---

### Task 9: 移除/禁用 a_common::heartbeat::Reporter 详细输出

**read_first:**
- `crates/a_common/src/heartbeat/mod.rs` 或相关文件（heartbeat 模块）
- `.planning/phases/07-log-driven-state/07-CONTEXT.md` (确认决策)

**action:**
根据 Context 决策，将 `heartbeat_report.json` 输出降级或禁用。找到 Reporter 的详细状态输出代码，注释掉或添加 feature gate。

**acceptance_criteria:**
- `grep -rn "heartbeat_report.json" crates/` 返回注释或移除的代码
- `cargo check -p a_common` 无编译错误

---

## 验证标准 (Verification)

1. `cargo check -p a_common` 无编译错误
2. `cargo check` 全项目无编译错误
3. 所有新增 pub 类型已在 `crates/a_common/src/lib.rs` 导出
4. JSON Lines 格式可解析：`{"timestamp":"...","component":"...","event":"...","data":{...}}`
5. `health.summary` 事件每小时仅输出一次（由 ComponentHealthLogger 定时器控制）

## Must-Haves（目标反向验证）

- [ ] CheckpointLogger trait 扩展了 `log_event()` 方法（Task 1）
- [ ] ComponentHealth struct 包含 last_tick_timestamp_ms/processed_kline_count/last_compute_latency_ms/error_count 字段（Task 2）
- [ ] ComponentHealthLogger 定时器间隔可配置，默认 3600 秒（Task 3）
- [ ] JsonLinesWriter 使用 mpsc channel 非阻塞发送（Task 4）
- [ ] TradingLogEventType 枚举包含全部 18 种事件类型（Task 5）
- [ ] StrategyActor 集成了 health_logger accumulator（Task 6）
- [ ] RiskActor 记录 risk.check/risk.check.skipped/order.filled/order.rejected 事件（Task 7）
- [ ] PipelineBus 信号发送时记录 strategy.signal 事件（Task 8）
- [ ] heartbeat_report.json 详细输出已降级（Task 9）
