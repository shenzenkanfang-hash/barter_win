# Phase 2: EngineManager 自动重启机制 - Context

**Gathered:** 2026-03-30
**Status:** Ready for planning

<domain>
## Phase Boundary

补全 `EngineManager` 缺失的自动重启机制：后台监控循环、指数退避重启策略、与 StateCenter 联动。
</domain>

<decisions>
## Implementation Decisions

### Claude's Discretion
纯基础设施阶段，实现选择由 Claude 自行判断：
- restart_loop 作为独立后台任务运行，通过 shutdown_tx 的 subscribe 机制优雅停止
- 指数退避: 1s, 2s, 4s, 8s, 16s, 32s, 60s 上限
- StateCenter 作为 Arc<dyn StateCenter> 注入 EngineManager
- respawn 需要组件工厂函数（FnOnce closure）传入
</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `crates/f_engine/src/engine_manager.rs`: 已有基础结构（spawn, restart, shutdown）
- `crates/x_data/src/state/center.rs`: StateCenterTrait（Phase 1 已完善）
- `tokio::sync::broadcast`: 用于 shutdown 信号传播

### Established Patterns
- 使用 `Arc<RwLock<HashMap>>` 管理协程表
- JoinHandle + mpsc::Sender 用于协程生命周期管理
- EngineManagerConfig 提供可配置参数
- 测试使用 tokio::test 宏

### Integration Points
- StateCenter: 注入为 Arc<dyn StateCenter>
- respawn: 需要传入 FnOnce(String, mpsc::Receiver<()>) -> JoinHandle<()> 工厂闭包
</code_context>

<specifics>
## Specific Ideas

设计规格（第八节 8.2-8.3）：
```
StrategyHandle {
    component_id, symbol, join_handle, shutdown_tx,
    retry_count: AtomicU64,  // 指数退避计数
    active: AtomicBool       // 是否活跃
}

restart_loop():
  - 10s 间隔检测
  - 调用 state_center.get_stale(threshold_secs)
  - 对每个 stale 组件调用 handle_stale()

handle_stale():
  - 指数退避: min(60, 2^retry_count) 秒
  - retry_count++
  - 重新检查 stale（避免重复重启）
  - 调用 respawn()

respawn(component_id):
  - 从 entries 移除旧 entry
  - 调用工厂闭包重建协程
  - 重置 retry_count
```
</specifics>

<deferred>
## Deferred Ideas

无 — 阶段范围清晰
</deferred>
