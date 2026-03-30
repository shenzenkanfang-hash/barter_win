# Phase 2: EngineManager 自动重启机制 - Verification

**status:** passed
**date:** 2026-03-30

## Verification Results

### cargo check
```
cargo check -p f_engine
Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.85s
```
**Result:** PASSED - 0 errors

### cargo test (engine_manager)
```
cargo test -p f_engine engine_manager
running 6 tests
test engine_manager::tests::test_subscribe_shutdown ... ok
test engine_manager::tests::test_shutdown_nonexistent ... ok
test engine_manager::tests::test_spawn_duplicate ... ok
test engine_manager::tests::test_spawn_and_shutdown ... ok
test engine_manager::tests::test_restart ... ok
test engine_manager::tests::test_get_all_states ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 22 filtered out
```
**Result:** PASSED - 6/6 tests passed

### cargo clippy (f_engine)
```
cargo clippy -p f_engine
Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.85s
```
**Result:** PASSED - 0 warnings (only pre-existing warnings in other crates)

## Checklist

### 核心功能（设计规格 8.1-8.3）
- [x] EngineEntry 扩展：retry_count (AtomicU64), active (AtomicBool)
- [x] EngineManager 新增 state_center: Arc<dyn StateCenter> 字段
- [x] EngineManagerConfig 新增字段：stale_threshold_secs, restart_check_interval_secs, shutdown_timeout_secs
- [x] respawn() 方法实现：从 entries 移除旧 entry，重建协程
- [x] handle_stale() 方法实现：指数退避 min(60, 2^retry_count) 秒，retry_count++，重检 stale，调用 respawn
- [x] run_restart_loop() 后台监控：10s 间隔，调用 get_stale，指数退避重启
- [x] subscribe_shutdown() 方法：broadcast channel 用于优雅停止

### API 对齐
- [x] StateCenter Trait 作为 Arc<dyn StateCenter> 注入
- [x] 使用 report_alive/get/get_stale 与 Phase 1 API 对齐
- [x] 指数退避策略：1s, 2s, 4s, 8s, 16s, 32s, 60s 上限

### 代码质量
- [x] spawn_fn 使用 Arc<dyn Fn(...) -> (JoinHandle, mpsc::Sender) + Send + Sync>
- [x] Arc::clone 模式避免跨 await 生命周期问题
- [x] 双重 stale 检查（避免竞态条件）
- [x] 优雅 shutdown（stop_tx + timeout + handle abort）
- [x] 重试后重置 retry_count

### 文件变更
- [x] `crates/f_engine/src/engine_manager.rs` - 核心更新
