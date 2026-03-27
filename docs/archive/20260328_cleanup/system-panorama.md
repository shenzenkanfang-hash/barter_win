---
对应代码: crates/*/src/, src/sandbox_main.rs
最后验证: 2026-03-27
状态: 活跃
---

# 量化交易系统全景图

## 第一层：物理结构

系统由 9 个逻辑层级构成，物理上组织在 `crates/` 工作空间目录：

```
crates/
├── a_common/        # 工具层：基础设施
├── b_data_source/   # 数据层：市场数据接口
├── c_data_process/  # 信号层：指标计算
├── d_checktable/    # 检查层：策略验证
│   ├── h_15m/      #   高频15分钟策略
│   └── l_1d/       #   低频1天策略
├── e_risk_monitor/  # 风控层：风险管理
├── f_engine/        # 引擎层：交易协调
├── g_test/          # 测试层
├── h_sandbox/       # 沙盒层：模拟环境
│   ├── gateway/     #   ShadowBinanceGateway
│   ├── simulator/   #   订单引擎/风控检查
│   └── historical_replay/  # 历史数据回放
└── x_data/          # 数据状态管理
```

依赖关系（按 Cargo.toml workspace members 顺序）：
```toml
[workspace]
members = [
    "crates/a_common",
    "crates/b_data_source",
    "crates/c_data_process",
    "crates/d_checktable",
    "crates/e_risk_monitor",
    "crates/f_engine",
    "crates/g_test",
    "crates/h_sandbox",
    "crates/x_data",
]
```

---

## 第二层：逻辑架构

六层架构的职责划分：

| 层级 | 名称 | 核心职责 | 关键文件 |
|------|------|----------|----------|
| a_common | 工具层 | API网关、WS连接器、配置、错误类型、备份、波动率计算 | `crates/a_common/src/lib.rs` |
| b_data_source | 数据层 | 数据订阅、K线合成、DataFeeder统一接口、MarketDataStore | `crates/b_data_source/src/lib.rs` |
| c_data_process | 信号层 | Pine指标计算、策略状态、信号处理、EMA/RSI | `crates/c_data_process/src/lib.rs` |
| d_checktable | 检查层 | 15分钟/1分钟/1天周期策略检查、Trader | `crates/d_checktable/src/lib.rs` |
| e_risk_monitor | 风控层 | 风控检查、持仓管理、灾难恢复、账户池 | `crates/e_risk_monitor/src/lib.rs` |
| f_engine | 引擎层 | 策略协程管理、Trader生命周期、核心类型 | `crates/f_engine/src/lib.rs` |
| h_sandbox | 沙盒层 | 历史回放、订单拦截、账户模拟 | `crates/h_sandbox/src/lib.rs` |

沙盒边界职责：
```
h_sandbox 职责：
- historical_replay/：K线转Tick（60 ticks/K线）
- gateway/interceptor：ShadowBinanceGateway 拦截API
- simulator/：OrderEngine 完整交易流程

沙盒禁止：
- 计算业务指标
- 修改订单逻辑
- 构造业务数据
```

---

## 第三层：数据流动

市场数据从进入系统到触发交易的完整旅程（事件驱动）：

```
1. 外部市场
   | Binance REST API (fapi.binance.com)
   
2. sandbox_main.rs:fetch_klines_from_api()
   | 分页拉取历史K线
   | 最大1000条/页，最大100页
   | url: "https://fapi.binance.com/fapi/v1/klines?..."
   
3. StreamTickGenerator (historical_replay)
   | 1根K线 → 60个Tick (50ms间隔)
   | tick_generator.rs:100-200
   
4. 事件通道 (mpsc::channel)
   | tick_tx.send(tick).await  // 背压处理
   | sandbox_main.rs:180-220
   
5. TradingLoop (事件驱动核心)
   | tick_rx.recv().await  // 阻塞等待
   | on_tick() → 串行处理
   | sandbox_main.rs:280-340
   
6. 数据写入路径
   |
   +--→ DataFeeder::push_tick()
   |    b_data_source/src/api/data_feeder.rs:65
   |    latest_ticks.insert(symbol, tick)
   |
   +--→ IndicatorCache::update()
        sandbox_main.rs:145-230
        ├── price_history 更新
        ├── 波动率计算 (O(n))
        ├── RSI 计算 (14周期)
        └── EMA5/EMA20 计算
        
7. 策略决策路径
   |
   +--→ TradingLoop::check_volatility()
   |    volatility > 2% 阈值 → 触发策略
   |
   +--→ TradeManagerEngine::spawn_strategy_task()
        sandbox_main.rs:450-600
        
8. 交易执行路径
   |
   +--→ ShadowRiskChecker::pre_check()
   |    h_sandbox/src/simulator/risk_checker.rs
   |
   +--→ ShadowBinanceGateway::place_order()
        h_sandbox/src/gateway/interceptor.rs
```

---

## 第四层：执行模型

系统采用**事件驱动 + 通道背压**的异步执行模型：

```rust
// src/sandbox_main.rs

// 1. 事件通道（核心通信）
let (tick_tx, tick_rx) = mpsc::channel(1024);  // 背压: 1024

// 2. TradingLoop - 事件驱动串行循环
let trading_loop = Arc::new(TradingLoop::new(...));
tokio::spawn(async move {
    trading_loop.run(tick_rx).await;  // 阻塞等待
});

// 3. 数据注入层 - 事件生产者
tokio::spawn(async move {
    while let Some(tick_data) = generator.next() {
        // 背压：channel 满时阻塞
        tick_tx.send(tick).await?;
    }
});

// 4. 策略任务 - 独立异步任务
tokio::spawn(async move {
    loop {
        // 从 DataFeeder 读取价格
        let price = data_feeder.ws_get_1m(&symbol)?;
        
        // 策略计算
        if should_open { ... }
        
        sleep(Duration::from_millis(50)).await;
    }
});
```

### 并发结构

```
┌─────────────────────────────────────────────────────────────┐
│                      Tokio Runtime                          │
│                                                              │
│  ┌─────────────┐    mpsc::channel    ┌──────────────────┐   │
│  │ 数据注入任务  │ ───────────────→ │  TradingLoop      │   │
│  │ (生产者)     │    背压 1024      │  (事件驱动核心)    │   │
│  └─────────────┘                    │  - 串行处理 Tick   │   │
│                                      │  - 无 sleep 轮询  │   │
│                                      └────────┬─────────┘   │
│                                               │              │
│  ┌─────────────┐                               │ 克隆引用      │
│  │ 策略任务     │ ←────────────────────────────┘              │
│  │ (50ms间隔)  │                                             │
│  └─────────────┘                                             │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### 关键并发参数

| 参数 | 值 | 位置 |
|------|-----|------|
| Channel 背压 | 1024 | sandbox_main.rs:180 |
| 策略执行间隔 | 50ms | sandbox_main.rs:460 |
| 心跳超时 | 90秒 | sandbox_main.rs:540 |
| Tick 生成间隔 | 50ms | generator.next() |
| K线 → Tick 比率 | 60:1 | 1根K线 = 60个Tick |

---

## 第五层：接口契约

组件之间的功能约定：

### DataFeeder 接口
```rust
// crates/b_data_source/src/api/data_feeder.rs
pub struct DataFeeder {
    kline_1m: Arc<RwLock<Option<Kline1mStream>>>,  // WS K线合成器
    depth_stream: Arc<RwLock<Option<DepthStream>>>,  // 订单簿
    volatility_manager: Arc<VolatilityManager>,     // 波动率管理
    account_syncer: FuturesDataSyncer,               // 账户同步
    latest_ticks: RwLock<HashMap<String, Tick>>,     // 最新Tick缓存
}

impl DataFeeder {
    // WS 数据查询
    pub fn ws_get_1m(&self, symbol: &str) -> Option<KLine>
    pub fn ws_get_15m(&self, symbol: &str) -> Option<KLine>
    pub fn ws_get_1d(&self, symbol: &str) -> Option<KLine>
    pub fn ws_get_depth_book(&self, symbol: &str) -> Option<OrderBook>
    pub fn ws_get_volatility(&self, symbol: &str) -> Option<VolatilityEntry>
    
    // API 数据查询
    pub async fn api_get_account(&self) -> Result<FuturesAccountData, MarketError>
    pub async fn api_get_positions(&self) -> Result<Vec<FuturesPositionData>, MarketError>
    
    // 数据注入
    pub fn push_tick(&self, tick: Tick)  // 沙盒用
}
```

### RiskChecker trait
```rust
// crates/f_engine/src/interfaces/risk.rs
pub trait RiskChecker: Send + Sync {
    fn pre_check(&self, order: &OrderRequest, account: &ExchangeAccount) -> RiskCheckResult;
    fn post_check(&self, order: &ExecutedOrder, account: &ExchangeAccount) -> RiskCheckResult;
    fn scan(&self, positions: &[PositionInfo], account: &ExchangeAccount) -> Vec<RiskWarning>;
    fn thresholds(&self) -> RiskThresholds;
}
```

### OrderRequest 结构
```rust
// crates/f_engine/src/types.rs:70
pub struct OrderRequest {
    pub symbol: String,
    pub side: Side,
    pub order_type: OrderType,
    pub qty: Decimal,
    pub price: Option<Decimal>,
}

pub enum Side { Buy, Sell }
pub enum OrderType { Market, Limit }
```

### ExchangeAccount 接口
```rust
// crates/a_common/src/exchange.rs
pub struct ExchangeAccount {
    pub total_equity: Decimal,
    pub available: Decimal,
    pub frozen_margin: Decimal,
    pub unrealized_pnl: Decimal,
}
```

### TaskState 结构
```rust
// crates/f_engine/src/types.rs:110
pub struct TaskState {
    pub symbol: String,
    pub status: RunningStatus,    // Running/Stopped/Ended
    pub last_beat: i64,          // 心跳时间戳
    pub forbid_until: Option<i64>,
    pub forbid_reason: Option<String>,
    pub done_reason: Option<String>,
}
```

---

## 第六层：状态管理

系统通过共享存储实例和 DashMap 实现跨组件状态同步：

```rust
// src/sandbox_main.rs:80-130

// 共享组件创建
let data_feeder = Arc::new(DataFeeder::new());
let indicator_cache = Arc::new(IndicatorCache::new());
let gateway = Arc::new(ShadowBinanceGateway::with_default_config(initial_fund));
let risk_checker = Arc::new(ShadowRiskChecker::new());
let engine = Arc::new(TradeManagerEngine::new(...));

// IndicatorCache 使用 DashMap 避免锁竞争
struct IndicatorCache {
    cache: dashmap::DashMap<String, Indicators>,  // 无锁并发
}

struct Indicators {
    rsi: Option<Decimal>,
    ema5: Option<Decimal>,
    ema20: Option<Decimal>,
    volatility: Decimal,
    price_history: Vec<Decimal>,  // 滚动窗口 100
}
```

### MarketDataStore 结构
```rust
// crates/b_data_source/src/store/store_impl.rs
pub struct MarketDataStoreImpl {
    memory: Arc<MemoryStore>,      // 实时分区
    history: Arc<HistoryStore>,   // 历史分区
    volatility: Arc<VolatilityManager>,
}

pub struct MemoryStore {
    klines: RwLock<HashMap<String, KlineData>>,
    orderbooks: RwLock<HashMap<String, OrderBookData>>,
}
```

### 任务注册表
```rust
// sandbox_main.rs:400-410
type TaskMap = std::collections::HashMap<String, Arc<TokioRwLock<TaskState>>>;

struct TradeManagerEngine {
    tasks: Arc<TokioRwLock<TaskMap>>,  // 策略任务注册
    global_lock: Arc<tokio::sync::Mutex<()>>,  // 下单互斥
    stats: Arc<TokioRwLock<EngineStats>>,
}
```

---

## 第七层：边界处理

### 事件驱动边界

```rust
// TradingLoop 串行处理保证数据一致性
// sandbox_main.rs:300-340
async fn run(self: Arc<Self>, mut tick_rx: mpsc::Receiver<Tick>) {
    while let Some(tick) = tick_rx.recv().await {
        // 关键：每个 Tick 处理完才接收下一个
        if let Err(e) = self.on_tick(tick, counter).await {
            tracing::error!("处理 Tick 出错: {:?}", e);
        }
    }
}
```

### 背压处理

```rust
// 数据注入层阻塞等待
// sandbox_main.rs:220-240
if tick_tx_clone.send(tick).await.is_err() {
    tracing::info!("事件通道已关闭，停止注入");
    break;
}
```

### 风控拦截（临时跳过）

```rust
// h_sandbox/src/simulator/risk_checker.rs
impl RiskChecker for ShadowRiskChecker {
    fn pre_check(&self, order: &OrderRequest, account: &ExchangeAccount) -> RiskCheckResult {
        // 当前：临时跳过所有检查专注架构测试
        // TODO: 后续启用真实风控规则
        RiskCheckResult::new(true, true)
    }
}
```

### 波动率触发

```rust
// sandbox_main.rs:350-380
async fn check_volatility(&self, tick: &Tick) {
    let volatility = self.indicator_cache.get_volatility(&tick.symbol);
    let threshold = dec!(0.02);  // 2%
    
    if volatility > threshold && !self.engine.has_task(&tick.symbol).await {
        // 触发策略任务
        self.engine.spawn_strategy_task(tick.symbol.clone()).await;
    }
}
```

### 策略互斥

```rust
// sandbox_main.rs:200-210
struct TradingLoop {
    strategy_locks: Arc<DashMap<String, ()>>,  // 防止重复触发
}
```

---

## 第八层：设计原则

### 核心设计思想

1. **事件驱动架构**
   - Tick 到达触发完整处理链
   - mpsc::channel 替代轮询
   - 串行处理保证数据一致性

2. **模块隔离原则**
   - 模块之间禁止直接访问内部数据
   - 所有交互必须通过公开接口
   - 字段必须设为私有

3. **沙盒边界清晰**
   ```
   沙盒职责：
   - historical_replay：注入原始 Tick
   - gateway：拦截 API 调用
   - simulator：模拟账户/订单
   
   沙盒禁止：
   - 计算业务指标
   - 修改订单逻辑
   - 构造业务数据
   ```

4. **共享存储实例**
   ```rust
   // 所有组件共享同一实例
   let data_feeder = Arc::new(DataFeeder::new());
   let indicator_cache = Arc::new(IndicatorCache::new());
   ```

5. **DashMap 替代 RwLock**
   - 减少锁竞争
   - 适合读多写少场景
   - `dashmap::DashMap` 用于 `IndicatorCache`

### 关键权衡

| 权衡点 | 选择 | 原因 |
|--------|------|------|
| 并发模型 | mpsc::channel + 串行循环 | Tick 有序处理，数据一致性 |
| 存储并发 | DashMap | 读多写少，无锁设计 |
| 背压机制 | channel 容量 1024 | 生产者阻塞，防止内存溢出 |
| 策略触发 | 波动率阈值 + 互斥锁 | 避免重复任务 |
| 指标计算 | 滚动窗口 + 增量 | O(n) 但可控 |

---

## 附录：关键文件索引

| 功能 | 文件路径 | 行号 |
|------|----------|------|
| 沙盒入口 | `src/sandbox_main.rs` | 1-700 |
| TradingLoop | `src/sandbox_main.rs` | 260-400 |
| TradeManagerEngine | `src/sandbox_main.rs` | 380-550 |
| IndicatorCache | `src/sandbox_main.rs` | 130-250 |
| DataFeeder | `crates/b_data_source/src/api/data_feeder.rs` | 1-120 |
| MarketDataStore | `crates/b_data_source/src/store/store_impl.rs` | 1-80 |
| RiskChecker trait | `crates/f_engine/src/interfaces/risk.rs` | 1-30 |
| OrderRequest | `crates/f_engine/src/types.rs` | 70-100 |
| ShadowBinanceGateway | `crates/h_sandbox/src/gateway/interceptor.rs` | 1-100 |
| ShadowRiskChecker | `crates/h_sandbox/src/simulator/risk_checker.rs` | 1-80 |
| StreamTickGenerator | `crates/h_sandbox/src/historical_replay/tick_generator.rs` | 1-200 |
| VolatilityManager | `crates/b_data_source/src/ws/volatility.rs` | - |
| 配置结构 | `crates/a_common/src/config.rs` | - |
