# 全项目深度扫描报告

Date: 2026-03-26
Author: Droid

---

## 【1. GlobalState 完整职责】

**实际代码中没有独立的 GlobalState**，状态管理分散在以下结构中：

### 1.1 SymbolState（品种状态）- `f_engine/src/core/state.rs`

```rust
pub struct SymbolState {
    // --- 策略绑定 ---
    bound_strategy_id: Option<String>,  // 被哪个策略绑定
    bound_at: Option<i64>,             // 绑定时间

    // --- 分钟级状态 ---
    last_1m_request_ts: i64,          // 上次分钟请求
    last_1m_ok_ts: i64,               // 上次分钟成功
    last_1m_signal_ts: i64,            // 分钟信号时间
    last_1m_signal: Option<TradingDecision>,

    // --- 日线级状态 ---
    last_daily_request_ts: i64,
    last_daily_ok_ts: i64,
    last_daily_signal: Option<TradingDecision>,

    // --- 交易锁 ---
    trade_lock: TradeLock,             // 品种级锁
}
```

### 1.2 品种状态方法

| 方法 | 职责 |
|-----|------|
| `bind_strategy(id)` | 绑定策略 → 进入 Trading |
| `unbind_strategy()` | 解绑 → 进入 Idle |
| `is_bound()` | 检查是否在交易中 |
| `trade_lock.try_lock()` | 获取品种锁（1s超时） |

### 1.3 缺失的状态管理

**问题：没有两层状态联动机制**

```
期望的流程：
日线 MACD变色 → 设置 Watching → 等待分钟级确认
                               ↓
分钟 高波动 + Watching → 开仓 → 设置 Trading
```

**实际代码：**
- MinuteTrigger 只检查 `is_bound()`，没有检查"是否 Watching"
- 没有 `SymbolState` 的 `watching` 状态

---

## 【2. 全流程步进（从Tick到下单闭环）】

```
步骤1：Tick 从哪里来？
       ↓
       - 生产：Kline1mStream.next_message() → Binance WS
       - 沙盒：StreamTickGenerator → push_tick() → DataFeeder

步骤2：Tick 进入哪个函数？
       ↓
       - main.rs 主循环只计数，没有调用引擎
       - engine.process_tick() 从未被调用 ❌

步骤3：DataFeeder 做了什么？
       ↓
       - push_tick(tick) → latest_ticks[symbol] = tick
       - ws_get_1m(symbol) → 返回最新K线

步骤4：TradingEngineV2 如何接收？
       ↓
       - 没有接收！main.rs 没有调用 check_and_trade()
       - 引擎是"被动等待调用"的，没有主动拉取

步骤5：日线触发器做什么？
       ↓
       DailyTrigger.check(symbol, price_position, is_green, is_red, state)
       - 条件：低位+转绿(做多) OR 高位+转红(做空)
       - 输出：TriggerResult { precheck_passed }

步骤6：GlobalState 如何更新？
       ↓
       - 没有更新机制
       - 日线触发只返回结果，不修改状态 ❌

步骤7：分钟触发器做什么？
       ↓
       MinuteTrigger.check(symbol, volatility_ratio, state)
       - 条件：波动率 > 5%
       - 检查：!is_bound() + symbol_count < max
       - 输出：TriggerResult

步骤8：开仓检查依赖哪两个条件？
       ↓
       - 分钟触发通过
       - 品种未绑定 (!is_bound)
       - 但缺少：品种处于 Watching 状态 ❌

步骤9：风控检查流程
       ↓
       pipeline.pre_check() → RiskChecker.pre_check()
       risk_manager.lock_check() → 二次精校

步骤10：下单发给哪个网关？
        ↓
        order_sender.execute() → gateway.place_order()
        - MockBinanceGateway（测试）
        - ShadowBinanceGateway（沙盒）

步骤11：订单状态如何更新回 GlobalState？
        ↓
        - 订单成交后 → 更新 SymbolState.trade_lock.position
        - 没有通知 WatchList 移除品种 ❌
```

---

## 【3. 完整架构图（带数据流）】

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         数据源层 (b_data_source)                         │
│  ┌─────────────────────┐         ┌─────────────────────┐              │
│  │   Kline1mStream     │         │  StreamTickGenerator│              │
│  │   (真实WS)          │         │  (沙盒回放)         │              │
│  └──────────┬──────────┘         └──────────┬──────────┘              │
│             │                               │                          │
│             └───────────────┬───────────────┘                          │
│                             ↓                                          │
│                    DataFeeder.push_tick()                              │
│                    latest_ticks[Symbol] = Tick                         │
└─────────────────────────────────────────────────────────────────────────┘
                              ↓ ws_get_1m()
┌─────────────────────────────────────────────────────────────────────────┐
│                    EngineStateHandle (全局状态)                         │
│  ┌─────────────────────────────────────────────────────────────────┐  │
│  │ symbols: FnvHashMap<String, SymbolState>                        │  │
│  │ - bound_strategy_id: 绑定策略                                    │  │
│  │ - trade_lock: 品种锁                                            │  │
│  │ - last_1m_signal: 分钟信号缓存                                   │  │
│  │ - last_daily_signal: 日线信号缓存                                │  │
│  └─────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────────────┐
│                    TradingEngineV2.check_and_trade()                    │
│                              ↓                                          │
│  ┌─────────────────────────────────────────────────────────────────┐  │
│  │ TriggerManager                                                   │  │
│  │   ┌─────────────────────┐    ┌─────────────────────┐           │  │
│  │   │  DailyTrigger       │    │  MinuteTrigger      │           │  │
│  │   │  - MACD变色检测      │    │  - 波动率 > 5%     │           │  │
│  │   │  - 低位+绿→做多     │    │  - 未绑定 + 未超限  │           │  │
│  │   │  - 高位+红→做空     │    │                     │           │  │
│  │   └─────────────────────┘    └─────────────────────┘           │  │
│  └─────────────────────────────────────────────────────────────────┘  │
│                              ↓                                          │
│  ┌─────────────────────────────────────────────────────────────────┐  │
│  │ TradingPipeline                                                  │  │
│  │   build_strategy_query() → 策略自己拉取K线                       │  │
│  │   execute_strategy() → TradingDecision                          │  │
│  └─────────────────────────────────────────────────────────────────┘  │
│                              ↓                                          │
│  ┌─────────────────────────────────────────────────────────────────┐  │
│  │ RiskManager                                                     │  │
│  │   pre_check() → 锁外预检                                        │  │
│  │   lock_check() → 锁内精校                                       │  │
│  └─────────────────────────────────────────────────────────────────┘  │
│                              ↓                                          │
│  ┌─────────────────────────────────────────────────────────────────┐  │
│  │ fund_pool.freeze() → 冻结资金                                   │  │
│  │ order_sender.execute() → 下单                                   │  │
│  └─────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────────────┐
│                         网关层                                          │
│  ┌─────────────────────┐         ┌─────────────────────┐              │
│  │ MockBinanceGateway  │         │ ShadowBinanceGateway│              │
│  │ (测试)              │         │ (沙盒)              │              │
│  │ - place_order()     │         │ - place_order()     │              │
│  │ - get_account()     │         │ - get_account()     │              │
│  │ - get_position()    │         │ - get_position()    │              │
│  └─────────────────────┘         └─────────────────────┘              │
└─────────────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────────────┐
│                         沙盒层 (h_sandbox)                              │
│  ┌─────────────────────────────────────────────────────────────────┐  │
│  │ OrderEngine (simulator/)                                        │  │
│  │   - apply_open() → 更新持仓                                     │  │
│  │   - apply_close() → 平仓                                        │  │
│  │   - deduct_fee() → 手续费                                       │  │
│  └─────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 【4. 代码中所有问题、缺失、疑问、BUG】

### 4.1 缺失的函数/逻辑

| # | 问题 | 影响 |
|---|------|------|
| 1 | **没有两层状态联动** | 日线触发不更新 Watching 状态，分钟级无法确认品种是否"等待确认" |
| 2 | **SymbolState 缺少 Watching 状态** | 只有 Idle/Bound，没有"已触发等待确认" |
| 3 | **main.rs 未调用引擎** | `Kline1mStream` 订阅后只计数，`check_and_trade()` 从未调用 |
| 4 | **ShadowBinanceGateway 未实现 trait** | 方法签名匹配但没有 `impl ExchangeGateway for ShadowBinanceGateway` |
| 5 | **沙盒两端未串联** | StreamTickGenerator 和 ShadowBinanceGateway 是独立的，没有一起工作 |

### 4.2 命名不清晰

| # | 问题 | 建议 |
|---|------|------|
| 1 | `process_tick()` | 改为 `check_and_trade()` |
| 2 | `Tick` 结构 | 考虑简化为直接用 `KLine` |
| 3 | `SymbolState.is_bound()` | `is_trading()` 更直观 |

### 4.3 结构冗余

| # | 问题 |
|---|------|
| 1 | `Tick { kline_1m: Option<KLine> }` - 只是 K 线包装器 |
| 2 | `b_data_source` 和 `x_data` 都有 `Tick` 类型 |

### 4.4 层与层未连接

| # | 问题 |
|---|------|
| 1 | `Kline1mStream` → `DataFeeder` 未连接 |
| 2 | `DataFeeder` → `TradingEngineV2` 未连接 |
| 3 | `TradingEngineV2` → `ShadowBinanceGateway` 缺少 trait impl |

### 4.5 日线/分钟未联动

| # | 问题 |
|---|------|
| 1 | 日线触发 `DailyTrigger.check()` 只返回 TriggerResult |
| 2 | 没有机制：日线通过 → 设置品种为 Watching |
| 3 | 分钟触发 `MinuteTrigger.check()` 不检查 Watching 状态 |

### 4.6 沙盒与实盘不一致

| # | 问题 |
|---|------|
| 1 | 实盘 main.rs：K线订阅 和 引擎 是分离的 |
| 2 | 沙盒 sandbox_main：有 StreamTickGenerator 但引擎未接入 |
| 3 | 沙盒两端拦截模式：代码存在但未串联 |

### 4.7 未验证的疑问

| # | 疑问 |
|---|------|
| 1 | 策略层从哪里拉取 K 线？通过 DataFeeder 吗？ |
| 2 | VolatilityManager 的 volatility_ratio 如何计算？ |
| 3 | 触发器的 `max_running_symbols` 如何控制？ |
| 4 | 日线触发后，品种何时从 Watching 变为 Idle？ |

---

## 问题汇总表

| 优先级 | 问题 | 状态 |
|-------|------|------|
| P0 | main.rs 未调用 check_and_trade() | 未修复 |
| P0 | 两层状态未联动 (Watching) | 未实现 |
| P1 | ShadowBinanceGateway 未实现 trait | 未修复 |
| P1 | 沙盒两端未串联 | 未实现 |
| P2 | process_tick 命名不准确 | 未修复 |
| P2 | Tick 结构冗余 | 未修复 |
| P3 | 日线触发后无状态更新 | 未实现 |

---

## 结论

项目核心流程清晰，但状态管理和层间连接有缺失。需要先修复 P0 问题。

---

## 更新记录

| 日期 | 更新内容 |
|-----|---------|
| 2026-03-26 | 初始版本，完整扫描报告 |
| 2026-03-26 | 新增 TradeManager 架构（精简版引擎） |

---

## 【5. TradeManager 架构（最终版）】

### 5.1 架构原则

**四层自运行架构：**
- 数据源层：后台自运行，接收数据存入 DataFeeder
- 指标层：后台自运行，从 DataFeeder 获取数据计算指标
- 引擎层：监控波动率，不监控价格
- 策略层：每个任务独立，从 DataFeeder/IndicatorCache 获取数据

### 5.2 四层架构

```
┌─────────────────────────────────────────────────────────────────────┐
│  数据源层（后台自运行）                                            │
│  StreamTickGenerator → push_tick → DataFeeder                     │
└─────────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────────┐
│  指标层（后台自运行）                                              │
│  从 DataFeeder 获取 K 线 → 计算指标 → IndicatorCache              │
└─────────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────────┐
│  引擎层（监控波动率触发任务）                                       │
│  监控波动率 → 波动率 > 阈值 → spawn_task                         │
└─────────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────────┐
│  策略层（每个任务独立）                                            │
│  从 DataFeeder 获取价格                                            │
│  从 IndicatorCache 获取指标                                        │
│  策略计算 → 风控 → 下单                                            │
└─────────────────────────────────────────────────────────────────────┘
```

**Engine 结构：**

```rust
pub struct Engine {
    tasks: Arc<TokioRwLock<FnvHashMap<String, Arc<TokioRwLock<TaskState>>>>>,
    db: EngineDb,
    heartbeat_timeout: i64,
}
```

### 5.3 品种层（TaskState）

```
TaskState
├── 自己的循环间隔 (interval_ms)
├── 自己的运行状态 (status: Running/Stopped/Ended)
├── 自己的心跳 (last_beat)
├── 自己的持仓信息
└── 自己决定是否结束
```

**TaskState 结构：**

```rust
pub struct TaskState {
    symbol: String,
    status: RunningStatus,
    last_beat: i64,
    position_qty: Decimal,
    position_price: Decimal,
    forbid_until: Option<i64>,
    trade_count: u32,
    done_reason: Option<String>,
    interval_ms: u64,
}
```

### 5.4 执行流程

```
┌─────────────────────────────────────────────────────────────────────┐
│  数据源层（自运行）                                                │
│  StreamTickGenerator → push_tick → DataFeeder                     │
└─────────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────────┐
│  指标层（自运行）                                                  │
│  从 DataFeeder 获取 K 线 → 计算指标 → IndicatorCache              │
└─────────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────────┐
│  引擎层 (Engine)                                                    │
│  ├─ 监控波动率 (check_triggers) → 波动率 > 阈值 → spawn_task     │
│  ├─ 心跳检查 (check_heartbeat)                                     │
│  ├─ 任务移除 (check_tasks) - 发现 Ended 则移除                    │
│  └─ 持久化 (EngineDb) - 只在任务变化时持久化                       │
└─────────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────────┐
│  策略层（每个任务独立）                                            │
│  ├─ 从 DataFeeder 获取价格                                         │
│  ├─ 从 IndicatorCache 获取指标                                     │
│  ├─ 策略计算                                                        │
│  ├─ 风控检查                                                        │
│  ├─ 下单执行                                                        │
│  └─ 平仓完成 → 设置 status=Ended → 自己退出                         │
└─────────────────────────────────────────────────────────────────────┘
```

### 5.5 引擎 vs 策略层职责划分

| 功能 | 引擎层 | 策略层 |
|-----|-------|--------|
| 波动率监控 | ✅ | ❌ |
| 触发任务 | ✅ | ❌ |
| 任务注册/移除 | ✅ | ❌ |
| 心跳检查 | ✅ | ❌ |
| 持久化 | ✅ | ❌ |
| 获取价格 | ❌ | ✅ 从 DataFeeder |
| 获取指标 | ❌ | ✅ 从 IndicatorCache |
| 策略计算 | ❌ | ✅ |
| 风控检查 | ❌ | ✅ |
| 下单执行 | ❌ | ✅ |

### 5.6 异步任务模式

**概念：**
- 不是子线程，是 `tokio::spawn` 的协程
- 引擎 spawn 任务，任务自己循环
- 任务结束自己退出，引擎发现后移除

**代码示例：**

```rust
// 引擎启动任务
impl Engine {
    pub async fn spawn_task(&self, symbol: String, interval_ms: u64) {
        let state = Arc::new(TokioRwLock::new(TaskState::new(...)));
        self.tasks.write().await.insert(symbol.clone(), Arc::clone(&state));
        self.db.persist_task_created(&symbol, interval_ms);
        
        tokio::spawn(async move {
            // 任务自己循环
            loop {
                // 策略 + 风控 + 下单
                match execute_once(&symbol, &state).await {
                    Ok(TradingComplete) => {
                        state.write().await.end("TradeComplete");
                        break;
                    }
                    _ => {}
                }
                sleep(Duration::from_millis(interval_ms)).await;
            }
        });
    }
}
```

### 5.7 心跳机制

**品种层：**
```rust
impl TaskState {
    pub fn heartbeat(&mut self) {
        self.last_beat = Utc::now().timestamp();
    }
}
```

**引擎层：**
```rust
impl Engine {
    async fn check_heartbeat(&self) {
        let now = Utc::now().timestamp();
        for (symbol, state) in self.tasks.read().await.iter() {
            if state.read().await.last_beat < now - 90 {
                tracing::warn!("Task heartbeat timeout: {}", symbol);
            }
        }
    }
}
```

### 5.8 持久化策略

**只在任务变化时持久化：**

| 操作 | 持久化 |
|-----|--------|
| 创建任务 | ✅ symbol, interval |
| 任务结束 | ✅ reason, forbid_until |
| 普通执行 | ❌ |
| 心跳更新 | ❌ |

### 5.9 与 Python TradeManager 对应

| Python TradeManager | Rust Engine |
|--------------------|-------------|
| `instances` | `tasks` |
| `start_instance()` | `spawn_task()` |
| `run_once()` | 策略层自己的循环 |
| `monitor()` | `check_heartbeat()` |
| `heartbeat()` | `last_beat` |
| `scan_volatile()` | `check_triggers()` |

### 5.10 实现文件

- `crates/f_engine/src/core/engine.rs` - Engine, TaskState, RunningStatus
- `crates/f_engine/src/core/mod.rs` - 模块导出
