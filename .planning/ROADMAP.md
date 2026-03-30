# Barter-Rs 事件驱动架构迁移路线图

**项目**: Barter-Rs Rust 量化交易系统
**目标**: 从 PipelineBus+Actor 模式迁移到 v3.1 事件驱动协程自治架构
**创建时间**: 2026-03-30
**基于**: `docs/superpowers/specs/2026-03-30-event-driven-architecture-design.md`

---

## 里程碑 1: 事件驱动协程自治架构迁移 (m1.0)

**状态**: 规划中
**完成度**: 68%（已有基础组件）
**迁移策略**: 完全迁移到 v3.1（废弃 PipelineBus+Actor，采用 SharedStore+独立协程）

### 阶段依赖关系

```
Phase 1 (StateCenter API标准化)
    │
    ├── Phase 2 (EngineManager 自动重启)
    │
    ├── Phase 3 (风控服务两阶段抽取)
    │
    ├── Phase 4 (SharedStore 序列号完善)
    │
    ├── Phase 5 (独立指标服务实现)
    │
    └── Phase 6 (策略协程自治 + BarterWin 融合)
```

---

## Phase 1: StateCenter API 标准化与完善

**目标**: 统一 API 命名，补全缺失方法，建立轻量心跳机制
**基于设计规格**: 第三节 - StateCenter 详细设计
**依赖**: 无
**预计工作量**: 0.5天

### Goal
将 `heartbeat()` 统一为 `report_alive()`，`stop()` 统一为 `report_error()`，补全 `get_stale()` 实现，建立心跳超时检测机制。

### Plans
- [ ] 统一 StateCenterTrait API 命名（report_alive/heartbeat → report_alive, stop → report_error）
- [ ] 补全 get_stale() 方法实现
- [ ] 验证 ComponentStatus::Stale 状态转换逻辑
- [ ] 建立心跳超时阈值配置（默认 30s）
- [ ] 所有现有调用方更新为新 API 名称

### Verification
```
cargo check -p x_data
cargo test -p x_data
```

---

## Phase 2: EngineManager 自动重启机制

**目标**: 实现 restart_loop()、handle_stale()，与 StateCenter 联动自动重启
**基于设计规格**: 第八节 - EngineManager 详细设计
**依赖**: Phase 1（依赖 StateCenter API 完善）
**预计工作量**: 1天

### Goal
补全 EngineManager 缺失的 restart_loop() 后台监控循环、handle_stale() stale 组件处理、指数退避重启策略，实现组件心跳超时自动恢复。

### Plans
- [ ] 实现 restart_loop() 后台监控循环（10s 间隔检测 stale）
- [ ] 实现 handle_stale() 方法（指数退避：1s, 2s, 4s, 8s, 16s, 32s, 60s）
- [ ] 实现 StrategyHandle::retry_count 和 active 字段
- [ ] 与 StateCenter.get_stale() 联动
- [ ] 验证: 模拟组件心跳超时，验证自动重启

### Verification
```
cargo test engine_manager
cargo check -p f_engine
```

---

## Phase 3: 风控服务两阶段抽取

**目标**: 完善 RiskService PreCheck/FinalCheck 两阶段，补全 RiskCheckRequest/RiskCheckResult
**基于设计规格**: 第五节 - 风控层详细设计
**依赖**: Phase 1（依赖 StateCenter API）
**预计工作量**: 0.5天

### Goal
将 `re_check()` 重命名为 `final_check()`，补全 RiskCheckRequest/RiskCheckResult 结构体定义，完善两阶段锁机制文档。

### Plans
- [ ] 重命名 re_check() → final_check()（API 对齐设计规格）
- [ ] 补全 RiskCheckRequest 结构体（symbol, side, qty, price, strategy_id）
- [ ] 补全 RiskCheckResult 结构体（approved, reason, adjusted_qty）
- [ ] 验证 TradeLock 两阶段锁机制与风控服务联动
- [ ] 更新所有风控调用方

### Verification
```
cargo check -p e_risk_monitor
cargo test risk_service
```

---

## Phase 4: SharedStore 序列号机制完善

**目标**: 完善 KlineWithSeq/StoreOutput 结构，补全 get_since() 增量读取
**基于设计规格**: 第七节 - SharedStore 详细设计
**依赖**: 无
**预计工作量**: 0.5天

### Goal
确保 SharedStore 实现 KlineWithSeq 版本号机制，策略协程可通过 internal_seq 维护增量读取状态。

### Plans
- [ ] 验证 KlineWithSeq 结构实现（kline, seq, timestamp）
- [ ] 验证 StoreOutput<T> 泛型包装
- [ ] 补全 get_since(symbol, min_seq) 增量读取方法
- [ ] 验证序列号单调递增机制

### Verification
```
cargo check -p b_data_source
cargo test shared_store
```

---

## Phase 5: 独立指标服务实现

**目标**: 抽取 MinIndicatorService（日触发）和 DayIndicatorService（串行批量）
**基于设计规格**: 第四节 - 指标层详细设计
**依赖**: Phase 4（依赖 SharedStore）
**预计工作量**: 2天

### Goal
从现有 SignalProcessor 中抽取独立的 MinIndicatorService（日触发）和 DayIndicatorService（串行批量计算），实现 IndicatorStore trait 统一访问。

### Plans
- [ ] 创建 MinIndicatorService（日触发模式）
  - [ ] MinIndicatorService trait（compute() + get_latest()）
  - [ ] 日触发计算逻辑
  - [ ] 与策略协程集成
- [ ] 创建 DayIndicatorService（串行批量模式）
  - [ ] DayIndicatorService 结构
  - [ ] compute_lock 串行锁机制
  - [ ] 300s 间隔批量计算循环
  - [ ] compute_batch() 串行遍历所有 symbol
- [ ] 统一 IndicatorStore trait（get_min() + get_day()）
- [ ] 废弃旧的 SignalProcessorIndicatorStore

### Verification
```
cargo check -p c_data_process
cargo test indicator
```

---

## Phase 6: 策略协程自治 + BarterWin Engine 融合

**目标**: 实现 H15mStrategyService 自循环，融合 BarterWin Engine
**基于设计规格**: 第六节 + 第九节 - 策略协程 + main.rs 重构
**依赖**: Phase 2, Phase 3, Phase 5（依赖所有前置阶段）
**预计工作量**: 2.5天

### Goal
将策略逻辑重构为独立的 H15mStrategyService（自循环模式），实现自循环逻辑（拉数据→拉指标→决策→报状态→发起风控），融合 BarterWin Engine。

### Plans
- [ ] 创建 H15mStrategyService trait + 实现
  - [ ] StrategyService trait（component_id, symbol, status, run 自循环）
  - [ ] H15mStrategyService 结构（shared_store, indicator_store, trader, risk_service, trade_lock, gateway, state_center）
  - [ ] run_one_cycle() 自循环逻辑
  - [ ] execute_with_risk_control() 两阶段风控调用
- [ ] 废弃 src/actors.rs 中的 run_strategy_actor()
- [ ] 重构 main.rs 为纯启动引导（< 50行）
  - [ ] init_tracing()
  - [ ] create_shared_components()
  - [ ] create_services()
  - [ ] engine.spawn() 所有服务
  - [ ] run_monitor_loop()
- [ ] BarterWin Engine 融合
  - [ ] EngineState<IndicatorGlobalData, ExtendedInstData> 接入 SharedStore
  - [ ] H15mStrategy → AlgoStrategy trait 映射
  - [ ] RiskManager → ManagedRM 集成

### Verification
```
cargo check
cargo test
cargo clippy
```

---

## 验收标准

### 功能验收
- [ ] main.rs < 50行，无业务流水线逻辑
- [ ] StateCenter 轻量心跳上报正常
- [ ] 风控两阶段检查生效（PreCheck + FinalCheck）
- [ ] MinIndicatorService 事件触发计算正常
- [ ] DayIndicatorService 串行批量计算正常
- [ ] 策略协程自循环运行
- [ ] EngineManager 能监控和重启策略协程
- [ ] BarterWin Engine 成功融合

### 非功能验收
- [ ] cargo check 零警告
- [ ] cargo test 全通过
- [ ] cargo clippy 全通过

---

## 文件变更清单

### 新增文件
- `crates/c_data_process/src/min_service.rs` - MinIndicatorService
- `crates/c_data_process/src/day_service.rs` - DayIndicatorService
- `crates/d_checktable/src/h_15m/strategy_service.rs` - H15mStrategyService
- `docs/superpowers/plans/m1.0-event-driven-architecture/phase-{N}-*.md` - 各阶段详细计划

### 修改文件
- `crates/x_data/src/state/center.rs` - API 命名统一
- `crates/f_engine/src/engine_manager.rs` - restart_loop + handle_stale
- `crates/e_risk_monitor/src/risk_service.rs` - 两阶段 API 对齐
- `crates/b_data_source/src/shared_store.rs` - 序列号机制完善
- `src/actors.rs` - 废弃 run_strategy_actor()
- `src/main.rs` - 重构为启动引导
- `src/event_bus.rs` - 逐步废弃 PipelineBus

### 废弃文件（迁移完成后）
- `src/event_bus.rs` - PipelineBus（由 SharedStore 替代）
- `src/pipeline_bus.rs` - 旧流水线总线（如存在）

---

## 版本历史

| 日期 | 版本 | 变更 |
|------|------|------|
| 2026-03-30 | v1.0 | 初始路线图，基于 v3.1 设计规格 |
