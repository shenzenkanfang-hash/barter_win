# Phase 5: 独立指标服务实现 - Context

**Gathered:** 2026-03-30
**Status:** Ready for execution

<domain>
## Phase Boundary

实现 `MinIndicatorService`（事件触发）和 `DayIndicatorService`（串行批量），对齐设计规格第四章"指标层详细设计"。
</domain>

<decisions>
## Implementation Decisions

### MinIndicatorService - 事件触发
- 包装 `Arc<SignalProcessor>`
- `compute(symbol, kline) -> Result<Indicator1mOutput>` — 策略协程按需调用，同步计算
- `get_latest(symbol) -> Option<Indicator1mOutput>` — 读缓存
- 实现 `IndicatorStore` trait

### DayIndicatorService - 串行批量
- 包装 `Arc<SignalProcessor>` + `Arc<dyn StateCenter>`
- `run() -> !` — 自循环，5分钟批量计算
- `compute_lock: tokio::sync::Mutex<()>` — 串行锁
- `compute_batch()` — 遍历所有日线品种，调用 `day_get_pine`
- `report_alive()` — 同步调用 StateCenter
- 实现 `IndicatorStore` trait

### KlineInput
- 独立 `KlineInput` struct 封装 h/l/c/v
- 策略协程从 SharedStore 拉 KlineData 后转换为 KlineInput

### StateCenter 集成
- `report_alive` 是同步方法（trait 定义），无需 await
- DayIndicatorService::new 接受 Arc<dyn StateCenter> 依赖
</decisions>

<reusable>
## Reusable Assets
- `crates/c_data_process/src/indicator_services.rs`: 新实现文件
- `crates/c_data_process/src/processor.rs`: SignalProcessor 已有 min/day 更新逻辑
- `crates/c_data_process/src/indicator_store.rs`: IndicatorStore trait 已有
</reusable>
