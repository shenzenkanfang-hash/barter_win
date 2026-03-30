================================================================================
ARCHITECTURE_OVERVIEW.md - Barter-Rs 系统全景图 v7.0
================================================================================
Author: Claude Code
Created: 2026-03-30
Version: 7.0 (事件驱动协程自治架构)
================================================================================

# Barter-Rs 量化交易系统全景图

================================================================================
第一层：物理结构图
================================================================================

项目位于 D:\RusProject\barter-rs-main\，是一个 Cargo Workspace 多 crate 项目，
采用单入口二进制 + 多 crate 库的物理组织方式。

## 顶层目录

```
barter-rs-main/
  Cargo.toml              # Workspace 定义，8 个成员 crate
  rustfmt.toml            # 格式化配置
  src/main.rs             # 唯一程序入口（Trading System v7.0）
  crates/                 # 所有 crate（a_common 到 x_data 覆盖全链路）
    a_common/             # 基础设施层
    b_data_source/        # 真实市场数据
    b_data_mock/          # 模拟/沙盒数据（镜像实现）
    c_data_process/       # 指标处理
    d_checktable/         # 策略检查（业务核心）
    e_risk_monitor/       # 风控层
    f_engine/             # 交易引擎核心
    x_data/               # 业务数据类型抽象
  docs/                   # 设计文档
  data/                   # 运行时数据（SQLite、历史 K 线 CSV）
  deploy/                 # 部署配置
  .planning/              # 项目管理文档
```

## Crate 依赖关系图

```
a_common (基础设施层，无业务依赖)
    ↑
x_data (业务数据类型抽象层)
    ↑
    ├── b_data_source (数据采集层)
    ├── b_data_mock (模拟数据层)
    ├── c_data_process (指标处理层)
    ├── d_checktable (策略检查层)
    ├── e_risk_monitor (风控层)
    └── f_engine (交易引擎层)
```

## 主入口结构 (src/main.rs)

```rust
// 模块定义
mod components;     // 系统组件创建
mod pipeline;       // 事件驱动流水线
mod tick_context;   // 上下文配置
mod utils;          // 工具函数
mod event_bus;      // PipelineBus 事件总线
mod actors;         // StrategyActor + RiskActor

// 主流程
#[tokio::main]
async fn main() {
    // 1. 创建组件 (SystemComponents + DataLayer)
    // 2. 创建 PipelineBus
    // 3. run_pipeline() - 启动事件驱动流水线
    // 4. 打印心跳报告
}
```

## src/ 目录结构

```
src/
  main.rs             # 程序入口 (v7.0)
  components.rs      # 系统组件创建 (create_components)
  pipeline.rs        # 事件驱动流水线 (run_pipeline)
  event_bus.rs       # PipelineBus 事件总线
  actors.rs          # StrategyActor + RiskActor 协程
  tick_context.rs    # 上下文配置 (SYMBOL, DATA_FILE 等)
  utils.rs           # 工具函数 (parse_raw_kline 等)
```

## 关键 Crate 内物理结构

### a_common（最底层，纯粹的与业务无关的基础设施）

```
a_common/src/
  lib.rs              # 模块根，re-export 所有组件
  api/                # REST API 网关（BinanceApiGateway）
  ws/                 # WebSocket 连接器（BinanceWsConnector）
  config/             # 平台检测 + 路径常量
  models/             # 通用数据模型（OrderStatus, Side, OrderType）
  claint/             # 错误类型（MarketError, EngineError）
  backup/             # 高速内存备份系统（MemoryBackup）
  heartbeat/          # 心跳监控系统
  volatility/         # 波动率计算
```

### b_data_source（数据入口）

```
b_data_source/src/
  lib.rs
  shared_store.rs     # SharedStore<K,V> 带版本号的共享存储
  store/
    store_trait.rs    # MarketDataStore trait（跨 crate 共享接口）
    memory_store.rs  # 实时分区（当前 K 线/订单簿）
    history_store.rs # 历史分区（已闭合 K 线）
    volatility.rs    # 波动率管理器
    pipeline_store.rs# PipelineStore 封装
  ws/kline_1m/        # 1 分钟 K 线合成
```

### b_data_mock（模拟数据）

```
b_data_mock/src/
  api/
    mock_account.rs   # MockApiGateway（模拟交易所）
    mock_config.rs   # 模拟配置
  replay_source.rs   # 历史数据回放（从 CSV）
  store/
    market_data_store_impl.rs # MarketDataStoreImpl
  ws/kline_1m/
    ws.rs             # Kline1mStream（被动接口 next_message()）
```

### d_checktable（策略层）

```
d_checktable/src/
  lib.rs
  strategy_service.rs # StrategyService trait
  h_15m/
    strategy_service.rs # H15mStrategyService
    trader.rs           # Trader 主体（execute_once_wal）
    executor.rs        # 执行器
    repository.rs      # 状态持久化（SQLite）
    signal.rs          # 信号定义
    quantity_calculator.rs # 数量计算
```

### e_risk_monitor（风控层）

```
e_risk_monitor/src/
  lib.rs
  risk_service.rs     # RiskService trait (pre_check, final_check)
  risk/common/
    risk_pre_checker.rs # RiskPreChecker（余额/持仓检查）
    order_checker.rs   # OrderCheck（订单参数检查）
  trade_lock.rs        # TradeLock（全局交易锁）
  position/
    position_manager.rs # LocalPositionManager
```

### x_data（状态中心）

```
x_data/src/
  lib.rs
  state/
    center.rs         # StateCenter trait + StateCenterImpl
    component.rs      # ComponentState, ComponentStatus
  market/
    kline.rs          # K 线数据类型
    tick.rs           # Tick 数据类型
  position/
    types.rs          # 持仓类型
  account/
    types.rs          # 账户类型
```

## 关键设计：a_common 铁律

a_common 被所有其他 crate 依赖，因此它绝对禁止包含任何业务类型。

a_common 禁止包含：
  - TradingDecision, OrderRequest (f_engine 业务类型)
  - ExecutionResult (d_checktable 业务类型)
  - LocalPosition (e_risk_monitor 业务类型)

a_common 允许包含：
  - MarketError, EngineError（纯基础设施错误）
  - MemoryBackup 基础设施
  - Heartbeat 心跳监控


================================================================================
第二层：六层架构逻辑图
================================================================================

系统的逻辑分层严格遵循"只能依赖下层"的原则，层与层之间通过 trait 接口解耦。

## 分层依赖链（底部 到 顶部）

  a_common (基础设施层)
       |
       v
  x_data (业务数据类型抽象层)
       |
       v
  b_data_source / b_data_mock (数据采集层)
       |
       v
  c_data_process (指标处理层)
       |
       v
  d_checktable (策略检查层)
       |
       v
  e_risk_monitor (风控层)
       |
       v
  f_engine (交易引擎核心)
       |
       v
  src/main.rs (主入口，事件驱动协程)

## 各层职责

### a_common（基础设施层）

提供：API 网关 / WS 连接器 / 内存备份 / 心跳监控 / 波动率计算
特征：无任何业务依赖，是所有其他层的工具箱

### x_data（业务数据类型抽象层）

提供：Tick / KLine / Position / Account 统一业务类型
提供：StateCenter trait（组件状态中心）
意义：消除跨模块类型重复定义

### b_data_source / b_data_mock（数据采集层）

提供：MarketDataStore trait（统一存储接口）
提供：SharedStore<K,V> trait（带版本号的通用存储）
提供：Kline1mStream（被动接口，仅暴露 next_message()）
职责：K 线合成 / 数据存储 / 历史回放
特征：数据层被动设计，不主动驱动

### c_data_process（指标处理层）

提供：SignalProcessor（指标管理器）
实现：Pine Indicator 完整版（EMA/RSI/MACD）
算法：O(1) 增量计算（无全量重算）

### d_checktable（策略检查层）

提供：StrategyService trait（策略协程统一接口）
提供：Trader（品种交易主循环 execute_once_wal）
职责：决策表执行 / 状态管理 / 交易记录
子策略：h_15m（高频 15 分钟）

### e_risk_monitor（风控层）

提供：RiskService trait（两阶段风控检查）
提供：RiskPreChecker（预风控检查）
提供：TradeLock（全局交易锁，串行执行）
职责：pre_check() 前置检查 / final_check() 成交后复核

### f_engine（交易引擎核心）

提供：MockApiGateway（模拟交易所网关）
职责：订单执行 / 持仓更新

### src/main.rs（事件驱动协程层）

提供：run_pipeline() 启动函数
提供：run_strategy_actor() 策略协程（主动驱动）
提供：run_risk_actor() 风控协程（被动消费）
架构：StrategyActor（主动） + RiskActor（被动） + PipelineBus（信号总线）


================================================================================
第三层：数据生命周期图
================================================================================

描述一根 K 线数据从产生到触发订单的全过程。

## v7.0 事件驱动架构的数据流

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         事件驱动协程架构                                  │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  [数据层]                 [策略层]              [风控层]                 │
│  Kline1mStream      →   StrategyActor    →   RiskActor                │
│  (被动接口)              (主动驱动)           (被动消费)                 │
│       ↑                       ↓                    ↓                     │
│       │                  PipelineBus ←────────────┘                     │
│       │                  (策略信号)                                    │
│       │                       ↓                                        │
│       │                  PipelineBus.order_tx                          │
│       │                       ↓                                        │
│       └──────────────────→ [订单事件]                                   │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

## 完整数据生命周期

### 1. 数据产生（被动接口）

b_data_mock::ws::kline_1m::Kline1mStream
    - 被动接口：next_message() 被 StrategyActor 调用
    - 数据来源：ReplaySource（CSV 回放）或 WebSocket

### 2. 数据验证

src/actors.rs::run_strategy_actor
    - Stage B: 验证 kline.close > Decimal::ZERO

### 3. 价格更新（网关）

src/components.rs
    - components.gateway.update_price(SYMBOL, kline.close)

### 4. 指标计算

c_data_process::SignalProcessor
    - min_update(SYMBOL, high, low, close, volume)
    - O(1) 增量计算：EMA / RSI / MACD / PineColor

### 5. 策略执行（带 TradeLock）

d_checktable::h_15m::Trader
    - execute_once_wal() 返回 ExecutionResult
    - 锁冲突时发送 Skip 信号

### 6. 策略信号发送

src/event_bus.rs::PipelineBus
    - StrategySignalEvent { tick_id, decision, qty, reason }
    - 通过 broadcast::Sender 发送到 PipelineBus

### 7. 风控检查

e_risk_monitor::risk::common
    - risk_checker.pre_check() 余额检查
    - order_checker.pre_check() 订单检查

### 8. 订单提交

f_engine::MockApiGateway
    - place_order() 模拟成交

### 9. 订单事件发送

src/event_bus.rs::PipelineBus
    - OrderEvent { order_id, status, filled_price, filled_qty }

## 数据形态变化（沿链路）

| 阶段 | 数据形态           | 示例                                                    |
|------|------------------|--------------------------------------------------------|
| 1. 原始 | CSV/WS 消息      | 从 data/ 目录读取                                       |
| 2. K线 | KlineData        | { open, close, high, low, volume, is_closed }         |
| 3. 验证 | bool             | close > ZERO                                          |
| 4. 指标 | IndicatorCache   | { ema_fast, ema_slow, rsi, pine_color }               |
| 5. 信号 | ExecutionResult  | Executed { qty } / Skipped { reason } / Failed { e } |
| 6. 事件 | StrategySignalEvent | { decision: LongEntry, qty, reason }                |
| 7. 风控 | PreCheckResult   | { passed: bool, risk_level }                          |
| 8. 订单 | OrderResult      | { order_id, filled_price, filled_qty }               |
| 9. 结果 | OrderEvent       | { status: Filled/Rejected/Cancelled }                |


================================================================================
第四层：执行时序与并发模型
================================================================================

## v7.0 核心架构：事件驱动协程自治

```
src/main.rs
    │
    ├── create_components() → (SystemComponents, DataLayer)
    │
    ├── PipelineBus::new(128, 128) → (PipelineBusHandle, PipelineBus)
    │
    └── run_pipeline(components, data_layer, bus)
            │
            ├── stop_tx = broadcast::channel()
            │
            ├── tokio::spawn(run_strategy_actor(
            │       data_layer,     // 非 Send，单独传入
            │       components.clone(), // Send-safe
            │       bus_handle.clone(),
            │       stop_rx
            │   ))
            │
            └── tokio::spawn(run_risk_actor(
                    bus_receiver.receiver,
                    bus_handle,
                    components,
                    stop_rx
                ))
```

## StrategyActor（主动驱动方）

位置：src/actors.rs::run_strategy_actor

设计原则：
  - 拥有自己的循环，主动从数据层拉取数据
  - 调用 kline_stream.next_message()（数据层被动接口）
  - 调用 signal_processor.min_update()
  - 调用 trader.execute_once_wal()（带 TradeLock）
  - 通过 PipelineBus.strategy_tx 发送信号

循环结构：

```rust
loop {
    tokio::select! {
        biased;

        _ = stop_rx.recv() => { break; }           // 停止信号
        _ = heartbeat_tick.tick() => { ... }        // 心跳报到
        _ = sleep(Duration::from_millis(50)) => {  // 50ms 间隔
            // 1. 从数据层拉取 K 线
            let kline_data = {
                let mut stream = data_layer.kline_stream.lock().await;
                stream.next_message()
            };

            // 2. 解析和验证
            let kline = parse_raw_kline(&data)?;
            let valid = kline.close > Decimal::ZERO;

            // 3. 更新价格
            components.gateway.update_price(SYMBOL, kline.close);

            // 4. 更新指标
            components.signal_processor.min_update(...);

            // 5. 策略执行（带 TradeLock）
            let trade_result = {
                let guard = components.trade_lock.acquire("h_15m_strategy")?;
                let r = components.trader.execute_once_wal().await;
                drop(guard);
                r
            };

            // 6. 发送策略信号到 PipelineBus
            let signal = StrategySignalEvent { ... };
            bus_handle.send_strategy_signal(signal)?;
        }
    }
}
```

## RiskActor（被动消费者）

位置：src/actors.rs::run_risk_actor

设计原则：
  - 等待 PipelineBus.strategy_rx 收到信号
  - 执行风控检查和下单
  - 通过 PipelineBus.order_tx 发送订单结果

循环结构：

```rust
loop {
    tokio::select! {
        biased;

        _ = stop_rx.recv() => { break; }
        Ok(signal) = strategy_rx.recv() => {
            // 1. 检查是否有下单数量
            let Some(qty) = signal.qty else { continue; };

            // 2. 余额风控检查
            let balance_passed = components.risk_checker.pre_check(...).is_ok();

            // 3. 订单风控检查
            let order_passed = components.order_checker.pre_check(...).passed;

            // 4. 下单或拒绝
            if balance_passed && order_passed {
                let order = components.gateway.place_order(...)?;
                bus_handle.send_order(OrderEvent { status: Filled }).await;
            } else {
                bus_handle.send_order(OrderEvent { status: Cancelled }).await;
            }
        }
    }
}
```

## PipelineBus（跨协程信号通道）

位置：src/event_bus.rs

设计原则：
  - 仅传递策略信号和订单事件（不含原始数据）
  - 使用 broadcast::channel（Send-safe）
  - 不使用 watch::channel（RwLockReadGuard 非 Send）

```rust
pub struct PipelineBusHandle {
    pub strategy_tx: broadcast::Sender<StrategySignalEvent>,
    pub order_tx: mpsc::Sender<OrderEvent>,
}

pub struct PipelineBusReceiver {
    pub strategy_rx: broadcast::Receiver<StrategySignalEvent>,
}
```

## Send 约束边界

### 问题：Kline1mStream 非 Send

Kline1mStream 内部使用 rand::ThreadRng（Rc<UnsafeCell<...>>），非 Send。

### 解决方案：独立 block 释放锁

```rust
// guard 在此 block 结束时立即释放，不跨越 await
let kline_data = {
    let mut stream = data_layer.kline_stream.lock().await;
    stream.next_message()
};
// guard 已释放，可以安全 await
```

### 问题：Stop signal 非 Send

watch::Receiver 持有 std::sync::RwLockReadGuard，非 Send。

### 解决方案：使用 broadcast::Receiver

broadcast::Receiver 是 Send-safe，用于 stop signal。

## 并发模型总结

| 组件 | 角色 | 驱动方式 | Send 约束 |
|------|------|---------|-----------|
| StrategyActor | 主动驱动 | 自己循环拉取数据 | components 跨 await |
| RiskActor | 被动消费 | 等待 PipelineBus 信号 | components 跨 await |
| Kline1mStream | 被动接口 | 被 StrategyActor 调用 | 非 Send，block 内使用 |
| PipelineBus | 信号总线 | 跨协程传递信号 | broadcast::Sender/Receiver Send-safe |


================================================================================
第五层：接口契约图
================================================================================

## 核心 Trait 接口

### 1. MarketDataStore trait（数据层 -> 策略/指标层）

位置：b_data_source/src/store/store_trait.rs

```rust
pub trait MarketDataStore: Send + Sync {
    fn write_kline(&self, symbol: &str, kline: &KlineData, is_closed: bool);
    fn get_current_kline(&self, symbol: &str) -> Option<KlineData>;
    fn get_history_klines(&self, symbol: &str) -> Vec<KlineData>;
    fn get_volatility(&self, symbol: &str) -> Option<VolatilityData>;
    fn preload_klines(&self, symbol: &str, klines: Vec<StoreKline>);
}
```

### 2. SharedStore<K, V> trait（通用共享存储）

位置：b_data_source/src/shared_store.rs

```rust
pub trait SharedStore<K, V>: Send + Sync
where
    K: Eq + Hash + Clone + Send + Sync,
    V: Clone + Send + Sync,
{
    fn get(&self, key: &K) -> Option<VersionedData<V>>;
    fn version(&self, key: &K) -> u64;
    fn write(&self, key: K, value: V, timestamp_ms: i64);
    fn get_since(&self, key: &K, min_seq: u64) -> Vec<VersionedData<V>>;
    fn global_version(&self) -> u64;
}
```

### 3. StateCenter trait（组件状态中心）

位置：x_data/src/state/center.rs

```rust
#[async_trait]
pub trait StateCenter: Send + Sync {
    fn register(&self, component_id: String);
    fn report_alive(&self, component_id: &str) -> Result<(), StateCenterError>;
    fn report_error(&self, component_id: &str, error: &str) -> Result<(), StateCenterError>;
    fn get(&self, component_id: &str) -> Option<ComponentState>;
    fn get_alive(&self, timeout_secs: i64) -> Vec<ComponentState>;
    fn get_stale(&self, threshold_secs: i64) -> Vec<ComponentState>;
}
```

### 4. RiskService trait（风控服务）

位置：e_risk_monitor/src/risk_service.rs

```rust
#[async_trait]
pub trait RiskService: Send + Sync {
    fn name(&self) -> &str;
    async fn pre_check(&self, request: PreCheckRequest) -> Result<PreCheckResult, RiskServiceError>;
    async fn final_check(&self, request: FinalCheckRequest) -> Result<FinalCheckResult, RiskServiceError>;
    async fn freeze(&self, order_id: &str, amount: Decimal) -> Result<(), RiskServiceError>;
    async fn confirm(&self, order_id: &str, fill_value: Decimal) -> Result<(), RiskServiceError>;
}
```

### 5. StrategyService trait（策略服务）

位置：d_checktable/src/strategy_service.rs

```rust
#[async_trait]
pub trait StrategyService: Send + Sync {
    fn strategy_info(&self) -> StrategyInfo;
    async fn start(&self) -> Result<(), StrategyServiceError>;
    async fn stop(&self) -> Result<(), StrategyServiceError>;
    async fn health_check(&self) -> Result<StrategyHealth, StrategyServiceError>;
}
```

### 6. TradeLock（全局交易锁）

位置：e_risk_monitor/src/trade_lock.rs

```rust
pub struct TradeLock { ... }

impl TradeLock {
    pub fn new() -> Arc<Self>;
    pub fn acquire(&self, strategy_id: &str) -> Result<TradeLockGuard, TradeLockError>;
}
```

## SystemComponents（系统组件集合）

位置：src/components.rs

```rust
#[derive(Clone)]
pub struct SystemComponents {
    pub signal_processor: Arc<SignalProcessor>,  // 指标处理
    pub trader: Arc<Trader>,                    // 交易执行
    pub risk_checker: Arc<RiskPreChecker>,      // 余额风控
    pub order_checker: Arc<OrderCheck>,        // 订单风控
    pub gateway: Arc<MockApiGateway>,           // 交易所网关
    pub pipeline_store: Arc<PipelineStore>,     // 流水线存储
    pub trade_lock: Arc<TradeLock>,             // 交易锁
}

// Send-safe：所有字段都实现 Send + Sync
```

## DataLayer（数据层，非 Send）

```rust
#[derive(Clone)]
pub struct DataLayer {
    pub kline_stream: Arc<tokio::sync::Mutex<Kline1mStream>>,
}

// 非 Send：Kline1mStream 内部含 ThreadRng
// 仅在 StrategyActor 内局部使用，不跨 await
```

## 接口调用方式总结

| 接口 | 位置 | 调用方式 | 同步/异步 |
|-----|------|---------|----------|
| MarketDataStore | b_data_source | trait method | 同步 |
| SharedStore | b_data_source | trait method | 同步 |
| StateCenter | x_data | trait method | 同步 |
| RiskService | e_risk_monitor | async trait | 异步 |
| StrategyService | d_checktable | async trait | 异步 |
| TradeLock | e_risk_monitor | sync method | 同步 |


================================================================================
第六层：状态与存储图
================================================================================

## 状态分布

系统状态分散在多个组件中，通过共享存储单例实现跨组件数据交换。

## 存储单例 1：MarketDataStoreImpl

位置：b_data_mock/src/store/market_data_store_impl.rs

```
MemoryStore（实时分区）
  HashMap<symbol, KlineData>    当前 K 线
  HashMap<symbol, OrderBookData> 当前订单簿
  特征：每次 write_kline 覆盖更新

HistoryStore（历史分区）
  HashMap<symbol, Vec<KlineData>> 已闭合 K 线序列
  特征：append-only

VolatilityManager
  HashMap<symbol, VolatilityData> 波动率快照
```

## 存储单例 2：PipelineStore

位置：b_data_source/src/store/pipeline_store.rs

```
HashMap<symbol, PipelineState>

PipelineState 内部：
  VersionTracker（原子版本号）
    data_version (AtomicU64)
    indicator_version (AtomicU64)
    signal_version (AtomicU64)
    decision_version (AtomicU64)

  stage_timestamps HashMap<PipelineStage, i64>
```

## 存储单例 3：SharedStore<K, V>

位置：b_data_source/src/shared_store.rs

带版本号的通用存储，支持增量读取。

```
SharedStoreImpl<K, V>
  data: RwLock<HashMap<K, Vec<VersionedData<V>>>>
  global_version: AtomicU64
  key_versions: RwLock<HashMap<K, u64>>

方法：
  get_since(key, min_seq) -> 获取指定版本后的所有数据
  global_version() -> 全局版本号
  is_current(key, min_version) -> 检查是否为最新
```

## 存储单例 4：StateCenterImpl

位置：x_data/src/state/center.rs

轻量级组件状态中心，仅记录生死状态。

```
HashMap<component_id, ComponentState>

ComponentState:
  component_id: String
  status: ComponentStatus (Running/Stale/Stopped)
  last_active: DateTime<Utc>
  error_msg: Option<String>
```

## 版本号追踪机制（PipelineState）

一致性检查方法：

```
indicators_current() -> indicator_version >= data_version
signals_current()    -> signal_version >= indicator_version
decisions_current()  -> decision_version >= signal_version
is_consistent()      -> 三者全部满足
```

## 状态一致性要求

| 数据类型 | 一致性要求 | 实现机制 |
|---------|-----------|--------|
| 当前 K 线 | 最终一致 | 每次 write 覆盖 |
| 历史 K 线 | 追加一致 | append-only |
| 指标缓存 | 版本一致 | VersionTracker 原子递增 |
| 持仓状态 | 串行一致 | parking_lot::RwLock |
| 账户状态 | 串行一致 | parking_lot::RwLock |
| Pipeline | 原子一致 | AtomicU64 + RwLock |


================================================================================
第七层：错误与边界处理图
================================================================================

## 数据缺口处理

### 场景：K 线序列号不连续

通过 PipelineState 的 indicator_stale() 检测：

```rust
pub fn indicator_stale(&self, max_age_ms: i64, now_ms: i64) -> bool {
    match self.stage_time(PipelineStage::IndicatorComputed) {
        Some(ts) => now_ms - ts > max_age_ms,
        None => true,  // 从未计算过 = 过时
    }
}
```

### 场景：数据层耗尽

src/actors.rs::run_strategy_actor

```rust
let kline_data = {
    let mut stream = data_layer.kline_stream.lock().await;
    stream.next_message()
};

let Some(data) = kline_data else {
    tracing::info!("data exhausted at tick {}", tick_id);
    break;
};
```

## TradeLock 锁竞争处理

### 场景：锁冲突

src/actors.rs::run_strategy_actor

```rust
let trade_result = {
    let guard = match components.trade_lock.acquire("h_15m_strategy") {
        Ok(g) => g,
        Err(e) => {
            // 锁冲突：发送 Skip 信号
            let signal = StrategySignalEvent {
                tick_id,
                decision: StrategyDecision::Skip,
                reason: format!("lock_conflict: {}", e),
                ...
            };
            let _ = bus_handle.send_strategy_signal(signal);
            continue;
        }
    };
    let r = components.trader.execute_once_wal().await;
    drop(guard);
    r
};
```

## 风控降级处理

### 场景：pre_check 失败

src/actors.rs::run_risk_actor

```rust
let balance_passed = components.risk_checker.pre_check(...).is_ok();
let order_passed = components.order_checker.pre_check(...).passed;

if balance_passed && order_passed {
    // 下单
} else {
    // 风控拒绝：发送 Cancelled 事件
    let event = OrderEvent {
        status: OrderStatus::Cancelled,
        ...
    };
    let _ = bus_handle.send_order(event).await;
}
```

## Kline1mStream 非 Send 类型处理

### 问题：ThreadRng 非 Send

Kline1mStream 内部使用 rand::ThreadRng（Rc<UnsafeCell<...>>），非 Send。

### 解决方案：Mutex guard 在 await 前释放

```rust
// 正确：guard 在 block 结束时释放，不跨越 await
let kline_data = {
    let mut stream = data_layer.kline_stream.lock().await;
    stream.next_message()
};
// guard 已释放，可以安全 await
```

## 计算失败的回退策略

| 失败场景 | 回退行为 | 影响范围 |
|---------|---------|---------|
| K 线解析失败 | 跳过该 tick，记录日志 | 策略暂停该 tick |
| 指标计算失败 | 使用默认值（中性值） | 信号保守 |
| Trader 失败 | ExecutionResult::Failed | 不下单 |
| Risk pre_check 失败 | OrderStatus::Cancelled | 跳过下单 |
| Gateway 失败 | OrderStatus::Rejected | 记录日志 |


================================================================================
第八层：设计哲学与权衡
================================================================================

## 核心设计哲学一：并行 vs 串行（主动 vs 被动）

系统选择了事件驱动协程架构：StrategyActor 主动 + RiskActor 被动。

### 方案 A：传统轮询（已废弃）

```
loop {
    sleep(50ms)
    tick = get_data()
    decision = strategy.decide(tick)
    risk_check(decision)
    order(decision)
}
```

### 方案 B：事件驱动协程（当前）

```
StrategyActor (主动驱动):
    loop {
        data = kline_stream.next_message()  // 主动拉取
        decision = trader.execute_once_wal() // 带 TradeLock
        bus.send_strategy_signal(decision)  // 发送信号
    }

RiskActor (被动消费):
    loop {
        signal = bus.recv()                  // 等待信号
        risk_check(signal)                   // 风控检查
        order = gateway.place_order(signal) // 下单
        bus.send_order(order)               // 发送结果
    }
```

权衡：
  - 优点：职责分离、扩展性好、并发处理
  - 缺点：需要协调、引入 PipelineBus

## 核心设计哲学二：共享存储 vs 消息传递

系统选择了共享存储 pull 模式 + PipelineBus 信号传递。

### 数据层：SharedStore + MarketDataStore

```
Kline1mStream 写入 → SharedStore / MarketDataStore ← StrategyActor 读取
```

### 信号层：PipelineBus

```
StrategyActor --信号--> PipelineBus --信号--> RiskActor
```

## 核心设计哲学三：数据层被动接口设计

### 原则：数据层不主动驱动，只暴露 pull 接口

```
传统设计（主动）:
    DataSourceActor
        loop {
            data = fetch()
            bus.send(data)  // 主动推送
        }

当前设计（被动）:
    StrategyActor (主动拉取)
        loop {
            data = kline_stream.next_message()  // 被动接口
            ...
        }
```

优点：
  - 数据层不依赖上层逻辑
  - 策略可以控制拉取节奏
  - 便于测试（Mock 数据源只需实现 next_message）

## 核心设计哲学四：Send 约束与 Actor 边界

### 问题：非 Send 类型不能跨 await

Kline1mStream 含 ThreadRng，watch::Receiver 含 RwLockReadGuard。

### 解决方案

1. DataLayer：单独传入 StrategyActor，不跨协程
2. Stop signal：使用 broadcast::Receiver（Send-safe）
3. SystemComponents：所有字段都实现 Send + Sync

## 关键权衡清单

| 权衡项 | 当前决策 | 理由 |
|-------|---------|------|
| 主动 vs 被动 | StrategyActor 主动 + RiskActor 被动 | 职责分离、扩展性好 |
| 共享存储 vs 消息队列 | 共享存储（数据） + PipelineBus（信号） | 零复制、易调试 |
| 数据层接口 | 被动（next_message） | 解耦、便于测试 |
| Send 处理 | broadcast channel | 解决 RwLock 非 Send 问题 |
| 单核 vs 多核 | 单核事件循环 | 中频策略无需多核 |
| O(n) vs O(1) 指标 | O(1) 增量计算 | 毫秒级延迟预算 |

## PipelineState：可观测性设计

这是系统最重要的架构创新之一。

每次阶段完成时记录：
  1. 计算延迟（从上一阶段的最后时间戳）
  2. 原子递增版本号（data/indicator/signal/decision）
  3. 输出结构化链路日志

版本号不一致时告警：当 indicator_version < data_version 时，
说明指标未基于最新数据更新，系统可立即告警并暂停策略。

================================================================================
总结：全景图纵览
================================================================================

Barter-Rs v7.0 是一个层次清晰、事件驱动、可观测的交易专用 Rust 量化系统。

它的核心架构哲学体现在三个选择上：

  1. 主动驱动 + 被动消费 = 职责分离 + 并发处理
  2. 共享存储（数据） + PipelineBus（信号） = 零复制 + 灵活协调
  3. 数据层被动接口 = 解耦 + 便于测试

系统的设计始终围绕数据一致性和执行确定性两个核心目标展开，
这两点对于金融交易系统而言，比吞吐量和并行度更为重要。

PipelineState 的引入进一步将可观测性从"结果日志"升级为"过程追踪"，
使得每一个交易决策都能被精确回溯和验证。

================================================================================
END OF ARCHITECTURE_OVERVIEW.md
================================================================================