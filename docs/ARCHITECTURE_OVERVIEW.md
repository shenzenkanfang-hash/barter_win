================================================================================
ARCHITECTURE_OVERVIEW.md - Barter-Rs 系统全景图
================================================================================
Author: Claude Code
Created: 2026-03-30
Status: Complete
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
  Cargo.toml              # Workspace 定义，9 个成员 crate
  rustfmt.toml            # 格式化配置
  src/main.rs             # 唯一程序入口（Trading System v5.5）
  crates/                 # 所有 crate（a_common 到 x_data 覆盖全链路）
    a_common/             # 基础设施层
    b_data_source/        # 真实市场数据
    b_data_mock/          # 模拟/沙盒数据（镜像实现）
    c_data_process/       # 指标处理
    d_checktable/         # 策略检查（业务核心）
    e_risk_monitor/       # 风控层
    f_engine/             # 交易引擎核心
    x_data/               # 业务数据类型抽象
    g_test/               # 集成测试（已禁用）
  .planning/codebase/     # 架构文档（ARCHITECTURE.md, STRUCTURE.md 等）
  docs/                   # 设计文档（业务概览等）
  data/                   # 运行时数据（SQLite、历史 K 线 CSV）
  deploy/                 # 部署配置
  sandbox/                # 沙盒/测试场
```

## Crate 内物理结构（核心 crate）

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
  logs/               # 检查点日志（CheckpointLogger）
  exchange/            # 交易所网关类型
```

### b_data_source（数据入口，按职责分目录）

```
b_data_source/src/
  lib.rs
  api/                # REST 数据接口（DataFeeder 统一入口）
  ws/
    kline_1m/         # 1 分钟 K 线合成
    kline_1d/         # 1 天 K 线合成
    order_books/      # 订单簿聚合
  store/              # 数据存储层（核心）
    store_trait.rs    # MarketDataStore trait（跨 crate 共享接口）
    store_impl.rs     # 默认实现：Memory + History + Volatility 组合
    memory_store.rs   # 实时分区（当前 K 线/订单簿）
    history_store.rs  # 历史分区（已闭合 K 线）
    volatility.rs     # 波动率管理器
    pipeline_state.rs # 流水线状态追踪器
    pipeline_store.rs # PipelineStore 封装
  replay_source.rs     # 历史数据回放
```

### d_checktable（策略层，按交易频率分目录）

```
d_checktable/src/
  lib.rs
  h_15m/              # 高频 15 分钟策略
    trader.rs         # Trader 主体（execute_once 循环）
    executor.rs       # 执行器（拆单、重试）
    status.rs         # 状态机
    signal.rs         # 信号定义
    repository.rs     # 状态持久化（SQLite）
    quantity_calculator.rs # 数量计算
  l_1d/               # 低频 1 天策略
  h_volatility_trader/ # 波动率交易器
```

## 关键设计：a_common 铁律

a_common 被所有其他 crate 依赖，因此它绝对禁止包含任何业务类型。

a_common 禁止包含：
  - TradingDecision, OrderRequest (f_engine 业务类型)
  - CheckSignal, CheckChainResult (d_checktable 业务类型)
  - LocalPosition, PositionSide (e_risk_monitor 业务类型)

a_common 允许包含：
  - MarketError, EngineError（纯基础设施错误）
  - SymbolRulesData（交易所无关的通用规则）
  - MemoryBackup 基础设施


================================================================================
第二层：六层架构逻辑图
================================================================================

系统的逻辑分层严格遵循"只能依赖下层"的原则，层与层之间通过 trait 接口解耦。

## 分层依赖链（底部 到 顶部）

  a_common (基础设施层)
       |
       | re-export x_data 类型
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

## 各层职责

### a_common（基础设施层）

提供：API 网关 / WS 连接器 / 内存备份 / 心跳监控 / 波动率计算 / 检查点日志
特征：无任何业务依赖，是所有其他层的工具箱

### x_data（业务数据类型抽象层）

提供：Tick / KLine / Position / Account 统一业务类型
提供：StateManager / StateViewer trait
意义：消除跨模块类型重复定义

### b_data_source / b_data_mock（数据采集层）

提供：MarketDataStore trait（统一存储接口）
职责：K 线合成 / 订单簿聚合 / 波动率检测
真实实现：Binance WS + REST
模拟实现：Mock 实现（feature flag 切换）

### c_data_process（指标处理层）

提供：SignalProcessor（指标管理器）
实现：Pine Indicator v5 完整版（EMA/RSI/MACD）
算法：O(1) 增量计算（无全量重算）

### d_checktable（策略检查层）

提供：CheckTable（信号验证表）
提供：Trader（品种交易主循环）
职责：决策表执行 / 状态管理 / 交易记录
子策略：h_15m（高频 15 分钟）/ l_1d（低频 1 天）

### e_risk_monitor（风控层）

提供：RiskPreChecker（预风控检查）
提供：PositionManager（本地持仓管理）
职责：串行把关（前置检查）/ 灾难恢复

### f_engine（交易引擎核心）

提供：EventEngine（事件驱动引擎）
职责：协调全流程 / 依赖注入（策略+网关）
模式：事件驱动（零轮询 / 零 spawn / 串行处理）

## 工具层的共享机制

a_common 作为工具层被所有其他层共享，具体共享方式为 re-export 依赖链。

re-export 内容包括：

  API: BinanceApiGateway, RateLimiter, SymbolRulesData
  WS: BinanceWsConnector, BinanceTradeStream
  Backup: MemoryBackup, ACCOUNT_FILE, POSITIONS_FILE
  Heartbeat: Clock, Config, Entry, Reporter
  Volatility: VolatilityCalc, VolatilityStats

其他 crate 通过 use a_common::XXX 访问这些组件。

## 数据从上往下流动的方式

系统的数据流动遵循共享存储 pull 模式（而非消息推送模式）。

MarketDataStore trait 定义：

  write_kline(symbol, kline, is_closed)  <- [b] Kline1mStream 写入
  get_current_kline(symbol)               -> [d] Trader 读取
  get_history_klines(symbol)              -> [c] SignalProcessor 读取
  get_volatility(symbol)                  -> [d] Trader 读取

MemoryBackup 单例：

  写入：[e] RiskPreChecker / [d] Trader 状态变更时
  读取：[e] StartupRecoveryManager 启动恢复时


================================================================================
第三层：数据生命周期图
================================================================================

描述一根 Tick 数据从产生到触发订单的全过程。

## Tick 生命周期（完整链路）

### 1. 数据产生

Binance WebSocket（@trade 流）
    原始 JSON 消息

### 2. 协议解析

a_common::ws::BinanceWsConnector
    解析为 BinanceTradeMsg（内部类型）

### 3. K 线合成

b_data_source::ws::kline_1m::Kline1mStream
    将逐笔成交合成 1 分钟 K 线
    is_closed = true 时触发历史写入

### 4. 存储写入（共享存储）

b_data_source::store::MarketDataStoreImpl

    write_kline(is_closed=false) -> memory_store（实时分区）
    volatility.update()          -> volatility_manager（每次都算）
    write_kline(is_closed=true) -> history_store（历史分区）

### 5. 指标计算（被 [d] 调用）

c_data_process::SignalProcessor
    min_update(OHLCV) / day_update(OHLCV)
    O(1) 增量计算

计算内容：
  - EMA(5), EMA(20)         -> Pine 颜色判断
  - RSI(14)                 -> 超买超卖信号
  - TR（真实波幅）           -> 波动率基准
  - zscore(14)              -> 偏离度
  - BigCycleCalculator      -> 日线大周期信号

### 6. 策略决策（业务核心）

d_checktable::h_15m::Trader::execute_once()

    从 Store 读取 K线/指标
    CheckTable 执行决策表
    PinStatusMachine 状态机
    返回 ExecutionResult: Executed / Skipped / Failed

### 7. 风控检查（串行把关）

e_risk_monitor::risk::common::RiskPreChecker

    pre_check()：账户余额 / 最大持仓
    OrderCheck：订单参数合理性

### 8. 订单提交（Feature Flag 切换）

    实盘：a_common::api::BinanceApiGateway::place_order()
    沙盒：b_data_mock::api::MockApiGateway::place_order()
    REST API -> Binance Exchange

### 9. 持仓更新

e_risk_monitor::position::LocalPositionManager

    写入本地持仓状态
    计算未实现盈亏
    MemoryBackup 持久化

### 10. 状态导出

TickContext::to_report() -> JSON 报告
PipelineState::record_with_trace() -> 结构化链路日志

## 数据形态变化（沿链路）

| 阶段 | 数据形态       | 示例                                                    |
|------|--------------|--------------------------------------------------------|
| 1. 原始    | 原始 JSON    | {"s":"HOTUSDT","p":"0.0123","q":"1000","T":123456}     |
| 2. 解析    | BinanceTradeMsg | BinanceTradeMsg { symbol, price, qty, timestamp }  |
| 3. K线     | KlineData    | KlineData { open, close, high, low, volume, is_closed } |
| 4. 存储    | Memory + History | 实时分区 + 历史分区分离                            |
| 5. 指标    | IndicatorCache | { ema_fast, ema_slow, rsi, pine_color }             |
| 6. 信号    | StrategySignal | { signal, qty, reason }                             |
| 7. 风控    | RiskCheckResult | { passed, reason }                                  |
| 8. 订单    | OrderResult  | { order_id, filled_price, filled_qty }               |
| 9. 持仓    | LocalPosition | { qty, avg_price, unrealized_pnl }                    |
| 10. 报告   | JSON Value   | serde_json::Value                                     |


================================================================================
第四层：执行时序图
================================================================================

## 主循环架构（src/main.rs 中的 run_pipeline）

系统的运行时行为由单一 tokio 主循环驱动，不是多个后台任务并行。

核心循环结构：

  loop {
      tokio::select! {
          _ = heartbeat_tick.tick() -> { 心跳保活 }
          _ = tokio::time::sleep(Duration::from_millis(50)) -> {
              [b] 数据引擎：获取 K线（从 ReplaySource 拉取）
              [b] 写入 Store
              [f] 执行层：更新价格/账户
              [d] 策略层：做交易决策（d 内部调用 c）
              [c] 指标层：已在 d 内部调用
              [e] 风控层：d 有决策才触发
          }
      }
  }

## 并行关系分析

### 串行部分（严格顺序 b->f->d->c->e）

每个 Tick 内的 5 个阶段严格串行执行，一个 Tick 处理完毕后才开始下一个 Tick。

### 并行部分

  心跳定时器 与 业务循环：通过 tokio::select! 并行监听
  Kline1mStream 内部：WebSocket 接收与 K线合成并行（异步）
  MemoryBackup 写入：异步后台写入（不影响主流程）

## 时间因素处理

系统通过 50ms 轮询间隔模拟事件驱动：

  用 tokio::time::sleep 作为"秒表"，50ms 检查一次数据是否到达
  pull 模式：let kline_data = kline_stream.next_message()

这种设计选择的原因是：沙盒/回测场景下数据源是文件（ReplaySource），
没有真实的 WebSocket 推送，因此用 50ms 间隔轮询模拟数据到达。

## 共享存储的同步机制

多个组件通过共享 Arc 引用访问同一个 Store 实例：

  let store = Arc::new(MarketDataStoreImpl::new())

  Kline1mStream 持有写入引用（store.clone）
  Trader 持有读取引用（shared_store: StoreRef = store.clone）
  SignalProcessor 持有引用（pipeline_store.clone）


================================================================================
第五层：接口契约图
================================================================================

## 核心 Trait 接口（跨 crate 共享）

### 1. MarketDataStore trait（数据层 -> 策略/指标层）

定义位置：b_data_source/src/store/store_trait.rs

接口方法：

  写入：
    write_kline(symbol, kline, is_closed)
    write_orderbook(symbol, orderbook)
    preload_klines(symbol, klines)

  查询：
    get_current_kline(symbol) -> Option<KlineData>
    get_orderbook(symbol) -> Option<OrderBookData>
    get_history_klines(symbol) -> Vec<KlineData>
    get_history_orderbooks(symbol) -> Vec<OrderBookData>
    get_volatility(symbol) -> Option<VolatilityData>

稳定性：稳定接口。被 d_checktable、c_data_process、f_engine 共同依赖。

### 2. Strategy trait（策略层 -> 引擎层）

定义位置：f_engine/src/event/event_engine.rs

接口方法：

  async fn decide(state: &EngineState) -> Option<TradingDecision>

稳定性：业务核心接口。不同策略（h_15m、l_1d、h_volatility_trader）实现此接口，
注入到 EventEngine。

### 3. ExchangeGateway trait（网关层 -> 引擎层）

定义位置：f_engine/src/event/event_engine.rs

接口方法：

  async fn place_order(order: OrderRequest) -> Result<OrderResult, GatewayError>
  async fn get_account() -> Result<AccountInfo, GatewayError>
  async fn get_position(symbol: &str) -> Result<Option<PositionInfo>, GatewayError>

稳定性：Feature Flag 切换点。实盘用 BinanceApiGateway，沙盒用 MockApiGateway，
通过泛型或 Arc<dyn> 注入切换。

### 4. RiskChecker trait（风控层 -> 引擎层）

定义位置：f_engine/src/interfaces/risk.rs

接口方法：

  fn pre_check(order: &OrderRequest, account: &AccountInfo) -> bool

## StoreRef 类型别名（依赖注入约定）

定义位置：d_checktable/src/h_15m/trader.rs

  pub type StoreRef = Arc<dyn MarketDataStore + Send + Sync>

这是整个系统的核心依赖注入约定。所有需要访问市场数据的组件都通过
StoreRef 持有 MarketDataStore 的 trait object。

## PipelineStore 全链路追踪接口

定义位置：b_data_source/src/store/pipeline_state.rs

接口方法：

  get_or_create(symbol) -> Arc<RwLock<PipelineState>>
  record(symbol, stage, timestamp_ms)
  record_with_trace(symbol, stage, timestamp_ms, trace_id)
  version_snapshot(symbol) -> Option<VersionSnapshot>
  indicator_stale(symbol, max_age_ms, now_ms) -> bool

## 接口调用方式总结

| 接口                | 调用方式       | 同步/异步 | 拉/推      |
|-------------------|--------------|---------|-----------|
| MarketDataStore   | trait method | 同步     | 拉（各层主动读取共享存储） |
| Strategy::decide  | async fn     | 异步     | 引擎调用策略    |
| ExchangeGateway   | async fn     | 异步     | 引擎调用网关    |
| RiskChecker       | sync fn      | 同步     | 引擎调用风控    |
| PipelineStore     | sync fn      | 同步     | 各组件记录阶段完成 |


================================================================================
第六层：状态与存储图
================================================================================

## 状态分布

系统状态分散在多个组件中，通过共享存储单例实现跨组件数据交换。

## 存储单例 1：MarketDataStoreImpl

组成部分：

  MemoryStore（实时分区）
    HashMap<symbol, KlineData>    当前 K 线
    HashMap<symbol, OrderBookData> 当前订单簿
    特征：每次 write_kline 覆盖更新，无历史积累

  HistoryStore（历史分区）
    HashMap<symbol, Vec<KlineData>> 已闭合 K 线序列
    特征：append-only，内存 + 磁盘双写

  VolatilityManager
    HashMap<symbol, VolatilityData> 波动率快照
    特征：每次 write_kline 都更新（is_closed 无关）

## 存储单例 2：MemoryBackup

包含内容：

  AccountSnapshot     账户状态
  PositionSnapshot    持仓快照
  TaskPool            待处理订单
  SystemConfig        限速器状态

## 存储单例 3：PipelineStore

  HashMap<symbol, PipelineState>

PipelineState 内部结构：

  VersionTracker   原子版本号追踪
    data_version (AtomicU64)
    indicator_version (AtomicU64)
    signal_version (AtomicU64)
    decision_version (AtomicU64)

  stage_timestamps HashMap<PipelineStage, i64>
  recent_events Vec<PipelineEvent>

## 版本号追踪机制（PipelineState 核心创新）

一致性检查方法：

  indicators_current() -> indicator_version >= data_version
  signals_current()    -> signal_version >= indicator_version
  decisions_current()  -> decision_version >= signal_version
  is_consistent()     -> 三者全部满足

这套版本号机制解决了量化系统中最关键的问题：如何确保策略决策基于
的是最新数据，而不是过期数据。

## 状态一致性要求

| 数据类型      | 一致性要求  | 实现机制                    |
|-------------|-----------|--------------------------|
| 当前 K 线     | 最终一致    | 每次 write 覆盖，无并发写入       |
| 历史 K 线     | 追加一致    | append-only，磁盘持久化         |
| 指标缓存      | 版本一致    | VersionTracker 原子递增       |
| 持仓状态      | 串行一致    | parking_lot::RwLock 保护    |
| 账户状态      | 串行一致    | parking_lot::RwLock 保护    |
| Pipeline 状态 | 原子一致    | AtomicU64 + RwLock        |

## 锁策略

  热路径（Tick 处理）：无锁
  冷路径（账户/持仓操作）：parking_lot::RwLock 保护

lock_order（获取锁的顺序约定，防止死锁）：
  1. PositionManager (parking_lot::RwLock)
  2. AccountPool (parking_lot::RwLock)

## 存储平台感知路径

Platform::detect() -> Windows (E:/) 或 Linux (/dev/shm/)

高速备份存储（Primary）：
  Windows: E:/shm/backup/
  Linux: /dev/shm/backup/

持久化存储（Secondary）：
  Windows: E:/backup/trading_events.db
  Linux: data/trading_events.db

设计理由：Linux 上使用 /dev/shm（内存文件系统）作为高速备份存储，
Windows 上使用 E: 盘符路径模拟同等效果。


================================================================================
第七层：错误与边界图
================================================================================

## 数据缺失的处理

### 场景 1：Store 中无历史 K 线（沙盒启动时）

问题：沙盒启动时 history_len=0，导致 Trader 第一根 tick 无法计算指标。

解决方案：preload_klines() 批量填充历史数据。

  if !replay_source.is_empty() {
      let store_klines = replay_source.to_store_klines();
      store.preload_klines(SYMBOL, store_klines.clone());
      Trader 启动时即可读取历史，无需等待逐根 K 线闭合
  }

### 场景 2：WebSocket 连接中断

  指数退避重连：sleep 1s -> 2s -> 4s -> 8s -> ... 最大 60s
  重连期间：策略暂停，不盲跑

### 场景 3：K 线序列号不连续

通过 PipelineState 的 indicator_stale() 检测：

  检查指标是否超过 max_age_ms 未更新
  pub fn indicator_stale(&self, max_age_ms: i64, now_ms: i64) -> bool {
      match self.stage_time(PipelineStage::IndicatorComputed) {
          Some(ts) => now_ms - ts > max_age_ms,
          None => true,  从未计算过 = 过时
      }
  }

## 计算失败的回退策略

| 失败场景              | 回退行为                              | 影响范围           |
|-------------------|-----------------------------------|----------------|
| EMA 计算（第 1 个值）    | 直接使用当前价格作为 EMA 初值                 | 指标精度降级          |
| RSI 计算（第 1 个值）    | 返回中性值 50                         | 信号保守           |
| Trader execute_once 失败 | ExecutionResult::Failed             | 不下单，记录日志       |
| Risk pre_check 失败    | 返回 false，跳过下单                   | 策略暂停该 tick      |
| Gateway place_order 失败 | 记录错误，rejected_orders++           | 策略继续运行         |

## 组件故障隔离

单个 Trader 故障：
  不影响其他 Trader（每个品种独立 Trader 实例）
  不影响数据层（b_data_source 独立运行）
  心跳监控系统 3 秒无报到 -> 告警

Gateway 故障（API 超时/拒绝）：
  MockApiGateway 沙盒：永不失败（模拟环境）
  BinanceApiGateway 实盘：熔断器 AccountPool
    连续 N 次失败 -> 熔断，暂停该账户所有交易
  持久化：失败订单记录到 TASKS_DIR，供人工处理

## 系统对输入数据的假设

系统假设：

  close > 0：价格必须为正
  high >= low：K 线合法性
  high >= open && high >= close：K 线合法性
  exchange_timestamp_drift < 100ms：交易所时钟漂移容忍
  sequence_number 单调递增：K 线序列号连续（检测缺失）

假设检查示例（stage_b_data）：

  let valid = ctx.kline.close > Decimal::ZERO
  if !valid {
      ctx.errors.push(StageError {
          stage: "b".into(),
          code: "INVALID_PRICE".into(),
          detail: format!("close={} <= 0", ctx.kline.close),
      });
  }


================================================================================
第八层：设计哲学与权衡
================================================================================

## 核心设计哲学一：并行架构 vs 串行架构的选择

系统选择了串行事件循环而非多线程并行。

量化交易的热路径（Tick -> 决策 -> 风控 -> 订单）有以下特点：

  1. 每个 Tick 之间天然串行（下一个 tick 的数据还没来）
  2. 策略状态有依赖（上一个 tick 的持仓决定下一个 tick 的风控）
  3. 数据竞争的后果严重（一笔错单可能损失数千美元）

权衡：
  优点：零数据竞争、确定性执行、易调试、无锁开销
  缺点：无法利用多核、吞吐受单核限制

当前决策：对于中频策略（15 分钟/1 天级别），单核足够。

## 核心设计哲学二：共享存储 vs 消息传递的选择

系统选择了共享存储 pull 模式而非消息队列。

方案 A（消息队列）：
  WS -> Channel -> Strategy -> Channel -> Risk -> Channel -> Gateway
  每个环节都有 mpsc::Sender/Receiver 持有成本
  跨 await 传递 State 需要 Arc<Mutex<State>>
  调试困难：数据流散落在多个 channel 端点

方案 B（共享存储，本系统采用）：
  WS -> Store -> Strategy（读取）-> Risk（读取）-> Gateway
  单一数据源（Store），无数据复制
  结构化日志（PipelineState）串联全链路
  易于调试：任意时刻可 dump Store 状态

权衡：
  优点：零复制、无 channel 持有开销、结构化可观测、调试友好
  缺点：需要锁保护（但热路径无锁，仅冷路径加锁）

## 核心设计哲学三：沙盒只注入数据，不处理业务逻辑

b_data_mock 的设计原则：

MockApiGateway 的职责：
  - 模拟交易所响应（成交、滑点）
  - 维护模拟账户余额/持仓
  - 模拟网络延迟（可配置）

MockApiGateway 不做：
  - 不生成交易信号
  - 不做策略决策
  - 不改变业务逻辑

原因：
  同一套 d_checktable 策略代码，既可以在实盘跑，也可以在沙盒跑。
  沙盒只改变数据来源（真实 WS vs CSV 回放）。
  沙盒只改变执行结果（真实成交 vs 模拟成交）。
  业务逻辑（策略/风控）完全不变。

## 关键权衡清单

| 权衡项              | 当前决策          | 理由                   | 未来可能调整          |
|-------------------|---------------|----------------------|-------------------|
| 单线程 vs 多线程     | 单线程事件循环      | 中频策略无需多核、无数据竞争   | 高频化后引入 SPMC     |
| 共享存储 vs 消息队列   | 共享存储 pull      | 零复制、可观测、易调试      | 若吞吐不足则改队列     |
| Feature Flag vs 重写 | Mock 层镜像实现    | 实盘/沙盒共用业务代码      | -                |
| O(n) vs O(1) 指标    | O(1) 增量计算      | 毫秒级延迟预算           | 需新增指标时写增量版    |
| SQLite vs 纯内存     | SQLite + 内存双写   | 崩溃恢复、可审计          | 历史数据迁移到 Parquet  |
| Decimal vs f64      | rust_decimal    | 金融精度要求             | -                |

## PipelineState：可观测性设计

这是系统最重要的架构创新之一。传统的量化系统缺乏全链路追踪，
只知道"信号发出了，订单成交了"，但不知道"信号生成花了多少毫秒，
是否基于了过期数据"。

PipelineState 解决了这个问题。

每次阶段完成时记录：

  1. 计算延迟（从上一阶段的最后时间戳）
  2. 原子递增版本号（data/indicator/signal/decision）
  3. 输出结构化链路日志

输出示例：

  trace_id=1234 stage=DataWritten symbol=HOTUSDT latency_ms=0
    version=VersionSnapshot{data=5,indicator=4,signal=3,decision=3}
  trace_id=1234 stage=IndicatorComputed symbol=HOTUSDT latency_ms=2
    version=VersionSnapshot{data=5,indicator=5,signal=3,decision=3}
  trace_id=1234 stage=DecisionMade symbol=HOTUSDT latency_ms=1
    version=VersionSnapshot{data=5,indicator=5,signal=4,decision=4}

版本号不一致时：当 indicator_version < data_version 时，说明指标未基于
最新数据更新，系统可立即告警并暂停策略，避免基于过期数据的错误决策。


================================================================================
总结：全景图纵览
================================================================================

Barter-Rs 是一个层次清晰、数据驱动、可观测、交易专用的 Rust 量化交易系统。

它的核心架构哲学体现在三个选择上：

  1. 串行事件循环 + 共享存储 = 零数据竞争 + 确定性执行
  2. 共享存储 pull 模式 = 零复制 + 全链路可观测
  3. 沙盒仅注入数据 = 实盘/沙盒共用一套业务代码

系统的设计始终围绕数据一致性和执行确定性两个核心目标展开，
这两点对于金融交易系统而言，比吞吐量和并行度更为重要。

PipelineState 的引入进一步将可观测性从"结果日志"升级为"过程追踪"，
使得每一个交易决策都能被精确回溯和验证。


================================================================================
END OF ARCHITECTURE_OVERVIEW.md
================================================================================
