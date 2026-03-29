# Heartbeat Reporter 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 a_common 中实现心跳报告器，支持通过心跳标记追踪事件流连通性

**Architecture:** 基于 tokio::sync::RwLock + AtomicU64 的异步安全心跳系统，支持 Full/Sampling/Disabled 三种降级模式

**Tech Stack:** Rust, tokio, chrono, once_cell, parking_lot

---

## 文件结构

```
crates/a_common/src/heartbeat/
├── mod.rs          # 模块入口，导出所有类型和宏
├── clock.rs        # HeartbeatClock - 心跳时钟，原子序号生成
├── token.rs        # HeartbeatToken - 心跳标记，携带序号和时间
├── entry.rs        # ReportEntry - 报到条目
├── config.rs       # ReporterConfig - 报告器配置
├── mode.rs         # ReportMode - 降级模式枚举
├── points.rs       # TEST_POINT_NAMES - 测试点名称映射
├── macros.rs       # heartbeat! 宏定义
└── reporter.rs     # HeartbeatReporter - 核心报告器

crates/a_common/Cargo.toml  # 添加 once_cell 依赖

crates/a_common/tests/
└── heartbeat_test.rs  # 单元测试
```

---

## 依赖配置

**修改:** `crates/a_common/Cargo.toml`

在 `[dependencies]` 中添加:
```toml
once_cell = "1.19"
```

---

## Task 1: 创建 heartbeat 模块骨架

**Files:**
- Create: `crates/a_common/src/heartbeat/mod.rs`
- Create: `crates/a_common/src/heartbeat/clock.rs`
- Create: `crates/a_common/src/heartbeat/token.rs`
- Create: `crates/a_common/src/heartbeat/entry.rs`
- Create: `crates/a_common/src/heartbeat/config.rs`
- Create: `crates/a_common/src/heartbeat/mode.rs`
- Create: `crates/a_common/src/heartbeat/points.rs`
- Create: `crates/a_common/src/heartbeat/macros.rs`
- Create: `crates/a_common/src/heartbeat/reporter.rs`
- Modify: `crates/a_common/src/lib.rs` (添加 `pub mod heartbeat;`)
- Modify: `crates/a_common/src/lib.rs` (添加 re-export)
- Modify: `crates/a_common/Cargo.toml` (添加 once_cell)

- [ ] **Step 1: 创建目录结构**

```bash
mkdir -p crates/a_common/src/heartbeat
mkdir -p crates/a_common/tests
```

- [ ] **Step 2: 添加 once_cell 依赖到 Cargo.toml**

修改 `crates/a_common/Cargo.toml`，在 `[dependencies]` 部分添加:
```toml
once_cell = "1.19"
```

- [ ] **Step 3: 创建 clock.rs - 心跳时钟**

```rust
// crates/a_common/src/heartbeat/clock.rs
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// 心跳时钟 - 负责生成单调递增的心跳序号
pub struct HeartbeatClock {
    sequence: AtomicU64,
    started_at: Instant,
}

impl HeartbeatClock {
    pub fn new() -> Self {
        Self {
            sequence: AtomicU64::new(0),
            started_at: Instant::now(),
        }
    }

    /// 生成下一个心跳序号
    pub fn next_sequence(&self) -> u64 {
        self.sequence.fetch_add(1, Ordering::SeqCst)
    }

    /// 获取当前序号
    pub fn current_sequence(&self) -> u64 {
        self.sequence.load(Ordering::SeqCst)
    }

    /// 获取启动后经过的时间
    pub fn elapsed(&self) -> std::time::Duration {
        self.started_at.elapsed()
    }
}

impl Default for HeartbeatClock {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 4: 创建 token.rs - 心跳标记**

```rust
// crates/a_common/src/heartbeat/token.rs
use chrono::{DateTime, Utc};

/// 心跳标记 - 携带心跳序号和时间信息
#[derive(Clone, Debug)]
pub struct HeartbeatToken {
    /// 心跳序号
    pub sequence: u64,
    /// 墙钟时间 (用于展示)
    pub created_at: DateTime<Utc>,
    /// 单调时钟锚点 (用于计算elapsed)
    started_at: std::time::Instant,
}

impl HeartbeatToken {
    pub(crate) fn new(sequence: u64, started_at: std::time::Instant) -> Self {
        Self {
            sequence,
            created_at: Utc::now(),
            started_at,
        }
    }

    /// 获取序号字符串 "HB_1"
    pub fn sequence_str(&self) -> String {
        format!("HB_{}", self.sequence)
    }

    /// 获取自创建以来经过的时间
    pub fn elapsed(&self) -> std::time::Duration {
        self.started_at.elapsed()
    }
}

impl std::fmt::Display for HeartbeatToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HB_{}", self.sequence)
    }
}
```

- [ ] **Step 5: 创建 config.rs - 配置结构**

```rust
// crates/a_common/src/heartbeat/config.rs

/// 报告器配置
#[derive(Clone, Debug)]
pub struct ReporterConfig {
    /// 失联阈值 (心跳周期数)
    pub stale_threshold: u64,
    /// 报告生成间隔 (秒)
    pub report_interval_secs: u64,
    /// 文件保留时间 (小时)
    pub max_file_age_hours: u64,
    /// 文件大小限制 (MB)
    pub max_file_size_mb: u64,
}

impl Default for ReporterConfig {
    fn default() -> Self {
        Self {
            stale_threshold: 3,
            report_interval_secs: 300,
            max_file_age_hours: 24,
            max_file_size_mb: 100,
        }
    }
}
```

- [ ] **Step 6: 创建 mode.rs - 降级模式**

```rust
// crates/a_common/src/heartbeat/mode.rs
use std::sync::atomic::{AtomicU8, Ordering};

/// 报告模式 - 支持 Full/Sampling/Disabled 三种模式
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportMode {
    /// 正常模式 - 每次都报到
    Full,
    /// 采样模式 - 1/N 概率报到
    Sampling(u32),
    /// 禁用模式 - 完全关闭
    Disabled,
}

impl ReportMode {
    /// 判断是否应该报到
    pub fn should_report(&self) -> bool {
        match self {
            ReportMode::Full => true,
            ReportMode::Sampling(n) => {
                use std::collections::hash_map::RandomState;
                use std::hash::{BuildHasher, Hash, Hasher};
                let rng: u32 = RandomState::new()
                    .build_hasher()
                    .finish() as u32;
                rng % n == 0
            },
            ReportMode::Disabled => false,
        }
    }
}

impl Default for ReportMode {
    fn default() -> Self {
        ReportMode::Full
    }
}
```

- [ ] **Step 7: 创建 points.rs - 测试点映射**

```rust
// crates/a_common/src/heartbeat/points.rs

/// 测试点名称映射表
const TEST_POINT_NAMES: &[(&str, &str)] = &[
    // a_common
    ("AC-001", "BinanceApiGateway"),
    ("AC-004", "BinanceWsConnector"),
    ("AC-005", "BinanceWsConnector"),
    ("AC-006", "BinanceWsConnector"),

    // b_data_source
    ("BS-001", "Kline1mStream"),
    ("BS-003", "Kline1dStream"),
    ("BS-004", "DepthStream"),
    ("BS-007", "FuturesDataSyncer"),

    // c_data_process
    ("CP-001", "SignalProcessor"),
    ("CP-007", "StrategyStateManager"),

    // d_checktable
    ("DT-001", "CheckTable"),
    ("DT-002", "h_15m::Trader"),

    // e_risk_monitor
    ("ER-001", "RiskPreChecker"),
    ("ER-003", "OrderCheck"),

    // f_engine
    ("FE-001", "EventEngine"),
    ("FE-003", "EventBus"),
];

/// 根据测试点ID获取名称
pub fn get_point_name(point_id: &str) -> Option<&'static str> {
    TEST_POINT_NAMES.iter()
        .find(|(id, _)| *id == point_id)
        .map(|(_, name)| *name)
}
```

- [ ] **Step 8: 创建 entry.rs - 报到条目**

```rust
// crates/a_common/src/heartbeat/entry.rs
use chrono::{DateTime, Utc};

/// 报到条目 - 记录每个测试点的报到状态
#[derive(Clone, Debug)]
pub struct ReportEntry {
    /// 测试点ID
    pub point_id: String,
    /// 测试点名称
    pub point_name: String,
    /// 模块名
    pub module: String,
    /// 函数名
    pub function: String,
    /// 文件路径
    pub file: String,
    /// 报到次数
    pub reports_count: u64,
    /// 最后报到时间
    pub last_report_at: DateTime<Utc>,
    /// 最后报到对应的心跳序号
    pub last_heartbeat_seq: u64,
    /// 是否失联
    pub is_stale: bool,
}

impl ReportEntry {
    pub fn new(
        point_id: &str,
        point_name: &str,
        module: &str,
        function: &str,
        file: &str,
    ) -> Self {
        Self {
            point_id: point_id.to_string(),
            point_name: point_name.to_string(),
            module: module.to_string(),
            function: function.to_string(),
            file: file.to_string(),
            reports_count: 0,
            last_report_at: Utc::now(),
            last_heartbeat_seq: 0,
            is_stale: false,
        }
    }

    /// 记录一次报到
    pub fn record_report(&mut self, seq: u64) {
        self.reports_count += 1;
        self.last_report_at = Utc::now();
        self.last_heartbeat_seq = seq;
        self.is_stale = false;
    }

    /// 检查是否失联
    pub fn check_stale(&mut self, current_seq: u64, threshold: u64) -> bool {
        if current_seq - self.last_heartbeat_seq > threshold {
            self.is_stale = true;
        }
        self.is_stale
    }
}
```

- [ ] **Step 9: 创建 reporter.rs - 核心报告器**

```rust
// crates/a_common/src/heartbeat/reporter.rs
use std::collections::HashMap;
use std::sync::Arc;
use chrono::{DateTime, Utc};
use tokio::sync::RwLock;

use super::{Clock, Config, Entry, Mode, Points, Token};

/// 心跳报告器核心
pub struct HeartbeatReporter {
    clock: Clock,
    entries: RwLock<HashMap<String, Entry>>,
    config: Config,
    mode: Arc<Mode>,
}

impl HeartbeatReporter {
    pub fn new(config: Config) -> Self {
        Self {
            clock: Clock::new(),
            entries: RwLock::new(HashMap::new()),
            config,
            mode: Arc::new(Mode::default()),
        }
    }

    /// 生成新的心跳标记
    pub async fn generate_token(&self) -> Token {
        let seq = self.clock.next_sequence();
        let started_at = std::time::Instant::now();
        Token::new(seq, started_at)
    }

    /// 报到
    pub async fn report(
        &self,
        token: &Token,
        point_id: &str,
        module: &str,
        function: &str,
        file: &str,
    ) {
        // 检查模式
        if !self.mode.should_report() {
            return;
        }

        let mut entries = self.entries.write().await;
        let entry = entries.entry(point_id.to_string()).or_insert_with(|| {
            let name = Points::get_point_name(point_id).unwrap_or("Unknown");
            Entry::new(point_id, name, module, function, file)
        });
        entry.record_report(token.sequence);
    }

    /// 获取摘要
    pub async fn summary(&self) -> Summary {
        let entries = self.entries.read().await;
        let total = entries.len();
        let active = entries.values().filter(|e| !e.is_stale).count();
        let stale = entries.values().filter(|e| e.is_stale).count();
        let reports: u64 = entries.values().map(|e| e.reports_count).sum();
        Summary { total_points: total, active_count: active, inactive_count: stale, reports_count: reports }
    }

    /// 获取失联点
    pub async fn get_stale_points(&self) -> Vec<String> {
        let entries = self.entries.read().await;
        entries.values()
            .filter(|e| e.is_stale)
            .map(|e| e.point_id.clone())
            .collect()
    }

    /// 设置模式
    pub async fn set_mode(&self, mode: Mode) {
        *self.mode = Arc::new(mode);
    }
}

/// 摘要统计
#[derive(Clone, Debug)]
pub struct Summary {
    pub total_points: usize,
    pub active_count: usize,
    pub inactive_count: usize,
    pub reports_count: u64,
}
```

- [ ] **Step 10: 创建 macros.rs - 便捷宏**

```rust
// crates/a_common/src/heartbeat/macros.rs

#[macro_export]
macro_rules! heartbeat {
    ($token:expr, $point_id:expr) => {{
        $crate::heartbeat_reporter::global()
            .report($token, $point_id, module_path!(), function_name!(), file!())
            .await;
    }};
}

#[macro_export]
macro_rules! heartbeat_with_info {
    ($token:expr, $point_id:expr, $module:expr, $function:expr, $file:expr) => {{
        $crate::heartbeat_reporter::global()
            .report($token, $point_id, $module, $function, $file)
            .await;
    }};
}

/// 获取当前函数名 (兼容 stable)
#[macro_export]
macro_rules! function_name {
    () => {{
        fn f() {}
        fn type_name<T>(_: &T) -> &'static str { std::any::type_name::<T>() }
        let name = type_name(&f);
        &name[5..name.len() - 1]
    }};
}
```

- [ ] **Step 11: 创建 mod.rs - 模块入口**

```rust
// crates/a_common/src/heartbeat/mod.rs
pub mod clock;
pub mod config;
pub mod entry;
pub mod macros;
pub mod mode;
pub mod points;
pub mod reporter;
pub mod token;

pub use clock::HeartbeatClock as Clock;
pub use config::ReporterConfig as Config;
pub use entry::ReportEntry as Entry;
pub use macros::{heartbeat, heartbeat_with_info, function_name};
pub use mode::ReportMode as Mode;
pub use points::{TEST_POINT_NAMES, get_point_name as Points};
pub use reporter::{HeartbeatReporter as Reporter, Summary};
pub use token::HeartbeatToken as Token;

use once_cell::sync::Lazy;
use tokio::sync::Mutex;

static REPORTER: Lazy<Mutex<Option<Reporter>>> = Lazy::new(|| Mutex::new(None));

/// 初始化全局报告器
pub async fn init(config: Config) {
    let mut reporter = REPORTER.lock().await;
    *reporter = Some(Reporter::new(config));
}

/// 获取全局报告器
pub fn global() -> &'static Reporter {
    panic!("HeartbeatReporter not initialized. Call heartbeat::init() first.")
}
```

- [ ] **Step 12: 修改 lib.rs - 添加模块导出**

在 `crates/a_common/src/lib.rs` 中添加:

```rust
// 添加模块
pub mod heartbeat;

// 添加 re-exports
pub use heartbeat::{Clock, Config, Entry, Mode, Points, Reporter, Summary, Token};
pub use heartbeat::{heartbeat, heartbeat_with_info, function_name};
```

- [ ] **Step 13: 验证编译**

Run: `cd D:/RusProject/barter-rs-main && cargo build -p a_common`
Expected: BUILD SUCCESSFUL

- [ ] **Step 14: 提交**

```bash
cd D:/RusProject/barter-rs-main
git add crates/a_common/src/heartbeat/
git add crates/a_common/src/lib.rs
git add crates/a_common/Cargo.toml
git commit -m "feat(a_common): 创建心跳报告器骨架 (Phase 1)"
```

---

## Task 2: 添加全局单例访问

**Files:**
- Modify: `crates/a_common/src/heartbeat/mod.rs`
- Modify: `crates/a_common/src/heartbeat/reporter.rs`

- [ ] **Step 1: 修改 mod.rs 添加全局单例**

```rust
// crates/a_common/src/heartbeat/mod.rs (修订)

use once_cell::sync::Lazy;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;

static REPORTER: Lazy<Arc<Reporter>> = Lazy::new(|| {
    Arc::new(Reporter::new(Config::default()))
});

/// 获取全局报告器引用
pub fn global() -> &'static Reporter {
    &REPORTER
}

/// 初始化全局报告器 (可选，用于自定义配置)
pub fn init_with_config(config: Config) {
    // 替换全局实例
    // 注意：这需要在程序启动早期调用
}
```

- [ ] **Step 2: 修改 reporter.rs 移除静态初始化逻辑**

reporter.rs 保持原样，全局单例在 mod.rs 中处理

- [ ] **Step 3: 验证编译**

Run: `cargo build -p a_common`
Expected: BUILD SUCCESSFUL

- [ ] **Step 4: 提交**

```bash
git add crates/a_common/src/heartbeat/mod.rs
git commit -m "refactor(a_common): 使用 once_cell 实现全局单例"
```

---

## Task 3: 添加 P0 测试点集成

**Files:**
- Modify: `crates/b_data_source/src/lib.rs`
- Modify: `crates/b_data_source/src/某个模块.rs`
- Modify: `crates/f_engine/src/lib.rs`
- Modify: `crates/c_data_process/src/lib.rs`
- Modify: `crates/d_checktable/src/lib.rs`
- Modify: `crates/e_risk_monitor/src/lib.rs`

- [ ] **Step 1: 在 BS-001 (Kline1mStream) 添加报到**

找到 `crates/b_data_source/src/` 中处理 Kline1m 的模块，添加:

```rust
use a_common::heartbeat::{global, Token};

// 在 K线处理函数中添加
pub async fn on_kline(&mut self, kline: Kline, token: &Token) -> Result<()> {
    global().report(token, "BS-001", "b_data_source", "on_kline", file!()).await;
    // ... 原有逻辑
}
```

- [ ] **Step 2: 在 FE-001/FE-003 (EventEngine/EventBus) 添加报到**

类似地在 f_engine 中添加

- [ ] **Step 3: 在 CP-001 (SignalProcessor) 添加报到**

- [ ] **Step 4: 在 DT-001 (CheckTable) 添加报到**

- [ ] **Step 5: 在 ER-001 (RiskPreChecker) 添加报到**

- [ ] **Step 6: 验证编译**

Run: `cargo build --workspace`
Expected: BUILD SUCCESSFUL

- [ ] **Step 7: 提交**

```bash
git add crates/b_data_source/ crates/f_engine/ crates/c_data_process/ crates/d_checktable/ crates/e_risk_monitor/
git commit -m "feat: 集成 P0 测试点心跳报到"
```

---

## Task 4: 添加单元测试

**Files:**
- Create: `crates/a_common/tests/heartbeat_test.rs`

- [ ] **Step 1: 创建测试文件**

```rust
// crates/a_common/tests/heartbeat_test.rs
use a_common::heartbeat::*;

#[tokio::test]
async fn test_token_generation() {
    let reporter = global();

    let t1 = reporter.generate_token().await;
    let t2 = reporter.generate_token().await;

    assert_eq!(t2.sequence - t1.sequence, 1);
    assert_eq!(t1.sequence_str(), format!("HB_{}", t1.sequence));
}

#[tokio::test]
async fn test_report_and_query() {
    let reporter = global();

    let token = reporter.generate_token().await;
    reporter.report(&token, "BS-001", "b_data_source", "test_fn", "test.rs").await;

    let summary = reporter.summary().await;
    assert!(summary.reports_count >= 1);
}

#[tokio::test]
async fn test_sampling_mode() {
    let reporter = global();

    // 切换到采样模式
    reporter.set_mode(ReportMode::Sampling(2)).await;

    let count_before = reporter.summary().await.reports_count;

    // 报到多次
    for _ in 0..100 {
        let token = reporter.generate_token().await;
        reporter.report(&token, "BS-001", "b_data_source", "test", "test.rs").await;
    }

    let count_after = reporter.summary().await.reports_count;
    let added = count_after - count_before;

    // 应该在 30-70 之间 (Sampling(2) 约 50%)
    assert!(added > 20 && added < 80);

    // 恢复 Full 模式
    reporter.set_mode(ReportMode::Full).await;
}

#[tokio::test]
async fn test_disabled_mode() {
    let reporter = global();

    let count_before = reporter.summary().await.reports_count;

    // 切换到禁用模式
    reporter.set_mode(ReportMode::Disabled).await;

    for _ in 0..100 {
        let token = reporter.generate_token().await;
        reporter.report(&token, "BS-001", "b_data_source", "test", "test.rs").await;
    }

    let count_after = reporter.summary().await.reports_count;

    // 应该没有增加
    assert_eq!(count_after, count_before);

    // 恢复 Full 模式
    reporter.set_mode(ReportMode::Full).await;
}
```

- [ ] **Step 2: 运行测试**

Run: `cargo test -p a_common heartbeat`
Expected: All tests PASS

- [ ] **Step 3: 提交**

```bash
git add crates/a_common/tests/heartbeat_test.rs
git commit -m "test(a_common): 添加心跳报告器单元测试"
```

---

## Task 5: 添加报告生成功能

**Files:**
- Modify: `crates/a_common/src/heartbeat/reporter.rs`
- Create: `crates/a_common/src/heartbeat/report_generator.rs`

- [ ] **Step 1: 添加报告生成方法**

在 reporter.rs 中添加:

```rust
use chrono::{DateTime, Utc};
use serde::Serialize;

/// 心跳报告
#[derive(Clone, Debug, Serialize)]
pub struct HeartbeatReport {
    pub report_time: DateTime<Utc>,
    pub heartbeat_sequence: u64,
    pub total_reports: u64,
    pub active_points: usize,
    pub stale_points: usize,
    pub points_detail: Vec<EntrySnapshot>,
}

#[derive(Clone, Debug, Serialize)]
pub struct EntrySnapshot {
    pub point_id: String,
    pub point_name: String,
    pub module: String,
    pub reports_count: u64,
    pub is_stale: bool,
}

impl HeartbeatReporter {
    /// 生成报告
    pub async fn generate_report(&self) -> HeartbeatReport {
        let entries = self.entries.read().await;
        let now = Utc::now();
        let seq = self.clock.current_sequence();

        let active = entries.values().filter(|e| !e.is_stale).count();
        let stale = entries.values().filter(|e| e.is_stale).count();

        HeartbeatReport {
            report_time: now,
            heartbeat_sequence: seq,
            total_reports: entries.values().map(|e| e.reports_count).sum(),
            active_points: active,
            stale_points: stale,
            points_detail: entries.values().map(|e| EntrySnapshot {
                point_id: e.point_id.clone(),
                point_name: e.point_name.clone(),
                module: e.module.clone(),
                reports_count: e.reports_count,
                is_stale: e.is_stale,
            }).collect(),
        }
    }

    /// 保存报告到文件
    pub async fn save_report(&self, path: &str) -> std::io::Result<()> {
        let report = self.generate_report().await;
        let json = serde_json::to_string_pretty(&report)?;
        tokio::fs::write(path, json).await
    }
}
```

- [ ] **Step 2: 验证编译**

Run: `cargo build -p a_common`
Expected: BUILD SUCCESSFUL

- [ ] **Step 3: 提交**

```bash
git add crates/a_common/src/heartbeat/reporter.rs
git commit -m "feat(a_common): 添加心跳报告生成功能"
```

---

## 实施检查清单

完成所有 Task 后，执行以下检查:

- [ ] `cargo build --workspace` 编译通过
- [ ] `cargo test -p a_common heartbeat` 所有测试通过
- [ ] 检查代码格式 `cargo fmt -- --check`
- [ ] 运行 clippy `cargo clippy -p a_common`

---

## 预期产出

1. `crates/a_common/src/heartbeat/` - 完整的心跳报告器模块
2. 各 crate 中集成的 P0 测试点报到
3. 单元测试覆盖
4. 可查询的全局心跳状态
