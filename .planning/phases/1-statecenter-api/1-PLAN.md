# Phase 1 Plan: StateCenter API 标准化与完善

## Goal
将 `StateCenterTrait` API 与设计规格完全对齐，建立轻量心跳机制。

## Tasks

### Task 1: 更新 StateCenterTrait 方法签名
**文件**: `crates/x_data/src/state/center.rs`
**变更**:
- `heartbeat(component_id)` → `report_alive(component_id)` + 返回 `Result<(), StateCenterError>`
- `stop(component_id)` → 保留（`stop`语义与`report_error`不同）
- `set_error(component_id, error)` → `report_error(component_id, error)` + 返回 `Result<(), StateCenterError>`
- `get_state(component_id)` → `get(component_id)`（返回值 `Option<ComponentState>` 不变）
- `get_all_states()` → `get_all()`（返回 `Vec<ComponentState>` 不变）
- `get_running_components()` → 移除（被 `get_alive` 替代）
- `get_stopped_components()` → 移除（不在设计规格中）
- `get_stale_components()` → `get_stale(threshold_secs: i64)`（加参数）
- 新增: `get_alive(timeout_secs: i64) -> Vec<ComponentState>`

### Task 2: 实现 StateCenterError 错误类型
**文件**: `crates/x_data/src/state/center.rs`
**变更**: 添加 `StateCenterError` 枚举，用于 `report_alive`/`report_error` 返回值

### Task 3: 更新 StateCenterImpl 实现
**文件**: `crates/x_data/src/state/center.rs`
**变更**: 按新方法签名更新实现体

### Task 4: 添加 backward compatibility type alias
**文件**: `crates/x_data/src/state/center.rs`
**变更**: 保留旧方法名作为 deprecated alias
```rust
#[deprecated(since = "0.1.0", note = "Use report_alive instead")]
fn heartbeat(&self, _: &str) -> Option<()> { ... }
```

### Task 5: 更新测试
**文件**: `crates/x_data/src/state/center.rs`（同一文件的 `#[cfg(test)]` 模块）
**变更**: 测试新方法名

### Task 6: 验证编译
**命令**: `cargo check -p x_data && cargo test -p x_data`

## Verification
```
cargo check -p x_data
cargo test -p x_data
cargo clippy -p x_data
```
