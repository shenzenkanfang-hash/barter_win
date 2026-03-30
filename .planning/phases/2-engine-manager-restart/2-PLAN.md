# Phase 2 Plan: EngineManager 自动重启机制

## Goal
补全 EngineManager 缺失的自动重启机制，与 StateCenter 联动，实现组件心跳超时自动恢复。

## Tasks

### Task 1: 扩展 EngineEntry 结构
**文件**: `crates/f_engine/src/engine_manager.rs`
**变更**: 在 EngineEntry 中添加：
```rust
struct EngineEntry {
    // 现有字段...
    /// 重启计数（用于指数退避）
    retry_count: std::sync::atomic::AtomicU64,
    /// 是否活跃
    active: std::sync::atomic::AtomicBool,
}
```

### Task 2: 添加 StateCenter 依赖
**文件**: `crates/f_engine/src/engine_manager.rs`
**变更**:
- `EngineManager` 结构添加 `state_center: Arc<dyn x_data::state::StateCenter>` 字段
- `EngineManager::new()` 添加 `state_center` 参数
- EngineEntry 中移除内部 heartbeat 跟踪（使用 StateCenter）
- 移除内部 `heartbeat()` 方法（使用 StateCenter.report_alive）

### Task 3: 实现 respawn 方法
**文件**: `crates/f_engine/src/engine_manager.rs`
**变更**: 添加 `respawn()` 方法：
```rust
pub async fn respawn(&self, component_id: &str, factory: F) -> Result<(), EngineError>
where F: FnOnce(String, mpsc::Receiver<()>) -> JoinHandle<()>
```
- 从 entries 移除旧 entry（shutdown）
- 使用工厂闭包重建协程
- 重置 retry_count 为 0

### Task 4: 实现 handle_stale 方法
**文件**: `crates/f_engine/src/engine_manager.rs`
**变更**: 添加 `handle_stale()` 方法：
- 计算指数退避延迟：`min(60, 2_i64.pow(retry_count))`
- retry_count++
- sleep 延迟
- 重新检查是否 stale（避免重复重启）
- 调用 respawn() 重启组件

### Task 5: 实现 restart_loop 后台监控
**文件**: `crates/f_engine/src/engine_manager.rs`
**变更**: 添加 `restart_loop()` 方法：
```rust
pub async fn run_restart_loop(&self, mut shutdown_rx: broadcast::Receiver<()>)
```
- 10s 间隔循环
- 调用 `self.state_center.get_stale(self.config.stale_threshold_secs)`
- 对每个 stale 组件调用 `self.handle_stale()`
- 监听 shutdown_rx，优雅退出

### Task 6: 更新 Config
**文件**: `crates/f_engine/src/engine_manager.rs`
**变更**:
- `EngineManagerConfig` 添加 `restart_check_interval_secs: u64`（默认 10s）
- 保留现有字段：heartbeat_timeout_secs, max_restart_count, restart_interval_secs

### Task 7: 添加测试
**文件**: `crates/f_engine/src/engine_manager.rs`（`#[cfg(test)]` 模块）
**变更**:
- 测试 respawn
- 测试 handle_stale 指数退避
- 测试 restart_loop 检测 stale 并重启

### Task 8: 更新导出
**文件**: `crates/f_engine/src/lib.rs`
**变更**: 检查 EngineManagerConfig 导出是否完整

## Verification
```
cargo check -p f_engine
cargo test -p f_engine engine_manager
cargo clippy -p f_engine
```
