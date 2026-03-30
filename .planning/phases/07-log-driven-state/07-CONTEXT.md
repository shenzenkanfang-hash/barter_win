# Phase 7: 日志驱动的系统状态研判 - Context

**Gathered:** 2026-03-30
**Status:** Ready for planning

<domain>
## Phase Boundary

设计并实现纯日志驱动的系统状态研判方案，替换当前 StateCenter 的详细状态输出。核心目标：
- StateCenter 保留（用于 EngineManager 复活检测所需的心跳感知）
- 所有详细运行状态通过 JSON Lines 日志输出
- 通过日志推断系统运行状态（数据流、策略决策、风控、交易闭环）
- 性能：单条日志写入 < 1ms，异步非阻塞

</domain>

<decisions>
## Implementation Decisions

### CheckpointLogger 处理
- **复用并扩展** `crates/a_common/src/logs/checkpoint.rs` 的 `CheckpointLogger` trait
- 保留现有 `Stage` 枚举（Indicator/Strategy/RiskPre/RiskRe/Order）
- 扩展新事件类型到 `Stage` 枚举或新枚举
- `TracingCheckpointLogger` 实现扩展，输出 JSON Lines 格式

### StateCenter 处理
- **保留 StateCenter**（`x_data` crate）
- StateCenter 仅保留存活报到功能（`report_alive` 用于 EngineManager 复活检测）
- StateCenter **不再输出详细状态**
- 详细运行状态 → JSON Lines 日志

### 日志事件类型（全部新增）
| event | 说明 | 用途 |
|-------|------|------|
| `component.started` | 组件启动 | 知道组件活了 |
| `component.stopped` | 组件停止 | 知道组件停了 |
| `health.summary` | 组件健康摘要（数据/指标层，1小时1次） | 1h间隔状态汇报 |
| `data.received` | 收到K线数据 | 数据流正常 |
| `indicator.computed` | 指标计算完成 | 计算链路正常 |
| `strategy.signal` | 策略产生信号（含决策理由） | 策略在决策 |
| `risk.check` | 风控检查结果 | 风控在工作 |
| `risk.check.skipped` | 风控跳过（pre_check 失败） | 风控降级 |
| `trade.lock.acquired` | TradeLock 获取成功 | 并发控制正常 |
| `trade.lock.skipped` | TradeLock 获取失败 | 锁竞争 |
| `order.submitted` | 订单提交 | 交易发起 |
| `order.filled` | 订单成交 | 交易闭环 |
| `order.rejected` | 订单拒绝 | 异常定位 |
| `order.cancelled` | 订单取消 | 人工干预 |
| `position.opened` | 开仓完整上下文（开发阶段详细） | 复盘训练 |
| `position.closed` | 平仓完整上下文（开发阶段详细） | 复盘训练 |
| `error.*` | 各类错误 | 异常定位 |
| `stale.detected` | 组件 stale 检测 | 复活触发 |

### 日志字段（精简）
- `timestamp` — ISO8601 时间戳（Mock回测时用K线时间戳，确保可复现）
- `level` — info/warn/error
- `component` — 组件名（data_service/indicator_service/strategy/risk/order）
- `event` — 事件类型（如上表）
- `symbol` — 交易对（可选，无则省略）
- `data` — 自由 JSON 对象（决策理由、数量、价格、错误详情等）

### 分层监控架构（新增核心决策）
**数据层 + 指标层（1小时间隔健康摘要）：**
```rust
pub struct ComponentHealth {
    pub last_tick_timestamp: i64,      // 最后处理K线时间
    pub processed_kline_count: u64,   // 已处理K线数
    pub last_compute_latency_ms: u64, // 上次计算延迟
    pub error_count: u32,              // 累计错误数
}
// 1小时输出一次到日志
{"timestamp":"2026-03-30T12:00:00Z","level":"INFO","component":"data_service","event":"health.summary","data":{"last_tick":"2026-03-30T11:59:58Z","processed":3600,"latency_ms":2,"errors":0}}
```

**引擎/策略层（关键操作详细记录）：**
- `position.opened`: 完整上下文（signal_source: ema_fast/slow/rsi/pine_color/cross_type, thresholds: profit_target/stop_loss, risk_check 各检查项, trade_lock 等待时间, order 参数, position_after）
- `position.closed`: 平仓完整上下文
- 风控触发: 检查项明细（哪项失败、阈值、当前值）
- 锁竞争: 等待时间、队列长度

### Mock回测时间戳对齐
- 数据/指标层: 用K线时间戳（而非系统时间）作为日志 timestamp
- 策略/引擎层: 关键操作实时详细记录
- 目标: 确保确定性回放，可重复执行

### 开发阶段详细模式
- 开平仓记录完整 signal_source（ema_fast/slow/rsi/pine_color/cross_type）
- 包含 thresholds（profit_target/stop_loss）
- 包含 risk_check 各项结果
- 包含 trade_lock 等待详情
- 生产模式: 可精简字段

### 日志格式
- **格式**: JSON Lines（每行一个 JSON 对象）
- **路径**: `./logs/trading.YYYYMMDD.jsonl`
- **实时性**: 行缓冲，`tail -f` 可实时读取

### 异步非阻塞实现
- 使用 **tracing buffered Writer** 方案
- `tracing-subscriber` 的 `EnvFilter` + `nonblocking::Writer`
- 后台有 tokio 任务异步消费，写入文件
- 目标：单条日志 API 调用 < 1ms（不阻塞主循环）

### EngineManager 复活模式（不变）
- 保留当前 `stale_threshold_secs: 30`
- 保留指数退避重启策略
- 保留 `get_stale()` 触发复活
- StateCenter 存活报到仍然有效

### AI研判方式
- 给定历史行情文件，预定义期望事件序列
- 实际日志与期望序列比对，输出偏差报告
- 从日志统计：延迟分布、成交率、风控触发频率

### a_common::heartbeat 处理
- `heartbeat_report.json` 文件输出可**废弃或降级**
- 如果保留，仅作为最后手段的兜底，不作为主研判依据
- 日常研判完全依赖 JSON Lines 日志

### Claude's Discretion
- JSON 文件滚动策略（保留多少天？）
- 日志文件大小上限（超过自动切分？）
- AI 研判工具的实现方式（独立 CLI 还是嵌入系统？）
- error.* 事件的详细程度（堆栈 vs 简洁错误信息）

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### 现有日志系统
- `crates/a_common/src/logs/checkpoint.rs` — CheckpointLogger trait 现有实现（要扩展）
- `crates/a_common/src/lib.rs` §exports — CheckpointLogger, TracingCheckpointLogger 导出

### StateCenter 和复活机制
- `crates/x_data/src/state/center.rs` — StateCenter trait（保留，只用于存活报到）
- `crates/f_engine/src/engine_manager.rs` — EngineManager 复活逻辑（stale 检测、指数退避）
- `src/actors.rs` — StrategyActor 心跳间隔 `HEARTBEAT_INTERVAL_SECS: 10`

### 灾备恢复
- `crates/e_risk_monitor/src/persistence/disaster_recovery.rs` — 灾备恢复机制

### 架构文档
- `docs/ARCHITECTURE_OVERVIEW.md` — 系统全景图（v7.0）
- `docs/superpowers/specs/2026-03-30-event-driven-architecture-design.md` — 设计规格

### 阶段参考
- `.planning/phases/6-strategy-coroutine-autonomy/6-SUMMARY.md` — Phase 6 完成状态

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `CheckpointLogger` trait (`crates/a_common/src/logs/checkpoint.rs`): Stage enum + trait，TracingCheckpointLogger 实现 → 直接扩展
- `StageResult`: pass/fail 结构 → 可复用为新事件的 data 字段
- `CompositeCheckpointLogger`: 多 logger 组合 → 可同时写 Console + JSON Lines

### Established Patterns
- `tracing::info!/warn!/error!` 已广泛使用 → tracing_subscriber 集成
- `Stage` enum 分段式 Pipeline 日志已有模式
- 异步非阻塞已有基础（tokio runtime）

### Integration Points
- 所有组件需要调用 CheckpointLogger（新/扩展的）
- EngineManager 继续使用 StateCenter（不变）
- main.rs / components.rs 心跳初始化逻辑可能需要调整
- a_common::heartbeat 的 heartbeat_report.json 输出需要废弃或降级

### 需要新增的代码位置
- JSON Lines 文件 Writer（基于 tracing subscriber）
- 扩展 Stage 枚举或新增 Event 枚举
- 各组件的日志插桩点（strategy_service.rs、risk_service.rs、actors.rs 等）
</code_context>

<specifics>
## Specific Ideas

- 日志路径：`./logs/trading.YYYYMMDD.jsonl`
- 数据字段：自由 JSON 对象，strategy.signal 要包含"决策理由"（如 "TR_RATIO > 1, EMA12 上穿"）
- AI 研判：输入历史行情 → 预定义期望事件序列 → 实际日志比对 → 输出偏差报告
- 单条日志写入 < 1ms（异步，非阻塞）
</specifics>

<deferred>
## Deferred Ideas

- AI 研判工具的具体实现方式（独立 CLI vs 嵌入系统）— 后续讨论
- 日志文件滚动和清理策略 — 后续讨论
- a_common::heartbeat 的 heartbeat_report.json 完全废弃 — 确认无风险后再执行

</deferred>

---

*Phase: 07-log-driven-state*
*Context gathered: 2026-03-30*
