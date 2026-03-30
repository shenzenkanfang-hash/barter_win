# Phase 6: 策略协程自治 + BarterWin 融合 - Context

**Gathered:** 2026-03-30
**Status:** Ready for execution

<domain>
## Phase Boundary

实现 `H15mStrategyService`，完成策略协程自治 + EngineManager 集成，对齐设计规格第六章"策略协程详细设计"。
</domain>

<decisions>
## Implementation Decisions

### H15mStrategyService 设计
- 包装 `Arc<Trader>`，实现 `StrategyService` trait
- `H15mStrategyServiceConfig` 持有 strategy_id/symbol/cycle_interval/state_center
- `run_one_cycle()` — 调用 `trader.execute_once_wal()` + `state_center.report_alive()`
- `StrategyService` trait: start/stop/health_check/snapshot

### 与 EngineManager 集成
- EngineManager 通过 `StrategyService` trait 管理生命周期
- start() → spawn 协程运行 run()（自循环）
- stop() → 发送 shutdown 信号
- health_check() → 查询 Trader 状态

### Trader 集成
- 直接使用 `trader.execute_once_wal()` 现有实现
- execute_once_wal() 已包含 TradeLock + 指标更新 + 策略决策 + WAL 日志

### main.rs 状态
- main.rs 已为 41 行（<50 行目标已达成）
- 无需重构：已有 `run_pipeline()` 启动函数

### BarterWin 融合
- `StrategyService` trait 已定义，可与 BarterWin EngineState 集成
- `H15mStrategyService` 通过 trait object 持有，EngineManager 通过 trait 接口管理
</decisions>

<reusable>
## Reusable Assets
- `crates/d_checktable/src/strategy_service.rs`: StrategyService trait（已有）
- `crates/d_checktable/src/h_15m/strategy_service.rs`: H15mStrategyService（新建）
- `crates/d_checktable/src/h_15m/trader.rs`: Trader 已有 execute_once_wal()
</reusable>
