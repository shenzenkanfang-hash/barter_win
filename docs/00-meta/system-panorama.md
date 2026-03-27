---
对应代码: crates/*/src/, src/sandbox_main.rs
最后验证: 2026-03-27
状态: 活跃
---

# 量化交易系统全景图

## 第一层：物理结构

系统由 8 个逻辑层级构成，物理上组织在 `crates/` 工作空间目录：

```
crates/
├── a_common/        # 工具层：基础设施
├── b_data_source/   # 数据层：市场数据接口
├── c_data_process/  # 信号层：指标计算
├── d_checktable/    # 检查层：策略验证
├── e_risk_monitor/   # 风控层：风险管理
├── f_engine/         # 引擎层：交易协调
├── g_test/           # 测试层
├── h_sandbox/        # 沙盒层：模拟环境
└── x_data/          # 数据状态管理
```

依赖关系（按 Cargo.toml workspace members 顺序）：
```toml
a_common = { path = "crates/a_common" }
b_data_source = { path = "crates/b_data_source" }
c_data_process = { path = "crates/c_data_process" }
d_checktable = { path = "crates/d_checktable" }
e_risk_monitor = { path = "crates/e_risk_monitor" }
f_engine = { path = "crates/f_engine" }
h_sandbox = { path = "crates/h_sandbox" }
```

---

## 第二层：逻辑架构

六层架构的职责划分：

| 层级 | 名称 | 核心职责 | 关键文件 |
|------|------|----------|----------|
| a_common | 工具层 | API网关、WS连接器、配置、错误类型、备份 | `crates/a_common/src/lib.rs` |
| b_data_source | 数据层 | 数据订阅、K线合成、DataFeeder统一接口 | `crates/b_data_source/src/lib.rs` |
| c_data_process | 信号层 | Pine指标计算、策略状态、信号处理 | `crates/c_data_process/src/lib.rs` |
| d_checktable | 检查层 | 15分钟/1分钟/1天周期策略检查 | `crates/d_checktable/src/lib.rs` |
| e_risk_monitor | 风控层 | 风控检查、持仓管理、灾难恢复 | `crates/e_risk_monitor/src/lib.rs` |
| f_engine | 引擎层 | 策略协程管理、Trader生命周期 | `crates/f_engine/src/lib.rs` |
| h_sandbox | 沙盒层 | 历史回放、订单拦截、账户模拟 | `crates/h_sandbox/src/lib.rs` |

沙盒边界职责：
```
h_sandbox 职责：
- historical_replay: K线转Tick，50ms间隔推送
- gateway/interceptor: 拦截API调用，走模拟账户
- simulator: 完整交易流程（账户/订单/风控）

沙盒禁止：
- 计算业务指标
- 修改订单逻辑
- 构造业务数据
```

---

## 第三层：数据流动

市场数据从进入系统到触发交易的完整旅程：

```
1. 外部市场
   | Binance WebSocket (fapi.binance.com)
   
2. b_data_source (数据层)
   |   ws/kline_1m/: 接收 1m K线回调
   |   ws/kline_1d/: 接收 1d K线回调  
   |   ws/depth/: 接收订单簿深度
   |   api/: REST API 账户/持仓查询
   | Tick { symbol, price, qty, timestamp, kline_1m }
   
3. DataFeeder (统一数据接口)
   | pub fn push_tick(&self, tick: Tick)
   | crates/b_data_source/src/api/data_feeder.rs:65
   |
4. MarketDataStore (共享存储)
   | pub struct MarketDataStoreImpl
   | crates/b_data_source/src/store/store_impl.rs:14
   |   - memory: Arc<MemoryStore> (实时分区)
   |   - history: Arc<HistoryStore> (历史分区)
   |
5. c_data_process (信号层)
   | IndicatorCache::update() 计算 RSI/EMA/波动率
   | sandbox_main.rs:150-200
   
6. f_engine (引擎层)
   | TradeManagerEngine::get_volatility()
   | sandbox_main.rs:350
   
7. 策略执行 (并行任务)
   |   - DataFeeder::ws_get_1m() 获取当前价格
   |   - IndicatorCache::get() 获取指标
   |   - ShadowRiskChecker::pre_check() 风控检查
   |   - ShadowBinanceGateway::place_order() 下单
```

---

## 第四层：执行模型

系统采用异步并行执行模型：

```rust
// 入口：src/sandbox_main.rs

// 并行任务1：数据源层 (50ms/tick)
tokio::spawn(async move {
    while let Some(tick_data) = generator.next() {
        data_feeder.push_tick(tick);  // 写入共享存储
        indicator_cache.update(&tick);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
});

// 并行任务2：指标层 (50ms/周期)
tokio::spawn(async move {
    loop {
        if let Some(kline) = data_feeder.ws_get_1m(&symbol) {
            indicator_cache.calculate_indicators(&kline);
        }
        sleep(Duration::from_millis(50)).await;
    }
});

// 并行任务3：引擎主循环 (1s/周期)
let engine_handle = tokio::spawn(async move {
    loop {
        let volatility = engine.get_volatility(&symbol).await;
        if volatility > threshold && !engine.has_task(&symbol).await {
            engine.spawn_strategy_task(symbol, 50).await;
        }
        engine.check_tasks().await;
        sleep(Duration::from_secs(1)).await;
    }
});

// 策略任务 (50ms/周期)
tokio::spawn(async move {
    loop {
        // 1. 检查禁止状态
        // 2. 获取全局锁 global_lock
        // 3. 从 DataFeeder 获取价格
        // 4. 从 IndicatorCache 获取指标
        // 5. 策略计算 (EMA金叉/死叉)
        // 6. 风控检查
        // 7. 下单
        // 8. 更新心跳
        sleep(Duration::from_millis(interval_ms)).await;
    }
});
```

关键并发参数：
- Tick 间隔：50ms (sandbox_main.rs:120)
- 指标计算：每 50ms 一次 (sandbox_main.rs:130)
- 引擎检查：1s 一次 (sandbox_main.rs:143)
- 心跳超时：90秒 (sandbox_main.rs:305)
- 策略间隔：50ms (sandbox_main.rs:325)

---

## 第五层：接口契约

组件之间的功能约定：

### DataFeeder 接口
```rust
// crates/b_data_source/src/api/data_feeder.rs
pub trait DataFeederInterface {
    fn ws_get_1m(&self, symbol: &str) -> Option<KLine>;
    fn ws_get_15m(&self, symbol: &str) -> Option<KLine>;
    fn ws_get_1d(&self, symbol: &str) -> Option<KLine>;
    fn ws_get_depth_book(&self, symbol: &str) -> Option<OrderBook>;
    fn ws_get_volatility(&self, symbol: &str) -> Option<VolatilityEntry>;
    fn push_tick(&self, tick: Tick);  // 沙盒注入数据
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

---

## 第六层：状态管理

系统通过共享存储实例实现跨组件状态同步：

```rust
// 共享存储实例创建
let data_feeder = Arc::new(DataFeeder::new());
let indicator_cache = Arc::new(IndicatorCache::new());
let gateway = Arc::new(ShadowBinanceGateway::with_default_config(initial_fund));
let risk_checker = Arc::new(ShadowRiskChecker::new());

// 克隆引用传递给并行任务
let data_feeder_for_gen = Arc::clone(&data_feeder);
let indicator_cache_for_gen = Arc::clone(&indicator_cache);

// MarketDataStoreImpl 结构
pub struct MarketDataStoreImpl {
    memory: Arc<MemoryStore>,      // 实时K线、订单簿
    history: Arc<HistoryStore>,    // 历史K线、订单簿
    volatility: Arc<VolatilityManager>,
}

// MemoryStore 结构
pub struct MemoryStore {
    klines: RwLock<HashMap<String, KlineData>>,
    orderbooks: RwLock<HashMap<String, OrderBookData>>,
}
```

状态同步机制：
- Tick 数据通过 DataFeeder::push_tick() 写入 MemoryStore
- 指标计算读取 DataFeeder::ws_get_1m() 获取最新 K 线
- 引擎通过 TradeManager.tasks: Arc<TokioRwLock<TaskMap>> 管理任务状态
- 账户通过 ShadowBinanceGateway.engine: Arc<RwLock<OrderEngine>> 管理

---

## 第七层：边界处理

系统对异常情况的处理：

### 数据缺失处理
```rust
// 禁止使用默认值掩盖错误
let kline = store.get_current_kline().unwrap_or_default();  // 错误！

// 正确做法：让系统自己处理
let kline = trader.get_current_kline().await?;  // 返回 Error
```

### 沙盒数据注入边界
```rust
// sandbox_main.rs:100-140
// 数据源层只负责注入原始 Tick，不预计算指标
while let Some(tick_data) = generator.next() {
    let tick = Tick { ... };  // 原始数据
    data_feeder.push_tick(tick.clone());  // 写入 Store
    indicator_cache.update(&tick);        // 沙盒只更新缓存
}
```

### 风控拦截
```rust
// crates/h_sandbox/src/simulator/risk_checker.rs
impl RiskChecker for ShadowRiskChecker {
    fn pre_check(&self, order: &OrderRequest, account: &ExchangeAccount) -> RiskCheckResult {
        // 当前：临时跳过所有检查专注架构测试
        // 真实环境应启用：
        // - 杠杆检查: leverage <= 20
        // - 订单金额: value >= 5 USDT
        // - 余额检查: order_value <= available
        RiskCheckResult::new(true, true)
    }
}
```

### 心跳超时检测
```rust
// sandbox_main.rs:380-390
async fn check_heartbeat(&self) {
    let now = Utc::now().timestamp();
    for (symbol, state_arc) in tasks.iter() {
        let s = state_arc.read().await;
        if s.status == RunningStatus::Running 
           && s.last_beat < now - self.heartbeat_timeout {
            // 任务心跳超时，标记处理
        }
    }
}
```

---

## 第八层：设计原则

### 核心设计思想

1. 模块隔离原则
   - 模块之间禁止直接访问内部数据
   - 所有交互必须通过公开接口 (Trait/公共方法)
   - 字段必须设为私有 (private)

2. 沙盒边界清晰
   ```
   沙盒职责：
   - 注入原始 Tick/K线
   - 拦截交易所请求
   - 模拟网络故障
   
   沙盒禁止：
   - 预计算指标
   - 补全数据
   - 修改订单逻辑
   ```

3. 共享存储实例
   ```rust
   // 正确：所有组件共享同一实例
   let shared_store = Arc::new(MarketDataStore::new());
   let data_feeder = DataFeeder::new(shared_store.clone());
   let trader = Trader::new(shared_store);
   
   // 错误：实例隔离
   let data_feeder = DataFeeder::new(default_store());
   let trader = Trader::new(default_store());  // 不同实例！
   ```

4. 诚实暴露问题
   - 绝不构造假数据
   - 绝不默认值降级
   - 绝不帮系统绕过错误

### 关键权衡

| 权衡点 | 选择 | 原因 |
|--------|------|------|
| 并发模型 | tokio::spawn 异步任务 | IO密集型，充分利用异步 |
| 存储架构 | MemoryStore + HistoryStore 分离 | 热数据/冷数据分离，内存效率 |
| 风控时机 | pre_check + post_check 两阶段 | 订单前后双重把关 |
| 指标计算 | 沙盒层独立计算 | 保持真实系统计算路径一致 |
| 数据注入 | 原始Tick注入 | 不污染业务逻辑计算路径 |

---

## 附录：关键文件索引

| 功能 | 文件路径 |
|------|----------|
| 沙盒入口 | src/sandbox_main.rs |
| DataFeeder | crates/b_data_source/src/api/data_feeder.rs |
| MarketDataStore | crates/b_data_source/src/store/store_impl.rs |
| RiskChecker trait | crates/f_engine/src/interfaces/risk.rs |
| OrderRequest | crates/f_engine/src/types.rs |
| ShadowGateway | crates/h_sandbox/src/gateway/interceptor.rs |
| ShadowRiskChecker | crates/h_sandbox/src/simulator/risk_checker.rs |
| VolatilityManager | crates/b_data_source/src/ws/volatility.rs |
| 配置结构 | crates/a_common/src/config.rs |
