# Phase 2 Summary: EngineManager 自动重启机制

**完成时间:** 2026-03-30

## 做了什么

1. **扩展 EngineEntry 结构**: 添加 `retry_count: AtomicU64` 和 `active: AtomicBool` 支持自动重启计数和活跃状态追踪
2. **集成 StateCenter**: EngineManager 持有 `Arc<dyn StateCenter>`，通过 `get_stale()` 检测 stale 组件
3. **实现 respawn()**: 关闭旧协程，重建新协程，复用已有 spawn 逻辑
4. **实现 handle_stale()**: 指数退避策略（1s, 2s, 4s, 8s, 16s, 32s, 60s 上限），双重 stale 重检避免重复重启
5. **实现 run_restart_loop()**: 10s 间隔后台监控循环，每轮遍历所有 stale 组件并指数退避重启
6. **新增 subscribe_shutdown()**: broadcast channel 优雅停止后台循环

## 关键决策

- spawn_fn 返回类型改为 `(JoinHandle, mpsc::Sender)` 元组（包含 stop_tx）
- 使用 `Arc::clone` 模式避免跨 await 传递闭包的生命周期问题
- spawn_fn 使用 `Arc<dyn Fn(...) -> (JoinHandle, mpsc::Sender) + Send + Sync>` trait object
- 两次 stale 检查（启动时 + 退避后）避免竞态条件和重复重启

## 文件变更

| 文件 | 变更 |
|------|------|
| `crates/f_engine/src/engine_manager.rs` | 核心更新：扩展 EngineEntry, respawn, handle_stale, run_restart_loop |

## 验证

```
cargo check -p f_engine  ✅ 0 errors
cargo test -p f_engine engine_manager  ✅ 6/6 passed
cargo clippy -p f_engine  ✅ 0 warnings (f_engine)
```

## 遗留问题

无。Phase 2 全部完成。
