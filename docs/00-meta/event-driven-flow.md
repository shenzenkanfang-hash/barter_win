---
对应代码: src/sandbox_main.rs
最后验证: 2026-03-27
状态: 活跃
---

# 事件驱动架构完整流程详解

## 一、系统启动阶段

**时机：main() 函数开始执行**

```
main() 开始
    |
    ├─→ 创建 DataFeeder (Arc::new)
    ├─→ 创建 IndicatorCache (Arc::new)
    ├─→ 创建 ShadowBinanceGateway (Arc::new)
    ├─→ 创建 ShadowRiskChecker (Arc::new)
    ├─→ fetch_klines_from_api() → 从币安API拉取K线数据
    ├─→ 创建 StreamTickGenerator
    ├─→ 创建 TradeManagerEngine
    ├─→ 创建 mpsc::channel(1024)  ← 关键：返回 (tick_tx, tick_rx)
    |
    └─→ tokio::spawn(data_injection_task)  ← 生产者任务
        └─→ tokio::spawn(trading_loop.run)  ← 消费者任务（但这里有误，实际run是直接await）
```

---

## 二、数据注入任务（生产者）详细流程

**代码位置：sandbox_main.rs:180-260**

```
时间点 T=0: 数据注入任务启动

┌─────────────────────────────────────────────────────────────────────┐
│                  数据注入任务 (tokio::spawn)                          │
│                                                                     │
│  while let Some(tick_data) = generator.next()  ← 阻塞等待取数据      │
│      │                                                              │
│      ├─→ generator.next()                                          │
│      │   来自: StreamTickGenerator                                   │
│      │   返回: SimulatedTick { symbol, price, qty, timestamp... }    │
│      │   频率: 每循环一次 = 1个Tick（由上层100ms sleep控制）          │
│      │                                                              │
│      ├─→ ws_converter.convert()                                    │
│      │   转换为 Binance WS 格式模拟                                   │
│      │   提取 is_closed 标志                                        │
│      │                                                              │
│      ├─→ 构建 Tick 结构体                                           │
│      │   Tick { symbol, price, qty, timestamp, kline_1m: Some(...) } │
│      │                                                              │
│      ├─→ tick_tx_clone.send(tick).await  ← 发送到channel            │
│      │   │                                                          │
│      │   ├─ 背压机制: 如果 channel 满了（1024条），这里会阻塞         │
│      │   │   阻塞等待直到消费者取了数据                                │
│      │   │                                                          │
│      │   └─ 如果消费者关闭了channel（所有Receiver drop），返回 Err    │
│      │       → break 退出循环                                        │
│      │                                                              │
│      └─→ tokio::time::sleep(100ms)  ← 100ms后生成下一个Tick         │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

**Tick 生成节奏**：
- 每个 Tick 之间间隔 **100ms**
- 1根K线 = 60个 Tick（6000ms = 1分钟）

---

## 三、TradingLoop（消费者）详细流程

**代码位置：sandbox_main.rs:280-320**

```
关键：TradingLoop::run() 不是 tokio::spawn，而是直接在当前任务await
这是串行处理的核心保证
```

### 3.1 run() 主循环

```rust
// sandbox_main.rs:290-310
async fn run(self: Arc<Self>, mut tick_rx: mpsc::Receiver<Tick>) {
    while let Some(tick) = tick_rx.recv().await {  // ← 阻塞等待
        let this = Arc::clone(&self);
        let counter = Arc::clone(&tick_counter);
        
        if let Err(e) = this.on_tick(tick, counter).await {
            tracing::error!("处理 Tick 出错: {:?}", e);
            // 继续处理，不退出
        }
    }
    tracing::info!("[TradingLoop] {} 事件循环正常结束", symbol);
}
```

### 3.2 等待机制详解

```
tick_rx.recv().await 的行为：

情况1: channel有数据
    → 立即返回 Some(Tick)，不阻塞
    → 立刻进入 on_tick 处理

情况2: channel为空
    → 阻塞当前task
    → 等待生产者 send() 发来数据
    → 收到数据后唤醒

情况3: 所有发送者都drop了（channel关闭）
    → recv() 返回 None
    → while循环结束
    → 函数返回，任务结束
```

### 3.3 on_tick() 串行处理链

```rust
// sandbox_main.rs:322-345
async fn on_tick(&self, tick: Tick, counter: Arc<AtomicU64>) -> anyhow::Result<()> {
    // 步骤1: 增加计数（原子操作）
    counter.fetch_add(1, Ordering::SeqCst);
    
    // 步骤2: 写入数据源  ← 同步调用，无锁
    self.data_feeder.push_tick(tick.clone());
    
    // 步骤3: 增量计算指标  ← 同步调用
    self.indicator_cache.update(&tick);
    
    // 步骤4: 波动率检查  ← 异步调用
    self.check_volatility(&tick).await;
    
    // 步骤5: 可配置间隔执行策略
    let count = counter.fetch_add(1, Ordering::Relaxed) + 1;
    if count % self.trigger_interval == 0 {
        self.run_strategy(&tick).await;
    }
    
    Ok(())  // ← on_tick 完成后，才进入下一次 recv().await
}
```

---

## 四、数据写入路径详解

### 4.1 DataFeeder::push_tick()

```rust
// crates/b_data_source/src/api/data_feeder.rs:65

pub fn push_tick(&self, tick: Tick) {
    self.update_tick(tick);  // 内部方法
}

fn update_tick(&self, tick: Tick) {
    let symbol = tick.symbol.clone();
    let mut ticks = self.latest_ticks.write();  // parking_lot::RwLock
    ticks.insert(symbol, tick);  // HashMap<String, Tick>
    // ← 写锁，阻塞其他写操作
    // ← 读操作不阻塞
}
```

**存储结构**：
```rust
struct DataFeeder {
    latest_ticks: RwLock<HashMap<String, Tick>>,  // 最新Tick缓存
}
```

### 4.2 IndicatorCache::update()

```rust
// sandbox_main.rs:145-230

fn update(&self, tick: &Tick) {
    let symbol = tick.symbol.clone();
    let price = tick.price;
    
    // DashMap 直接操作，无需锁
    let mut indicators = self.cache.entry(symbol.clone())
        .or_insert_with(Indicators::with_volatility_calc);
    
    // 1. 添加到价格历史
    indicators.price_history.push(price);
    if indicators.price_history.len() > 100 {
        indicators.price_history.remove(0);  // 滚动窗口100
    }
    
    // 2. 计算波动率（O(n)，n=20）
    if indicators.price_history.len() >= 20 {
        let recent = &indicators.price_history[...];
        let mean = recent.iter().sum::<Decimal>() / Decimal::from(20);
        let variance = recent.iter()
            .map(|p| (*p - mean) * (*p - mean))
            .sum::<Decimal>() / Decimal::from(20);
        indicators.volatility = variance.sqrt().unwrap_or(Decimal::ZERO);
    }
    
    // 3. 计算RSI（14周期）
    if indicators.price_history.len() >= 14 {
        // 计算涨跌...
        indicators.rsi = Some(...);
    }
    
    // 4. 计算EMA5/EMA20
    if indicators.price_history.len() >= 5 {
        indicators.ema5 = Some(Self::calc_ema(&indicators.price_history, 5));
    }
    if indicators.price_history.len() >= 20 {
        indicators.ema20 = Some(Self::calc_ema(&indicators.price_history, 20));
    }
    
    // 5. 15m波动率（仅K线闭合时）
    if let Some(ref kline) = tick.kline_1m {
        if kline.is_closed {
            indicators.volatility_calc.update(kline_input);
        }
    }
}
```

---

## 五、波动率触发机制

**代码位置：sandbox_main.rs:350-380**

```rust
async fn check_volatility(&self, tick: &Tick) {
    // 1. 获取15m波动率
    let volatility = self.indicator_cache.get_volatility(&tick.symbol);
    
    // 2. 阈值检查
    let threshold = dec!(0.02);  // 2%
    
    if volatility > threshold && !self.engine.has_task(&tick.symbol).await {
        // 3. 检查互斥锁
        if self.strategy_locks.contains_key(&tick.symbol) {
            tracing::debug!("策略任务已在运行，跳过");
            return;
        }
        
        // 4. 添加互斥锁
        self.strategy_locks.insert(tick.symbol.clone(), ());
        
        // 5. 触发策略任务
        self.engine.spawn_strategy_task(tick.symbol.clone()).await;
    }
}
```

**has_task() 逻辑**：
```rust
async fn has_task(&self, symbol: &str) -> bool {
    self.tasks.read().await.contains_key(symbol)
    // tasks: Arc<TokioRwLock<HashMap<String, Arc<TokioRwLock<TaskState>>>>>
}
```

---

## 六、策略任务（独立异步任务）详细流程

**代码位置：sandbox_main.rs:450-600**

```rust
// spawn_strategy_task_with_interval() 内部

tokio::spawn(async move {
    // ===== 策略任务主循环 =====
    loop {
        // 步骤1: 检查禁止状态
        {
            let s = state.read().await;  // 读取TaskState
            if s.is_forbidden() {
                sleep(Duration::from_secs(1)).await;
                continue;
            }
            if s.status == RunningStatus::Ended {
                break;  // ← 退出循环，任务结束
            }
        }
        
        // 步骤2: 获取全局锁（下单互斥）
        let _lock = global_lock.lock().await;
        
        // 步骤3: 从DataFeeder获取当前价格
        let current_price = {
            data_feeder.ws_get_1m(&symbol)  // 读最新K线
                .map(|k| k.close)
                .unwrap_or(Decimal::ZERO)
        };
        
        // 步骤4: 从IndicatorCache获取指标
        let indicators = indicator_cache.get(&symbol);
        
        // 步骤5: 策略计算（EMA金叉/死叉）
        let should_open = if !has_position && !current_price.is_zero() {
            if let Some(ind) = indicators.as_ref() {
                if let (Some(ema5), Some(ema20), Some(rsi)) = (ind.ema5, ind.ema20, ind.rsi) {
                    ema5 > ema20 && rsi < dec!(70) && rsi > dec!(30)  // 开多条件
                } else { false }
            } else { false }
        } else { false };
        
        let should_close = if has_position { ... } else { false };
        
        // 步骤6: 执行交易
        if should_open && !current_price.is_zero() {
            // 6.1 获取账户
            let account = gateway.get_account()?;
            
            // 6.2 风控检查 ← 关键
            let order_req = OrderRequest { ... };
            let risk_result = risk_checker.pre_check(&order_req, &account);
            
            if risk_result.pre_failed() {
                tracing::warn!("风控拦截");
            } else {
                // 6.3 下单
                match gateway.place_order(order_req) {
                    Ok(result) => {
                        has_position = true;
                        entry_price = current_price;
                        stats.total_orders += 1;
                        stats.total_trades += 1;
                    }
                    Err(e) => {
                        stats.total_errors += 1;
                    }
                }
            }
        }
        
        if should_close && has_position {
            // 平多仓逻辑（类似上面）
        }
        
        // 步骤7: 更新心跳
        state.write().await.heartbeat();
        
        // 步骤8: 释放锁
        drop(_lock);
        
        // 步骤9: 等待下一个周期
        sleep(Duration::from_millis(50)).await;
    }
    
    // 任务结束，从注册表移除
    tasks_ref.write().await.remove(&symbol);
});
```

---

## 七、风控检查流程

**代码位置：h_sandbox/src/simulator/risk_checker.rs**

```rust
impl RiskChecker for ShadowRiskChecker {
    fn pre_check(&self, order: &OrderRequest, account: &ExchangeAccount) -> RiskCheckResult {
        // 当前：临时跳过所有检查
        // TODO: 后续启用真实风控规则
        
        // 杠杆检查（当前禁用）
        // if !self.check_leverage(order.leverage) { ... }
        
        // 订单金额检查（当前禁用）
        // if !self.check_order_value(order.price, order.qty) { ... }
        
        // 余额检查（当前禁用）
        // if order_value > account.available { ... }
        
        RiskCheckResult::new(true, true)  // ← 直接通过
    }
}
```

**ShadowBinanceGateway::place_order()**：
```rust
// h_sandbox/src/gateway/interceptor.rs

pub fn place_order(&self, req: EngineOrderRequest) -> Result<OrderResult, EngineError> {
    let side = match req.side {
        EngineSide::Buy => Side::Buy,
        EngineSide::Sell => Side::Sell,
    };
    
    let price = req.price.unwrap_or_else(|| {
        self.engine.read().get_current_price(&req.symbol).unwrap_or(Decimal::ZERO)
    });
    
    let order_req = OrderRequest {
        symbol: req.symbol.clone(),
        side,
        qty: req.qty,
        price,
        leverage: dec!(1),
    };
    
    let result = self.engine.write().execute(order_req);
    Ok(result)
}
```

---

## 八、背压机制详解

```
背压原理：

mpsc::channel(1024) 的内部结构：
┌────────────────────────────────────────────────────────────┐
│                        Channel                              │
│  ┌──────────┐                          ┌──────────┐        │
│  │  Buffer  │  ← 容量: 1024             │ Receiver │        │
│  │ [Tick 1] │                          │ tick_rx  │        │
│  │ [Tick 2] │                          └──────────┘        │
│  │   ...    │                                               │
│  │ [Tick N] │                          ┌──────────┐        │
│  └──────────┘                          │ tick_tx  │        │
│       ↑                                │ (发送者)  │        │
│  sender.send() 写入这里               └──────────┘        │
│                                                             │
└────────────────────────────────────────────────────────────┘

背压流程（生产者视角）：

当生产者调用 send(tick).await 时：
    │
    ├─→ 如果 buffer 有空间 (< 1024)
    │   → 数据立即写入 buffer
    │   → send() 立刻返回 Ok()
    │   → 生产者继续执行下一行代码
    │
    └─→ 如果 buffer 满了 (= 1024)
        → send() 阻塞当前 task
        → 等待消费者调用 recv() 取走数据
        → 消费者取走数据后，buffer 有空间
        → send() 解除阻塞，返回 Ok()

背压效果：
- 生产者不会无限制生成数据
- 如果消费者处理慢，生产者会被迫等待
- 不会内存溢出
```

---

## 九、串行保序原理

```
为什么 Tick 不会乱序？

关键1：TradingLoop 不是 tokio::spawn
┌─────────────────────────────────────────────────────────────┐
│  // sandbox_main.rs:280-285                                  │
│  let loop_handle = tokio::spawn({                           │
│      let loop_core = Arc::clone(&trading_loop);             │
│      async move {                                            │
│          loop_core.run(tick_rx).await;  ← 不是 spawn！      │
│      }                                                       │
│  });                                                         │
└─────────────────────────────────────────────────────────────┘

关键2：run() 内部直接 await，不 spawn
┌─────────────────────────────────────────────────────────────┐
│  async fn run(self: Arc<Self>, mut tick_rx: ...) {          │
│      while let Some(tick) = tick_rx.recv().await {          │
│          // 这里没有 tokio::spawn！                          │
│          this.on_tick(tick, counter).await;  ← 等待完成      │
│          // on_tick 彻底完成后，才进入下一次 recv             │
│      }                                                       │
│  }                                                           │
└─────────────────────────────────────────────────────────────┘

关键3：Tokio 任务调度
┌─────────────────────────────────────────────────────────────┐
│  Tokio Runtime 调度：                                       │
│                                                             │
│  tick_rx.recv().await 阻塞时 → Runtime 切换到其他任务        │
│  recv() 返回数据后 → 将当前任务放回就绪队列                  │
│  on_tick() 执行完 → 将当前任务放回就绪队列                   │
│  下一轮 recv() → 继续处理                                   │
│                                                             │
│  关键：on_tick() 在返回前必须彻底执行完                       │
│  不会有两个 on_tick() 同时执行                               │
└─────────────────────────────────────────────────────────────┘

时序保证：
Tick1: recv() → on_tick() → 完成 → recv() → Tick2
                ↑ 这之间 Tick2 只能等待
                Tick2 不会插队
```

---

## 十、错误处理流程

```
单Tick处理失败：

情况1：on_tick() 返回 Err
┌─────────────────────────────────────────────────────────────┐
│  while let Some(tick) = tick_rx.recv().await {             │
│      if let Err(e) = this.on_tick(tick, counter).await {   │
│          tracing::error!("处理 Tick 出错: {:?}", e);        │
│          // 继续处理，不退出！                               │
│      }                                                      │
│  }                                                          │
│  // 只有 channel 关闭才会退出                               │
└─────────────────────────────────────────────────────────────┘

情况2：生产者 send() 失败（channel已关闭）
┌─────────────────────────────────────────────────────────────┐
│  if tick_tx_clone.send(tick).await.is_err() {             │
│      tracing::info!("事件通道已关闭，停止注入");             │
│      break;  ← 生产者退出                                  │
│  }                                                          │
└─────────────────────────────────────────────────────────────┘

情况3：策略任务内部出错
┌─────────────────────────────────────────────────────────────┐
│  tokio::spawn(async move {                                 │
│      loop {                                                │
│          // 出错时 continue，不退出任务                      │
│          if should_open {                                  │
│              match gateway.place_order(...) {               │
│                  Err(e) => { stats.total_errors += 1; }    │
│                  // 继续执行                                │
│              }                                             │
│          }                                                  │
│          sleep(50ms).await;                                │
│      }                                                      │
│  });                                                        │
└─────────────────────────────────────────────────────────────┘
```

---

## 十一、关闭流程

```
关闭触发（任一条件满足）：

条件1: 所有数据发送完毕
    │
    └→ generator.next() 返回 None
       └→ while 循环 break
          └→ tick_tx_clone 变量 drop（async block 结束）
             └→ 所有 Sender drop
                └→ channel 关闭
                   └→ tick_rx.recv() 返回 None
                      └→ while 循环结束
                         └→ run() 返回

条件2: 超时
    │
    └→ sleep(duration) 返回
       └→ tick_tx_clone drop（main 函数结束）
          └→ channel 关闭
             └→ 同上
```

**完整关闭序列**：
```
1. tick_tx_clone drop
2. mpsc::Sender drop（所有发送端）
3. channel 内部缓冲区清空
4. tick_rx.recv() 返回 None
5. while let Some(tick) = ... 退出
6. run() 函数返回
7. tokio::spawn() 的 async block 结束
8. loop_handle 可以等待结束
```

---

## 十二、完整时间线（Tick N 的生命周期）

```
时间: T=100ms（N=1）

┌──────────────────────────────────────────────────────────────────────────┐
│                           Tick N 生成                                    │
├──────────────────────────────────────────────────────────────────────────┤
│  主体: StreamTickGenerator + 数据注入任务                                 │
│  时机: 上一个Tick发送完成 + 100ms sleep后                                │
│  方式: generator.next() 同步返回 SimulatedTick                           │
│  动作:                                                                  │
│    1. generator.next() 返回 SimulatedTick                               │
│    2. ws_converter.convert() 转换为WS格式                               │
│    3. 构建 Tick { symbol, price, qty, timestamp, kline_1m }             │
│    4. tick_tx_clone.send(tick).await                                   │
│       ├─ 如果channel满 → 阻塞等待                                        │
│       └─ 如果channel有空间 → 立即返回                                    │
└──────────────────────────────────────────────────────────────────────────┘
                                    ↓
┌──────────────────────────────────────────────────────────────────────────┐
│                           Tick N 传输                                   │
├──────────────────────────────────────────────────────────────────────────┤
│  主体: mpsc::channel                                                    │
│  时机: send() 成功返回后                                                │
│  方式: 内部 buffer 直接写入                                              │
│  动作: Tick 放入 channel buffer [1024容量]                               │
└──────────────────────────────────────────────────────────────────────────┘
                                    ↓
┌──────────────────────────────────────────────────────────────────────────┐
│                        Tick N 接收（消费者）                             │
├──────────────────────────────────────────────────────────────────────────┤
│  主体: TradingLoop::run()                                               │
│  时机: channel有数据时立即唤醒                                            │
│  方式: tick_rx.recv().await 阻塞等待                                     │
│  动作:                                                                  │
│    1. recv() 从buffer取出Tick                                           │
│    2. 返回 Some(tick)                                                   │
│    3. 解除阻塞，进入 on_tick()                                           │
└──────────────────────────────────────────────────────────────────────────┘
                                    ↓
┌──────────────────────────────────────────────────────────────────────────┐
│                        Tick N 处理                                      │
├──────────────────────────────────────────────────────────────────────────┤
│  主体: TradingLoop::on_tick()                                           │
│  时机: recv()返回后立即执行                                              │
│  方式: 同步/异步混合，全程不释放控制权                                     │
│  动作:                                                                  │
│                                                                          │
│  [同步] counter.fetch_add(1) → 原子操作                                  │
│                                                                          │
│  [同步] data_feeder.push_tick(tick)                                     │
│    → latest_ticks.write().insert()                                      │
│                                                                          │
│  [同步] indicator_cache.update(&tick)                                   │
│    → DashMap 直接操作                                                   │
│    → 计算 volatility, RSI, EMA5, EMA20                                  │
│                                                                          │
│  [异步] check_volatility(&tick)                                         │
│    → get_volatility() → DashMap 读                                       │
│    → has_task() → TokioRwLock 读                                        │
│    → spawn_strategy_task() → tokio::spawn                               │
│                                                                          │
│  [条件触发] 如果 trigger_interval 满足                                   │
│    [异步] run_strategy(&tick)                                           │
│                                                                          │
│  → on_tick() 完成，返回 Ok()                                            │
└──────────────────────────────────────────────────────────────────────────┘
                                    ↓
┌──────────────────────────────────────────────────────────────────────────┐
│                      等待下一个 Tick                                     │
├──────────────────────────────────────────────────────────────────────────┤
│  主体: TradingLoop::run()                                               │
│  时机: on_tick()完成                                                    │
│  方式: 返回 run() 主循环，开始下一轮 recv().await                        │
│  动作:                                                                  │
│    1. while let Some(tick) 继续循环                                     │
│    2. tick_rx.recv().await                                              │
│       ├─ 如果channel有下一条数据 → 立即返回，下一个Tick                  │
│       └─ 如果channel空 → 阻塞等待生产者send()                          │
└──────────────────────────────────────────────────────────────────────────┘

在 on_tick() 内部，策略任务的执行是独立的 tokio::spawn：

┌──────────────────────────────────────────────────────────────────────────┐
│                       策略任务（独立异步）                                │
├──────────────────────────────────────────────────────────────────────────┤
│  主体: spawn_strategy_task() 内部的 tokio::spawn                        │
│  时机: check_volatility() 触发后                                        │
│  方式: 独立任务，50ms 间隔循环                                           │
│  动作:                                                                  │
│                                                                          │
│  loop {                                                                 │
│      // 1. 获取全局锁                                                    │
│      global_lock.lock().await                                           │
│                                                                          │
│      // 2. 读取价格和指标                                                │
│      data_feeder.ws_get_1m() → 读 latest_ticks                         │
│      indicator_cache.get() → 读 DashMap                                 │
│                                                                          │
│      // 3. 策略计算                                                      │
│      EMA金叉/死叉判断                                                    │
│                                                                          │
│      // 4. 风控检查                                                      │
│      risk_checker.pre_check()                                          │
│                                                                          │
│      // 5. 下单（如果有信号）                                           │
│      gateway.place_order()                                              │
│                                                                          │
│      // 6. sleep 50ms                                                  │
│      tokio::time::sleep(50ms).await                                     │
│  }                                                                     │
└──────────────────────────────────────────────────────────────────────────┘
```

---

## 十三、关键设计点总结

| 设计点 | 实现方式 | 效果 |
|--------|----------|------|
| **串行保序** | run() 直接 await on_tick()，不 spawn | 每个Tick彻底处理完才处理下一个 |
| **背压** | send().await 在channel满时阻塞 | 生产者不会无限生成 |
| **错误不崩溃** | on_tick() 返回 Err 后 continue | 单Tick失败不中断全流程 |
| **自然关闭** | 生产者 drop → channel关闭 → recv() 返回 None | 无需显式信号 |
| **无sleep轮询** | recv().await 阻塞等待 | 事件驱动，非轮询 |
| **策略独立** | tokio::spawn 独立任务 + 50ms间隔 | 不阻塞主循环 |

---

## 十四、架构特性对照表

| 特性 | 实现位置 | 关键代码 |
|------|----------|----------|
| Channel 创建 | sandbox_main.rs:180 | `mpsc::channel(1024)` |
| 生产者发送 | sandbox_main.rs:220 | `tick_tx_clone.send(tick).await` |
| 消费者接收 | sandbox_main.rs:295 | `tick_rx.recv().await` |
| 串行保证 | sandbox_main.rs:300 | `this.on_tick(tick).await` |
| 数据写入 | data_feeder.rs:65 | `push_tick()` |
| 指标计算 | sandbox_main.rs:145 | `IndicatorCache::update()` |
| 波动率触发 | sandbox_main.rs:350 | `check_volatility()` |
| 策略任务 | sandbox_main.rs:450 | `spawn_strategy_task()` |
| 风控检查 | risk_checker.rs:20 | `pre_check()` |
| 下单执行 | interceptor.rs:60 | `place_order()` |
