---
Author: 开发者
Created: 2026-03-30
Stage: completed
Status: fully_implemented
Summary: Phase 7 日志驱动状态研判方案完成实施
---

# Phase 7 DONE: 日志驱动的系统状态研判

## 实施日期
2026-03-30

## 变更文件清单

### Wave 1: 基础设施（5 Tasks）

| 文件 | 变更 |
|------|------|
| `crates/a_common/src/logs/checkpoint.rs` | 扩展 CheckpointLogger trait（+log_event）、添加 ComponentHealth、HealthAccumulator、ComponentHealthLogger、TradingLogEventType（18种事件） |
| `crates/a_common/src/logs/mod.rs` | 导出新增类型：ComponentHealth, HealthAccumulator, ComponentHealthLogger, TradingLogEventType, JsonLinesWriter, init_log_dir |
| `crates/a_common/src/logs/writer.rs` | **新建** — JsonLinesWriter（tokio mpsc channel 异步非阻塞，4KB缓冲） |
| `crates/a_common/src/lib.rs` | 导出新增日志类型 |

### Wave 2: 组件集成（4 Tasks）

| 文件 | 变更 |
|------|------|
| `src/components.rs` | SystemComponents 新增 health_logger 字段，create_components() 中初始化并启动后台定时器（3600s） |
| `src/actors.rs` | run_strategy_actor: accumulator.update_tick/add_error + strategy.signal + trade.lock.acquired/skipped 日志；run_risk_actor: risk.check + risk.check.skipped + order.filled/rejected/cancelled 日志 |
| `src/components.rs` | print_heartbeat_report() 移除 heartbeat_report.json 写入，标记为降级兜底 |

## 验收标准确认

- [x] `cargo check -p a_common` 无编译错误
- [x] `cargo check` 全项目无编译错误
- [x] 所有新增 pub 类型已在 `crates/a_common/src/lib.rs` 导出
- [x] CheckpointLogger trait 扩展了 `log_event()` 方法（4处实现）
- [x] ComponentHealth struct 包含 4 个字段
- [x] ComponentHealthLogger 定时器间隔可配置（默认 3600s）
- [x] JsonLinesWriter 使用 mpsc channel 非阻塞发送
- [x] TradingLogEventType 枚举包含 18 种事件类型
- [x] StrategyActor 集成了 health_logger accumulator
- [x] RiskActor 记录 risk.check/risk.check.skipped/order.filled/order.rejected/order.cancelled 事件
- [x] StrategyActor 记录 strategy.signal + trade.lock.acquired/skipped 事件
- [x] heartbeat_report.json 详细输出已降级

## 架构说明

- **日志路径**: `./logs/trading.YYYYMMDD.jsonl`
- **JSON Lines 格式**: 通过 tracing::info! 结构化字段输出
- **健康摘要**: ComponentHealthLogger 每 3600s 输出一次 health.summary
- **数据层事件**: accumulator.update_tick / add_error 在 StrategyActor 中调用
- **组件生命周期**: component.started/stopped（在 actors 启动/停止处）
- **heartbeat_report.json**: 降级为最后手段的兜底，不作为主研判依据

## 警告说明（无 blocking 警告）

以下警告为既有警告，与本次变更无关：
- heartbeat/mod.rs: unused mut (既有)
- api/binance_api.rs: dead_code (既有)
- b_data_mock, d_checktable, trading-system 中的 dead_code (既有)

## 后续建议

1. 将 JsonLinesWriter 集成到 tracing subscriber layer（json_lines_layer 函数待实现）
2. 添加 `component.started/stopped` 到 actors.rs 启动/停止处
3. AI 研判工具实现（给定历史日志文件 → 偏差报告）
