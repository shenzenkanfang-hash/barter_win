# e_strategy 目录重构实施计划

> **注意**: 此计划描述的重组已完成，但命名有所不同。实际结构：
> - 策略蓝图层: `d_blueprint` (而非 `e_strategy`)
> - 风控层: `e_risk_monitor`

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 e_strategy 按分层架构重组，创建 f_engine 和 h_sandbox，删除重复的 symbol/，更新所有依赖和导入路径。

**Architecture:** 按分层架构重组：业务层间只能数据交换+锁控制，a/b 层为工具层可直接引用。core/ → f_engine/core/, mock_binance_gateway.rs → h_sandbox/, order.rs → b_data_source/order/, gateway.rs → f_engine/order/。

**Tech Stack:** Rust, cargo workspace, parking_lot::RwLock

---

## 前置检查

1. 确认当前 `e_strategy/src/` 结构完整
2. 确认 `Cargo.toml` workspace members
3. 备份当前状态（git branch）

---

## Phase 1: 创建新 crate 结构

### Task 1: 创建 f_engine crate

**Files:**
- Create: `crates/f_engine/Cargo.toml`
- Create: `crates/f_engine/src/lib.rs`
- Create: `crates/f_engine/src/core/mod.rs`
- Create: `crates/f_engine/src/order/mod.rs`

- [ ] **Step 1: 创建 f_engine/Cargo.toml**

```toml
[package]
name = "f_engine"
version = "0.1.0"
edition = "2024"

[dependencies]
a_common = { path = "../a_common" }
b_data_source = { path = "../b_data_source" }
c_data_process = { path = "../c_data_process" }
d_risk_monitor = { path = "../d_risk_monitor" }

parking_lot = "0.12"
rust_decimal = { workspace = true }
rust_decimal_macros = { workspace = true }
thiserror = { workspace = true }
tracing = { workspace = true }
serde = { workspace = true }
tokio = { workspace = true }
fnv = { workspace = true }
async-trait = { workspace = true }
```

- [ ] **Step 2: 创建 f_engine/src/lib.rs**

```rust
#![forbid(unsafe_code)]

pub mod core;
pub mod order;
```

- [ ] **Step 3: 创建 f_engine/src/core/mod.rs**

```rust
#![forbid(unsafe_code)]
```

- [ ] **Step 4: 创建 f_engine/src/order/mod.rs**

```rust
#![forbid(unsafe_code)]
```

- [ ] **Step 5: Commit**

```bash
git add crates/f_engine/
git commit -m "[开发者] 创建 f_engine crate 骨架"
```

---

### Task 2: 创建 h_sandbox crate

**Files:**
- Create: `crates/h_sandbox/Cargo.toml`
- Create: `crates/h_sandbox/src/lib.rs`

- [ ] **Step 1: 创建 h_sandbox/Cargo.toml**

```toml
[package]
name = "h_sandbox"
version = "0.1.0"
edition = "2024"

[dependencies]
a_common = { path = "../a_common" }
b_data_source = { path = "../b_data_source" }

parking_lot = "0.12"
rust_decimal = { workspace = true }
rust_decimal_macros = { workspace = true }
thiserror = { workspace = true }
tracing = { workspace = true }
serde = { workspace = true }
tokio = { workspace = true }
```

- [ ] **Step 2: 创建 h_sandbox/src/lib.rs**

```rust
#![forbid(unsafe_code)]
```

- [ ] **Step 3: Commit**

```bash
git add crates/h_sandbox/
git commit -m "[开发者] 创建 h_sandbox crate 骨架"
```

---

## Phase 2: 移动 core/ → f_engine/core/

### Task 3: 移动 core/ 文件

**Files:**
- Move: `crates/e_strategy/src/core/engine.rs` → `crates/f_engine/src/core/engine.rs`
- Move: `crates/e_strategy/src/core/pipeline.rs` → `crates/f_engine/src/core/pipeline.rs`
- Move: `crates/e_strategy/src/core/pipeline_form.rs` → `crates/f_engine/src/core/pipeline_form.rs`
- Move: `crates/e_strategy/src/core/strategy_pool.rs` → `crates/f_engine/src/core/strategy_pool.rs`
- Move: `crates/e_strategy/src/core/mod.rs` → `crates/f_engine/src/core/mod.rs`

- [ ] **Step 1: 创建 f_engine/src/core/ 目录结构**

```bash
mkdir -p crates/f_engine/src/core
```

- [ ] **Step 2: 移动 engine.rs, pipeline.rs, pipeline_form.rs, strategy_pool.rs, mod.rs**

```bash
mv crates/e_strategy/src/core/engine.rs crates/f_engine/src/core/
mv crates/e_strategy/src/core/pipeline.rs crates/f_engine/src/core/
mv crates/e_strategy/src/core/pipeline_form.rs crates/f_engine/src/core/
mv crates/e_strategy/src/core/strategy_pool.rs crates/f_engine/src/core/
mv crates/e_strategy/src/core/mod.rs crates/f_engine/src/core/
```

- [ ] **Step 3: 更新 f_engine/src/core/mod.rs 导出**

```rust
#![forbid(unsafe_code)]

pub mod engine;
pub mod pipeline;
pub mod pipeline_form;
pub mod strategy_pool;

pub use engine::TradingEngine;
pub use pipeline::{Pipeline, Processor, MockIndicatorProcessor, MockStrategyProcessor, MockRiskProcessor};
pub use pipeline_form::PipelineForm;
pub use strategy_pool::{StrategyAllocation, StrategyPool};
```

- [ ] **Step 4: 更新 f_engine/src/lib.rs**

```rust
#![forbid(unsafe_code)]

pub mod core;
pub mod order;
```

- [ ] **Step 5: 删除空的 e_strategy/src/core/ 目录**

```bash
rmdir crates/e_strategy/src/core/
```

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "[开发者] 移动 core/ → f_engine/core/"
```

---

## Phase 3: 移动 gateway.rs → f_engine/order/

### Task 4: 移动 gateway.rs

**Files:**
- Move: `crates/e_strategy/src/order/gateway.rs` → `crates/f_engine/src/order/gateway.rs`
- Modify: `crates/e_strategy/src/order/mod.rs`

- [ ] **Step 1: 移动 gateway.rs**

```bash
mv crates/e_strategy/src/order/gateway.rs crates/f_engine/src/order/
```

- [ ] **Step 2: 更新 f_engine/src/order/mod.rs**

```rust
#![forbid(unsafe_code)]

pub mod gateway;

pub use gateway::ExchangeGateway;
```

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "[开发者] 移动 gateway.rs → f_engine/order/"
```

---

## Phase 4: 移动 order.rs → b_data_source/order/

### Task 5: 移动 order.rs

**Files:**
- Move: `crates/e_strategy/src/order/order.rs` → `crates/b_data_source/src/order/order.rs`
- Move: `crates/e_strategy/src/order/mod.rs` → `crates/b_data_source/src/order/mod.rs`
- Modify: `crates/b_data_source/src/lib.rs`

- [ ] **Step 1: 创建 b_data_source/order/ 目录**

```bash
mkdir -p crates/b_data_source/src/order
```

- [ ] **Step 2: 移动 order.rs 和 mod.rs**

```bash
mv crates/e_strategy/src/order/order.rs crates/b_data_source/src/order/
mv crates/e_strategy/src/order/mod.rs crates/b_data_source/src/order/
```

- [ ] **Step 3: 更新 b_data_source/src/order/mod.rs**

```rust
#![forbid(unsafe_code)]

pub mod order;

pub use order::OrderExecutor;
```

- [ ] **Step 4: 更新 b_data_source/src/lib.rs 添加 order 模块**

```rust
#![forbid(unsafe_code)]

pub mod api;
pub mod ws;
pub mod error;
pub mod backup;
pub mod kline;
pub mod market;
pub mod order;  // 新增

// ... existing exports
```

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "[开发者] 移动 order.rs → b_data_source/order/"
```

---

## Phase 5: 移动 mock_binance_gateway.rs → h_sandbox/

### Task 6: 移动 mock_binance_gateway.rs

**Files:**
- Move: `crates/e_strategy/src/order/mock_binance_gateway.rs` → `crates/h_sandbox/src/mock_binance_gateway.rs`
- Modify: `crates/e_strategy/src/order/` (删除)

- [ ] **Step 1: 移动 mock_binance_gateway.rs**

```bash
mv crates/e_strategy/src/order/mock_binance_gateway.rs crates/h_sandbox/src/
```

- [ ] **Step 2: 更新 h_sandbox/src/lib.rs**

```rust
#![forbid(unsafe_code)]

pub mod mock_binance_gateway;

pub use mock_binance_gateway::*;
```

- [ ] **Step 3: 删除 e_strategy/src/order/ 目录**

```bash
rm -rf crates/e_strategy/src/order/
```

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "[开发者] 移动 mock_binance_gateway.rs → h_sandbox/"
```

---

## Phase 6: 重组 e_strategy

### Task 7: 更新 e_strategy/lib.rs

**Files:**
- Modify: `crates/e_strategy/src/lib.rs`
- Modify: `crates/e_strategy/src/channel/mod.rs`
- Modify: `crates/e_strategy/src/shared/mod.rs`

- [ ] **Step 1: 更新 e_strategy/src/lib.rs**

```rust
#![forbid(unsafe_code)]

pub mod channel;
pub mod strategy;
pub mod shared;

// Re-exports
pub use channel::{channel::*, mode::*};
pub use strategy::{traits::*, types::*, pin_strategy::*, trend_strategy::*};
pub use shared::check_table::*;
```

- [ ] **Step 2: 更新 e_strategy/src/channel/mod.rs**

```rust
#![forbid(unsafe_code)]

pub mod channel;
pub mod mode;

pub use channel::{ChannelType, ChannelCheckpointCallback, VolatilityChannel};
pub use mode::ModeSwitcher;
```

- [ ] **Step 3: 更新 e_strategy/src/shared/mod.rs**

```rust
#![forbid(unsafe_code)]

pub mod check_table;

pub use check_table::{CheckEntry, CheckTable};
```

- [ ] **Step 4: 更新 e_strategy/src/strategy/mod.rs**

```rust
#![forbid(unsafe_code)]

pub mod traits;
pub mod types;
pub mod pin_strategy;
pub mod trend_strategy;

pub use traits::{MinuteIndicators, Strategy};
pub use types::{OrderRequest, Side, Signal};
```

- [ ] **Step 5: 删除 e_strategy/src/symbol/ 目录**

```bash
rm -rf crates/e_strategy/src/symbol/
```

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "[开发者] 重组 e_strategy：删除 symbol/，更新导出"
```

---

## Phase 7: 更新 Cargo.toml

### Task 8: 更新 workspace Cargo.toml

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: 更新 workspace members**

```toml
[workspace]
members = [
    "crates/a_common",
    "crates/b_data_source",
    "crates/c_data_process",
    "crates/d_risk_monitor",
    "crates/e_strategy",
    "crates/f_engine",
    "crates/h_sandbox",
    "crates/g_test",
]
```

- [ ] **Step 2: 添加 f_engine 和 h_sandbox 依赖**

```toml
[dependencies]
# ... existing
f_engine = { path = "crates/f_engine" }
h_sandbox = { path = "crates/h_sandbox" }
```

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "[开发者] 更新 Cargo.toml：添加 f_engine 和 h_sandbox"
```

---

## Phase 8: 更新所有导入路径

### Task 9: 更新 e_strategy 内部导入

**Files:**
- Modify: `crates/e_strategy/src/channel/channel.rs`
- Modify: `crates/e_strategy/src/strategy/*.rs`

- [ ] **Step 1: 检查并更新 channel.rs 导入**

查找所有 `use crate::order::` 或 `use crate::core::` 导入，替换为：
- `crate::channel` → 保持
- `crate::core` → `f_engine::core`
- `crate::order` → 删除（不再需要）

- [ ] **Step 2: 检查并更新 strategy/*.rs 导入**

- [ ] **Step 3: Commit**

---

### Task 10: 更新 f_engine/core/*.rs 导入

**Files:**
- Modify: `crates/f_engine/src/core/engine.rs`
- Modify: `crates/f_engine/src/core/pipeline.rs`
- Modify: `crates/f_engine/src/core/strategy_pool.rs`

- [ ] **Step 1: 更新 engine.rs 导入路径**

旧导入（来自 e_strategy 时期）：
```rust
use crate::order::gateway::ExchangeGateway;
use crate::order::OrderExecutor;
use crate::channel::mode::ModeSwitcher;
use crate::core::strategy_pool::StrategyPool;
```

新导入：
```rust
// gateway 和 OrderExecutor 来自 b_data_source
// channel 来自 e_strategy
// core::strategy_pool 保持（内部）
```

- [ ] **Step 2: 检查并修复所有内部 crate 引用**

```bash
cargo check -p f_engine 2>&1 | head -50
```

- [ ] **Step 3: Commit**

---

### Task 11: 更新 h_sandbox/mock_binance_gateway.rs 导入

**Files:**
- Modify: `crates/h_sandbox/src/mock_binance_gateway.rs`

- [ ] **Step 1: 检查导入路径并修复**

- [ ] **Step 2: 编译验证**

```bash
cargo check -p h_sandbox 2>&1 | head -50
```

- [ ] **Step 3: Commit**

---

## Phase 9: 编译验证

### Task 12: 全量编译验证

- [ ] **Step 1: 执行 cargo check --all**

```bash
cargo check --all 2>&1
```

- [ ] **Step 2: 如有错误，根据错误信息修复导入路径**

- [ ] **Step 3: 重复直到编译通过**

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "[测试工程师] 验证编译通过"
```

---

## 任务完成检查清单

- [ ] f_engine crate 创建完成
- [ ] h_sandbox crate 创建完成
- [ ] core/ 移动到 f_engine/core/
- [ ] gateway.rs 移动到 f_engine/order/
- [ ] order.rs 移动到 b_data_source/order/
- [ ] mock_binance_gateway.rs 移动到 h_sandbox/
- [ ] e_strategy/src/symbol/ 删除
- [ ] e_strategy/lib.rs 更新导出
- [ ] Cargo.toml 更新 workspace members
- [ ] 所有导入路径更新
- [ ] cargo check --all 通过
