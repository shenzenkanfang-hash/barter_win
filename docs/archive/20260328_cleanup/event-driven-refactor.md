---
对应代码: src/sandbox_event_driven.rs
最后验证: 2026-03-27
状态: 重构方案
---

# 事件驱动完全重构方案

## 一、改造目标

| 指标 | 改造前 | 改造后 |
|------|--------|--------|
| tokio::spawn | 3个 | 0个 |
| tokio::sleep | 5处 | 0处 |
| ws_get_1m() 调用 | 多处轮询 | 0处 |
| 多任务竞态 | 有 | 无 |
| sleep 调用点 | 行217,682,765,815,870 | 无 |

---

## 二、改造前后代码对比

### 2.1 入口结构对比

**改造前（多任务分裂）**：
```rust
// sandbox_main.rs

// 1. 创建组件
let data_feeder = Arc::new(DataFeeder::new());
let indicator_cache = Arc::new(IndicatorCache::new());
let engine = Arc::new(TradeManagerEngine::new(...));

// 2. 创建通道
let (tick_tx, tick_rx) = mpsc::channel(1024);

// 3. spawn 数据注入任务（spawn #1）
tokio::spawn(async move {
    while let Some(tick) = generator.next() {
        tick_tx_clone.send(tick).await?;
        tokio::time::sleep(100ms).await;  // ❌ sleep #1
    }
});

// 4. spawn TradingLoop（spawn #2）- 但实际代码有误
let loop_handle = tokio::spawn({
    async move {
        loop_core.run(tick_rx).await;  // ← run 内部又 spawn
    }
});

// 5. spawn 策略任务（spawn #3）
tokio::spawn(async move {
    loop {
        // ... 策略逻辑 ...
        sleep(50ms).await;  // ❌ sleep #2
    }
});
```

**改造后（单事件循环）**：
```rust
// sandbox_event_driven.rs

// 1. 创建组件
let gateway = Arc::new(ShadowBinanceGateway::with_default_config(...));
let risk_checker = Arc::new(ShadowRiskChecker::new());

// 2. 创建通道
let (tick_tx, tick_rx) = mpsc::channel(1024);

// 3. 创建引擎
let mut engine = TradingEngine::new(symbol, gateway, risk_checker);

// 4. 并发运行生产者和消费者
let (tx_result, rx_result) = tokio::join! {
    // 生产者：数据注入
    async {
        while let Some(tick) = generator.next() {
            tick_tx_clone.send(tick).await?;  // 背压驱动，无 sleep
        }
    },
    
    // 消费者：引擎主循环
    engine.run(tick_rx).await
};
// ✅ 无 spawn，无 sleep
```

---

### 2.2 TradingLoop 对比

**改造前（多方法分裂）**：
```rust
// sandbox_main.rs

struct TradingLoop {
    data_feeder: Arc<DataFeeder>,
    indicator_cache: Arc<IndicatorCache>,
    engine: Arc<TradeManagerEngine>,
    strategy_locks: Arc<DashMap<String, ()>>,
}

impl TradingLoop {
    // 3个独立方法，逻辑分裂
    
    async fn run(self: Arc<Self>, mut tick_rx: ...) {
        while let Some(tick) = tick_rx.recv().await {
            let this = Arc::clone(&self);
            this.on_tick(tick, counter).await;  // ← 又 spawn，又不等待
        }
    }

    async fn on_tick(&self, tick: Tick, ...) {
        self.data_feeder.push_tick(tick.clone());  // 写入
        self.indicator_cache.update(&tick);         // 计算
        self.check_volatility(&tick).await;         // 检查
        self.run_strategy(&tick).await;              // 执行
        // ❌ 四步分裂，轮询拉取
    }

    async fn check_volatility(&self, tick: &Tick) {
        let vol = self.indicator_cache.get_volatility(&tick.symbol);
        if vol > threshold && !self.engine.has_task(&tick.symbol).await {
            self.engine.spawn_strategy_task(tick.symbol.clone()).await;
            // ❌ 又 spawn 新任务
        }
    }

    async fn run_strategy(&self, tick: &Tick) {
        // 空方法，策略逻辑在 TradeManagerEngine 里
    }
}
```

**改造后（单方法内聚）**：
```rust
// sandbox_event_driven.rs

impl TradingEngine {
    /// 单事件循环 - 核心
    async fn run(&mut self, mut tick_rx: mpsc::Receiver<Tick>) {
        while let Some(tick) = tick_rx.recv().await {
            self.on_tick(tick).await;  // ✅ 直接 await，不 spawn
        }
    }

    /// 处理单个 Tick - 完整处理链内聚
    async fn on_tick(&mut self, tick: Tick) {
        let symbol = tick.symbol.clone();

        // 步骤1: 更新指标（增量计算）
        self.state.update_indicators(&tick);

        // 步骤2: 策略决策
        if let Some(decision) = self.decide(&tick) {
            // 步骤3: 风控检查
            if let Some(order) = self.check_risk(&decision) {
                // 步骤4: 异步下单
                self.submit_order(order).await;
            }
        }

        // 步骤5: 更新持仓
        self.update_position_from_trade(&tick);
    }
}
```

---

### 2.3 指标计算对比

**改造前（沙盒越界计算）**：
```rust
// sandbox_main.rs

// 在数据注入任务里调用（越界！）
tokio::spawn(async move {
    while let Some(tick) = generator.next() {
        // ...
        indicator_cache.update(&tick);  // ❌ 沙盒在计算指标
    }
});

// 在 TradingLoop 里又调用一次
async fn on_tick(&self, tick: Tick, ...) {
    self.indicator_cache.update(&tick);  // ❌ 重复计算
}
```

**改造后（引擎内部计算）**：
```rust
// sandbox_event_driven.rs

impl EngineState {
    fn update_indicators(&self, tick: &Tick) {
        let mut ind = self.indicators.entry(symbol)
            .or_insert_with(Indicators::default);
        ind.update(tick);  // ✅ 引擎内部增量计算
    }
}
```

---

### 2.4 策略任务对比

**改造前（独立任务自循环）**：
```rust
// sandbox_main.rs

async fn spawn_strategy_task_with_interval(&self, symbol: String, interval_ms: u64) {
    tokio::spawn(async move {  // ❌ spawn #3
        let mut has_position = false;
        loop {
            // 1. 检查禁止
            {
                let s = state.read().await;
                if s.is_forbidden() {
                    sleep(1s).await;  // ❌ sleep #3
                    continue;
                }
            }

            // 2. 获取全局锁
            let _lock = global_lock.lock().await;

            // 3. 轮询拉取价格 ❌
            let current_price = data_feeder.ws_get_1m(&symbol)
                .map(|k| k.close)
                .unwrap_or(Decimal::ZERO);

            // 4. 轮询拉取指标 ❌
            let indicators = indicator_cache.get(&symbol);

            // 5. 策略计算
            let should_open = ...;

            // 6. 风控 + 下单
            if should_open {
                gateway.place_order(...);
            }

            // 7. sleep 50ms ❌
            sleep(50ms).await;  // ❌ sleep #4, #5
        }
    });
}
```

**改造后（事件触发执行）**：
```rust
// sandbox_event_driven.rs

impl TradingEngine {
    fn decide(&self, tick: &Tick) -> Option<TradingDecision> {
        let indicators = self.state.get_indicators(&tick.symbol)?;
        let position = self.state.get_position(&tick.symbol);

        // ✅ 直接读取当前状态，无轮询
        let (ema5, ema20, rsi) = match (indicators.ema5, indicators.ema20, indicators.rsi) {
            (Some(e5), Some(e20), Some(r)) => (e5, e20, r),
            _ => return None,
        };

        // 策略逻辑内聚
        if !position.has_position && ema5 > ema20 && rsi < dec!(70) && rsi > dec!(30) {
            return Some(TradingDecision { ... });
        }
        // ...
        
        None
    }

    async fn submit_order(&self, order: OrderRequest) {
        // ✅ 异步下单，不阻塞
        match self.gateway.place_order(order) {
            Ok(result) => { ... }
            Err(e) => { ... }
        }
    }
}
```

---

## 三、关键设计差异

### 3.1 串行保序

**改造前**：
```rust
// 问题：on_tick 里 spawn 任务，不等待完成
async fn on_tick(&self, tick: Tick, counter: Arc<AtomicU64>) {
    counter.fetch_add(1, Ordering::SeqCst);
    self.data_feeder.push_tick(tick.clone());
    self.indicator_cache.update(&tick);
    self.check_volatility(&tick).await;  // ← await，但不等待下面的 run_strategy
    self.run_strategy(&tick).await;       // ← 两个都 await，但策略内部又 spawn
}
```

**改造后**：
```rust
// 解决：单事件循环，完全串行
async fn run(&mut self, mut tick_rx: mpsc::Receiver<Tick>) {
    while let Some(tick) = tick_rx.recv().await {
        self.on_tick(tick).await;  // ← 完全等待完成，才处理下一个
    }
}
```

### 3.2 数据一致性

**改造前**：
```rust
// 问题：两次独立读取，中间可能插入新 Tick
let current_price = data_feeder.ws_get_1m(&symbol)...  // Tick N
// ← 这里是另一个 Task 在运行，可能插入了 Tick N+1
let indicators = indicator_cache.get(&symbol);           // Tick N+1 的指标
```

**改造后**：
```rust
// 解决：单线程内聚读取，无并发
fn decide(&self, tick: &Tick) -> Option<TradingDecision> {
    let indicators = self.state.get_indicators(&tick.symbol)?;
    let position = self.state.get_position(&tick.symbol);
    // ✅ Tick N 的指标，Tick N 的持仓状态
}
```

### 3.3 背压机制

**改造前**：
```rust
// 问题：100ms sleep 强制速率，不考虑消费者处理能力
tokio::time::sleep(Duration::from_millis(100)).await;
tick_tx_clone.send(tick).await;  // 发送速率与处理速率脱节
```

**改造后**：
```rust
// 解决：send().await 本身是背压点
if tick_tx_clone.send(tick).await.is_err() {
    break;  // channel 满则阻塞，生产者等消费者
}
// ✅ 无 sleep，速率完全由消费者控制
```

---

## 四、文件清单

### 新增文件
| 文件 | 说明 |
|------|------|
| `src/sandbox_event_driven.rs` | 完全重构后的事件驱动版本 |

### 待删除文件
| 文件 | 说明 |
|------|------|
| `src/sandbox_main.rs` | 旧版多任务分裂版本 |

### 改造文件
| 文件 | 改造内容 |
|------|----------|
| `crates/b_data_source/src/api/data_feeder.rs` | 删除 ws_get_1m() 公开接口 |
| `crates/f_engine/src/types.rs` | 添加 RiskCheckResult 导出 |

---

## 五、验证检查清单

```bash
# 检查1: tokio::spawn 调用次数
grep -r "tokio::spawn" src/sandbox_event_driven.rs
# 预期: 0

# 检查2: tokio::sleep 调用次数
grep -r "sleep" src/sandbox_event_driven.rs
# 预期: 0

# 检查3: ws_get_1m 调用
grep -r "ws_get_1m" src/sandbox_event_driven.rs
# 预期: 0

# 检查4: indicator_cache.update 在沙盒
grep "indicator_cache.update" src/sandbox_event_driven.rs
# 预期: 0

# 检查5: spawn_strategy_task 调用
grep "spawn_strategy_task" src/sandbox_event_driven.rs
# 预期: 0
```

---

## 六、架构特性对比

| 特性 | 改造前 | 改造后 |
|------|--------|--------|
| 任务数 | 4+ 并行 | 2 并发（生产/消费） |
| tokio::spawn | 3个 | 0个 |
| tokio::sleep | 5处 | 0处 |
| ws_get_1m 轮询 | 多处 | 0处 |
| 串行保序 | ❌ 伪串行（spawn后不等待） | ✅ 真串行 |
| 数据一致性 | ❌ 多任务读取可能不一致 | ✅ 单线程读取一致 |
| 背压机制 | ❌ 100ms强制速率 | ✅ send().await背压 |
| 沙盒边界 | ❌ 越界计算指标 | ✅ 只注入数据 |

---

## 七、改造步骤

### 步骤1：替换入口文件
```bash
mv src/sandbox_main.rs src/sandbox_main_old.rs
mv src/sandbox_event_driven.rs src/sandbox_main.rs
```

### 步骤2：删除旧代码
```bash
rm src/sandbox_main_old.rs
```

### 步骤3：验证编译
```bash
cargo check --bin sandbox
```

### 步骤4：运行测试
```bash
cargo run --bin sandbox -- --symbol HOTUSDT --duration 60
```

---

## 八、后续优化方向

### 方向1：删除沙盒 spawn
当前 `tokio::join!` 里生产者仍然是独立任务。进一步优化：

```rust
// 方案A：生产者作为 tokio::spawn，引擎在主任务
tokio::spawn(producer(tick_tx));
engine.run(tick_rx).await;

// 方案B：都作为 tokio::spawn，用 channel 协调
let (done_tx, done_rx) = mpsc::channel(1);
tokio::spawn(producer(tick_tx, done_tx));
tokio::spawn(engine(tick_rx, done_rx));
done_rx.recv().await;
```

### 方向2：删除 gateway 克隆
当前 `Arc<ShadowBinanceGateway>` 仍有引用计数。进一步可用 `&mut`。

### 方向3：风控 Strict/Audit/Bypass 模式
当前风控仍是硬编码通过。应实现三层切换。
