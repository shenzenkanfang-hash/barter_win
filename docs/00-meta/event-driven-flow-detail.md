---
对应代码: src/sandbox_main.rs
最后验证: 2026-03-28
状态: 活跃
---

# 事件驱动交易系统 - 完整流程详解

## 一、整体架构

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         tokio::join!                                    │
│                    （两个并发执行的任务）                                 │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  ┌─────────────────────────────────┐    ┌─────────────────────────────┐ │
│  │     生产者任务 (Producer)        │    │    消费者任务 (Consumer)    │ │
│  │                                 │    │                             │ │
│  │  StreamTickGenerator::next()   │    │  TradingEngine::run()      │ │
│  │         ↓                      │    │         ↓                  │ │
│  │  生成 Tick 数据                 │    │  tick_rx.recv().await      │ │
│  │         ↓                      │    │         ↓                  │ │
│  │  send_tick_with_backpressure() │◄──►│  on_tick(tick)            │ │
│  │         ↓                      │    │         ↓                  │ │
│  │  tick_tx.send()                │    │  1. update_indicators()   │ │
│  │         ↓                      │    │  2. decide()               │ │
│  │  mpsc::channel(1024)           │    │  3. check_risk()          │ │
│  └─────────────────────────────────┘    │  4. submit_order()       │ │
│                                          └─────────────────────────────┘ │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 二、时间线：T=0 系统启动

### 时刻 T=0.0：main() 开始执行

**主体**: `src/sandbox_main.rs:821`

**动作**: 创建所有核心组件

```rust
// 代码位置: src/sandbox_main.rs:825-840
let gateway = Arc::new(ShadowBinanceGateway::with_default_config(...));
let risk_checker = Arc::new(ShadowRiskChecker::new());
let klines = fetch_klines_from_api(...).await?;
let (tick_tx, tick_rx) = mpsc::channel(1024);  // 创建通道
let mut engine = TradingEngine::new(...);        // 创建引擎
```

---

## 三、时间线：T=1 生产者初始化

### 时刻 T=1.1：创建生产者任务

**主体**: `src/sandbox_main.rs:860`

**动作**: 
1. 克隆 `tick_tx` → `tick_tx_clone`（发送端）
2. 创建 `StreamTickGenerator`
3. 创建 `TickToWsConverter`

```rust
let tick_tx_clone = tick_tx.clone();                    // 克隆发送端
let tick_gen = StreamTickGenerator::from_loader(...);  // 创建生成器
let ws_converter = TickToWsConverter::new(...);       // 创建转换器
```

---

## 四、时间线：T=2 tokio::join! 启动

### 时刻 T=2.1：两个任务同时开始

**主体**: `src/sandbox_main.rs:876`

**代码**:
```rust
let (tx_result, rx_result) = tokio::join! {
    producer_async_block,  // 消费者任务
    consumer_async_block    // 生产者任务
};
```

**解释**: 
- `tokio::join!` 是**并发执行**，不是串行
- 两个 async 块**同时开始运行**
- 但由于 `send().await` 的存在，它们会**互相等待**

---

## 五、时间线：T=3 第一个 Tick 的完整旅程

### T=3.1 生产者：StreamTickGenerator 生成 Tick

**主体**: `StreamTickGenerator::next()`  
**位置**: `crates/h_sandbox/src/historical_replay/tick_generator.rs`

**时机**: `generator.next()` 在 `while let Some(Tick_data) = generator.next()` 循环中

**动作**: 
```rust
// 内部逻辑：从内存中的 K线 数据，逐根生成 Tick
impl Iterator for StreamTickGenerator {
    type Item = SimulatedTick;
    
    fn next(&mut self) -> Option<Self::Item> {
        // 每根 K线 生成 4 个 Tick（开盘、高、低、收）
        // 最后一个 Tick 的 is_last_in_kline = true
    }
}
```

**输出**: `SimulatedTick` 结构体，包含：
- `symbol`: "HOTUSDT"
- `price`: 0.1234
- `qty`: 1.0
- `timestamp`: DateTime<Utc>
- `sequence_id`: 1（递增）
- `is_last_in_kline`: false

---

### T=3.2 生产者：构造完整 Tick

**主体**: 生产者 async 块  
**位置**: `src/sandbox_main.rs:884-910`

**动作**: 将 `SimulatedTick` 转换为 `b_data_source::Tick`

```rust
let tick = Tick {
    symbol: tick_data.symbol.clone(),
    price: tick_data.price,
    qty: tick_data.qty,
    timestamp: tick_data.timestamp,
    sequence_id: tick_data.sequence_id,    // ← 幂等性关键
    kline_1m: Some(KLine { ... }),         // ← 包含 1m K线
    kline_15m: None,
    kline_1d: None,
};
```

---

### T=3.3 生产者：发送 Tick 到通道（关键点！）

**主体**: `tick_tx_clone` (mpsc::Sender)  
**位置**: `src/sandbox_main.rs:918`

**调用**: `send_tick_with_backpressure(&tick_tx_clone, tick, backpressure_mode).await`

**这个 .await 是关键！**

```
当前状态:
┌─────────────────────────────────────────────────────────────┐
│  通道缓冲: [Tick#1] _ _ _ _ _ _ _ _ _ ... (1024容量)      │
│                           ↑                                  │
│                    发送端指向这里                             │
└─────────────────────────────────────────────────────────────┘
```

**Replay 模式下的行为**:
1. `tx.send(tick).await` 被调用
2. 如果**通道有空闲容量**（没满）→ 立即成功，继续执行
3. 如果**通道满了** → **阻塞等待**，直到消费者取走一个 Tick

**这是背压的核心机制！**

---

## 六、时间线：T=4 消费者接收 Tick

### T=4.1 消费者：等待 Tick（recv().await）

**主体**: `tick_rx.recv().await` (mpsc::Receiver)  
**位置**: `src/sandbox_main.rs:450`

**代码**:
```rust
while let Some(tick) = tick_rx.recv().await {
    // ↓ 只有当 Tick 可用时才继续执行
    self.on_tick(tick).await;
}
```

**时机**: 
- 当通道中有 Tick 时，`recv()` **立即返回** Tick
- 当通道为空时，`recv()` **阻塞等待**，直到有 Tick 到来

```
这是串行保序的核心！
        ↓
   recv().await
        ↓
   等待前一个 Tick 完全处理完
        ↓
   才有机会被唤醒
```

---

### T=4.2 消费者：幂等性检查

**主体**: `TradingEngine::run()`  
**位置**: `src/sandbox_main.rs:455-465`

**动作**:
```rust
if tick.sequence_id <= self.last_processed_seq {
    tracing::debug!("跳过重复 Tick");
    continue;  // ← 跳过，回到 recv().await
}
self.last_processed_seq = tick.sequence_id;
```

**时机**: 每次循环开始时

**目的**: 防止同一 Tick 被处理两次（虽然理论上不会发生）

---

### T=4.3 消费者：检查点保存（每 1000 Tick）

**主体**: `checkpoint_manager.save_checkpoint()`  
**位置**: `src/sandbox_main.rs:468-475`

**动作**:
```rust
if self.last_processed_seq % CHECKPOINT_INTERVAL == 0 {
    let checkpoint = Checkpoint {
        last_sequence_id: self.last_processed_seq,
        timestamp_ms: Utc::now().timestamp_millis(),
    };
    self.checkpoint_manager.save_checkpoint(&checkpoint);
}
```

**时机**: 每处理 1000 个 Tick

---

## 七、时间线：T=5 on_tick() 完整处理链

### T=5.1 步骤1：更新指标

**主体**: `state.update_indicators(&tick)`  
**位置**: `src/sandbox_main.rs:487`

**调用链**:
```rust
fn update_indicators(&self, tick: &Tick) {
    let symbol = tick.symbol.clone();
    let mut ind = self.indicators.entry(symbol)
        .or_insert_with(Indicators::default);
    ind.update(tick);  // 增量更新 EMA5, EMA20, RSI
}
```

**动作**:
1. 从 `DashMap<String, Indicators>` 获取或创建该品种的指标
2. 调用 `Indicators::update()` **增量计算**：
   - 新价格加入 `price_history`（固定容量 50，自动 GC）
   - 重新计算 EMA5、EMA20、RSI

**性能**: O(1) 增量，不是 O(n) 全量重算

---

### T=5.2 步骤2：策略决策（带间隔控制）

**主体**: `self.decide(&tick)`  
**位置**: `src/sandbox_main.rs:491`

**动作**:
```rust
fn decide(&self, tick: &Tick) -> Option<TradingDecision> {
    // === 策略间隔检查 ===
    let current_ts = tick.timestamp.timestamp_millis();  // 1002945612000
    let last_ts = self.state.last_strategy_run_ts.load(Ordering::Acquire);
    let interval = self.state.strategy_interval_ms;      // 默认 100ms
    
    if current_ts - last_ts < interval {
        tracing::trace!("Skip: strategy interval not reached");
        return None;  // ← 间隔未到，跳过决策
    }
    
    // ... EMA 金叉/死叉逻辑 ...
    
    // 如果产生决策，更新时间戳
    self.state.last_strategy_run_ts.store(current_ts, Ordering::Release);
}
```

**时机**: 每个 Tick 都会调用，但不一定产生决策

---

### T=5.3 步骤3：风控检查

**主体**: `self.check_risk(&decision)`  
**位置**: `src/sandbox_main.rs:510`

**动作**:
```rust
fn check_risk(&mut self, decision: &TradingDecision) -> Option<OrderRequest> {
    // 1. 获取账户信息
    let account = match self.gateway.get_account() {
        Ok(acc) => acc,
        Err(e) => {
            tracing::warn!("[Risk] 获取账户失败，跳过该决策");
            return None;  // ← 获取失败，跳过
        }
    };
    
    // 2. 构造订单请求
    let order = OrderRequest { ... };
    
    // 3. 风控检查
    match self.risk_checker.check_order(&account, &order) {
        Ok(()) => Some(order),
        Err(e) => {
            tracing::warn!("[Risk] 风控拦截: {}", e);
            None  // ← 风控拒绝，跳过
        }
    }
}
```

---

### T=5.4 步骤4：异步下单

**主体**: `self.submit_order(order).await`  
**位置**: `src/sandbox_main.rs:498`

**动作**:
```rust
async fn submit_order(&mut self, order: OrderRequest) {
    let symbol = order.symbol.clone();
    let side = order.side;
    
    match self.gateway.place_order(order).await {
        Ok(result) => {
            self.state.stats.total_orders += 1;
            tracing::info!("[Order] 下单成功: {}", result.order_id);
        }
        Err(e) => {
            self.state.stats.total_errors += 1;
            tracing::error!("[Order] 下单失败: {}", e);
        }
    }
}
```

**注意**: `gateway.place_order()` 是 `async fn`，会 await 网关响应

---

### T=5.5 步骤5：更新持仓状态

**主体**: `self.update_position_from_trade(&tick)`  
**位置**: `src/sandbox_main.rs:500`

**动作**: 检查是否有新成交，更新本地持仓缓存

---

## 八、时间线：T=6 背压机制详解

### 背压时机图

```
正常情况 (通道有空):
┌────────────────────────────────────────────────────┐
│  生产者: send() → 成功 → 立即继续生成下一个 Tick    │
│  消费者: recv() → 获得 Tick → 处理                  │
│                                                     │
│  时间线: ────[P:生成]──[P:发送]──[C:接收]──[C:处理]──▶
└────────────────────────────────────────────────────┘

背压情况 (通道满了):
┌────────────────────────────────────────────────────┐
│  消费者处理慢于生产者                                 │
│                                                     │
│  通道状态: [T1][T2]...[T1024][满!]                 │
│                                                     │
│  生产者 send() → 阻塞等待!                          │
│         ↑                                           │
│    等待消费者 recv() 取走 Tick                      │
│                                                     │
│  时间线: ───[P:发送]══════╗                         │
│                         阻塞等待                      │
│                [C:接收]──[C:处理]──▶               │
└────────────────────────────────────────────────────┘
```

### 代码实现

**位置**: `src/sandbox_main.rs:698-730`

```rust
async fn send_tick_with_backpressure(
    tx: &mpsc::Sender<Tick>,
    tick: Tick,
    mode: BackpressureMode,
) -> Result<(), ()> {
    match mode {
        BackpressureMode::Replay => {
            // 阻塞模式：等消费者腾出空间
            if tx.send(tick).await.is_err() {
                return Err(());  // 通道关闭
            }
            Ok(())
        }
        BackpressureMode::Realtime => {
            // 非阻塞模式：满了就丢新 Tick
            match tx.try_send(tick) {
                Ok(()) => Ok(()),
                Err(TrySendError::Full(_)) => {
                    tracing::warn!("Channel full, drop newest tick");
                    Ok(())  // 丢新保旧
                }
                Err(TrySendError::Closed(_)) => Err(()),
            }
        }
    }
}
```

---

## 九、时间线：T=7 错误处理流程

### 场景1：获取账户失败

```
生产者: 继续发送 Tick ──────────────────────────▶
                 ↓
消费者: on_tick(tick)
                 ↓
      decide() → 产生决策
                 ↓
      check_risk()
                 ↓
      gateway.get_account() → Err
                 ↓
      tracing::warn!("获取账户失败，跳过该决策")
                 ↓
      return None  ← 风控步骤返回 None
                 ↓
      下一个 Tick 继续处理  ← 不崩溃！
```

### 场景2：风控拦截

```
同场景1，但在风控检查处:
      risk_checker.check_order() → Err("超过最大仓位")
                 ↓
      tracing::warn!("[Risk] 风控拦截")
                 ↓
      return None
                 ↓
      下一个 Tick 继续处理  ← 不崩溃！
```

### 场景3：下单失败

```
      submit_order(order).await
                 ↓
      gateway.place_order() → Err("余额不足")
                 ↓
      tracing::error!("[Order] 下单失败")
                 ↓
      stats.total_errors += 1
                 ↓
      下一个 Tick 继续处理  ← 不崩溃！
```

---

## 十、时间线：T=8 关闭流程

### 触发条件

生产者 `StreamTickGenerator` 遍历完所有 K 线数据，`generator.next()` 返回 `None`

### 关闭步骤

```
步骤1: 生产者退出 while 循环
┌─────────────────────────────────────────────────┐
│  while let Some(Tick_data) = generator.next() { │
│      // ... 生成并发送 Tick                      │
│  }  ← generator.next() 返回 None，循环退出       │
│       ↓                                          │
│  tracing::info!("数据注入完成: {} ticks");       │
└─────────────────────────────────────────────────┘

步骤2: 生产者 drop tick_tx_clone
┌─────────────────────────────────────────────────┐
│  async {                                         │
│      // ...                                      │
│  }  ← async 块结束                               │
│       ↓                                          │
│  tick_tx_clone 被 drop                           │
│       ↓                                          │
│  mpsc::Sender 被 drop                            │
└─────────────────────────────────────────────────┘

步骤3: 消费者收到关闭信号
┌─────────────────────────────────────────────────┐
│  while let Some(tick) = tick_rx.recv().await {  │
│                     ↑                            │
│      recv() 返回 None ← Sender 被 drop 触发      │
│                     ↓                            │
│      退出 while 循环                            │
│       ↓                                          │
│  tracing::info!("事件循环结束");                 │
└─────────────────────────────────────────────────┘

步骤4: tokio::join! 汇总
┌─────────────────────────────────────────────────┐
│  let (tx_result, rx_result) = tokio::join! {   │
│      producer_async,   → Ok(())                  │
│      consumer_async,   → Ok(())                  │
│  };                                              │
│       ↓                                          │
│  tracing::info!("引擎退出完成");                 │
└─────────────────────────────────────────────────┘
```

### 关键机制

**mpsc 通道关闭机制**:
- 当 `Sender` 被 drop，所有 `Receiver.recv()` 会返回 `None`
- 这是 Rust mpsc 的内置机制，**不需要任何显式关闭代码**

---

## 十一、串行保序实现原理

### 核心代码

```rust
// src/sandbox_main.rs:450
while let Some(tick) = tick_rx.recv().await {
    //                        ↑↑↑
    //                   这里是关键！
    self.on_tick(tick).await;
    // ← on_tick 完成前，下一个 recv() 不会执行
}
```

### 为什么不会乱序？

```
tokio 调度器行为:

Tick#1 进入 on_tick():
┌─────────────────────────────────────────┐
│  recv() 返回 Tick#1                     │
│  on_tick() 开始执行                      │
│  ... (处理中，需要时间)                   │
│                                         │
│  ← 此时 recv() 被"锁定"，不会返回 Tick#2 │
└─────────────────────────────────────────┘

Tick#1 处理完成:
┌─────────────────────────────────────────┐
│  on_tick() 完成，返回                    │
│  while 循环回到 recv()                   │
│  recv() 再次被调用，等待 Tick#2          │
└─────────────────────────────────────────┘
```

**关键**: `recv().await` 在上一次 `on_tick().await` 完成后才会被调用

### 对比错误实现

```rust
// ❌ 错误：会导致乱序
tokio::spawn(async move {
    while let Some(tick) = rx.recv().await {
        // 处理可能还没完成
    }
});

// ❌ 错误：会导致并发处理
while let Some(tick) = rx.recv().await {
    tokio::spawn(async move {
        process(tick).await;  // ← 并发了！
    });
}

// ✅ 正确：串行处理
while let Some(tick) = rx.recv().await {
    process(tick).await;  // ← 等待完成才处理下一个
}
```

---

## 十二、完整数据流转路径

```
[内存中的K线数据]
        ↓ StreamTickGenerator::next()
[SimulatedTick]
        ↓ 构造 Tick 结构体
[b_data_source::Tick]
   ├── symbol: String          ("HOTUSDT")
   ├── price: Decimal           (0.1234)
   ├── qty: Decimal             (1.0)
   ├── timestamp: DateTime      (2025-10-10 00:00:00 UTC)
   ├── sequence_id: u64         (1)
   ├── kline_1m: Option<KLine>  (Some(KLine{...}))
   ├── kline_15m: Option        (None)
   └── kline_1d: Option          (None)
        ↓ send_tick_with_backpressure()
[mpsc::channel]
        ↓ recv().await
[TradingEngine::run]
        ↓ on_tick(tick)
   ├── 1. update_indicators()
   │      ↓ DashMap<String, Indicators>
   │      更新 price_history, EMA5, EMA20, RSI
   │
   ├── 2. decide()
   │      ├─ 间隔检查 (AtomicI64)
   │      ├─ EMA5 > EMA20? (金叉)
   │      ├─ RSI < 70 && RSI > 30?
   │      └→ TradingDecision 或 None
   │
   ├── 3. check_risk()
   │      ├─ gateway.get_account()
   │      ├─ risk_checker.check_order()
   │      └→ OrderRequest 或 None
   │
   ├── 4. submit_order()
   │      └→ gateway.place_order()
   │
   └── 5. update_position_from_trade()
          └→ 更新 DashMap<String, PositionState>
```

---

## 十三、关键常量速查

| 常量 | 值 | 作用 | 位置 |
|-----|-----|------|------|
| `MAX_PRICE_HISTORY` | 50 | 指标缓存容量 | `sandbox_main.rs:252` |
| `SLOW_TICK_THRESHOLD_MS` | 10 | 慢 Tick 告警阈值 | `sandbox_main.rs:256` |
| `DEFAULT_STRATEGY_INTERVAL_MS` | 100 | 策略执行间隔 | `sandbox_main.rs:262` |
| `CHECKPOINT_INTERVAL` | 1000 | 检查点保存间隔 | `sandbox_main.rs:265` |
| `CHANNEL_CAPACITY` | 1024 | mpsc 通道容量 | `sandbox_main.rs:845` |

---

## 十四、流程总图

```
┌─────────────────────────────────────────────────────────────────────────┐
│                          系统启动                                         │
│  main() → 创建组件 → tokio::join! 启动两个任务                          │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                    ┌───────────────┴───────────────┐
                    ▼                               ▼
        ┌───────────────────┐           ┌───────────────────┐
        │    生产者任务      │           │    消费者任务      │
        │                   │           │                   │
        │ StreamTickGenerator│           │  TradingEngine    │
        │    .next()         │           │    .run()         │
        │        ↓           │           │        ↓          │
        │  构造 Tick         │           │  recv().await     │
        │        ↓           │           │        ↓          │
        │ send().await ──────┼───────────┼─ recv() 返回      │
        │   (可能阻塞)        │   背压     │        ↓          │
        │        ↓           │           │  on_tick()        │
        │ 继续生成下一个     │           │   ├─ update_ind   │
        └───────────────────┘           │   ├─ decide       │
                                        │   ├─ check_risk   │
                                        │   └─ submit_order  │
                                        │        ↓           │
                                        │  recv().await     │
                                        │  (等待下一个)       │
                                        └───────────────────┘
                                                    │
                        ┌───────────────────────────┘
                        ▼
        ┌───────────────────────────────────────────┐
        │              系统关闭                       │
        │  1. 生产者数据耗尽 → drop Sender          │
        │  2. recv() 返回 None                      │
        │  3. 消费者退出 while 循环                  │
        │  4. tokio::join! 完成                     │
        │  5. 输出结果                              │
        └───────────────────────────────────────────┘
```
