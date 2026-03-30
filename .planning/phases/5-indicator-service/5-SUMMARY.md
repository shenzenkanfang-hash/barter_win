# Phase 5 Summary: 独立指标服务实现

**完成时间:** 2026-03-30

## 做了什么

1. **MinIndicatorService** — 事件触发型分钟级指标服务
   - 包装 `Arc<SignalProcessor>`
   - `compute(symbol, KlineInput)` — 策略协程按需调用，同步计算
   - `get_latest(symbol)` — 读缓存
   - 实现 `IndicatorStore` trait
   - `KlineInput` struct 封装 h/l/c/v

2. **DayIndicatorService** — 串行批量型日线级指标服务
   - 包装 `Arc<SignalProcessor>` + `Arc<dyn StateCenter>`
   - `run() -> !` 自循环（5分钟批量 + shutdown 监听）
   - `compute_lock: tokio::sync::Mutex<()>` 串行锁
   - `compute_batch()` — 遍历所有日线品种，批量计算并缓存
   - `report_alive()` — 同步调用 StateCenter 心跳
   - 实现 `IndicatorStore` trait
   - 实现了 `SignalProcessor::registered_day_symbols()` 公开访问器

3. **Cargo 依赖更新**: `c_data_process/Cargo.toml` 添加 `x_data` 依赖
4. **lib.rs 导出**: 添加 `MinIndicatorService`, `DayIndicatorService`, `KlineInput`
5. **单元测试**: 5 个测试覆盖计算/未注册/shutdown/cache/IndicatorStore trait

## 关键决策

- `report_alive` 是同步方法，StateCenter trait 中定义为 `fn` 而非 `async fn`
- `shutdown_rx` 在 `run()` 开始时 clone，避免 `Arc<Self>` 无法 `DerefMut`
- `DayIndicatorService` 通过 trait object 依赖 StateCenter，保持解耦

## 文件变更

| 文件 | 变更 |
|------|------|
| `crates/c_data_process/src/indicator_services.rs` | 新建（完整实现） |
| `crates/c_data_process/src/lib.rs` | 添加模块 + 导出 |
| `crates/c_data_process/Cargo.toml` | 添加 x_data 依赖 |
| `crates/c_data_process/src/processor.rs` | 添加 `registered_day_symbols()` |

## 验证

```
cargo check -p c_data_process  ✅ 0 errors
cargo test -p c_data_process indicator_services  ✅ 5/5 passed
```

## 遗留问题

无。Phase 5 全部完成。
