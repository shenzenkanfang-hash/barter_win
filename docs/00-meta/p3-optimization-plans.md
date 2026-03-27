---
P3 优化实施方案
版本: 2026-03-27
状态: 待实施
---

# P3 优化实施方案

## P3-1: 多品种 Tick 路由

### 问题
当前所有品种的 Tick 都塞进同一个 channel，每个单品种 Engine 要遍历所有 Tick 过滤自己的 symbol。

### 解决方案

```rust
// 位置: src/sandbox_main.rs 或新文件 src/multi_engine.rs

/// 多品种路由器
struct TickRouter {
    /// 按品种的 channel
    engines: HashMap<String, EngineInstance>,
}

struct EngineInstance {
    tx: mpsc::Sender<Tick>,
    // 或直接存 Engine
}

impl TickRouter {
    /// 启动多品种引擎
    async fn run(&mut self, symbols: Vec<String>) {
        // 1. 为每个品种创建独立的 channel
        for symbol in symbols {
            let (tx, rx) = mpsc::channel(1024);
            self.engines.insert(symbol.clone(), EngineInstance { tx });
            
            // 2. 为每个品种启动独立的事件循环
            let engine = TradingEngine::new(symbol.clone(), /* ... */);
            tokio::spawn(async move {
                engine.run(rx).await;
            });
        }
    }
    
    /// 分发 Tick 到对应品种
    async fn dispatch(&self, tick: &Tick) {
        if let Some(engine) = self.engines.get(&tick.symbol) {
            let _ = engine.tx.send(tick.clone()).await;
        }
    }
}
```

### 改动范围
- 新文件: `src/multi_engine.rs` 或 `src/router.rs`
- 修改: `src/sandbox_main.rs` - 替换单引擎为路由

---

## P3-2: 背压场景适配

### 问题
当前阻塞背压只适合回放，实盘需要非阻塞。

### 解决方案

```rust
/// 背压模式
enum BackpressureMode {
    /// 回放模式：阻塞保时间线
    Replay,
    /// 实盘模式：非阻塞丢旧数据
    Realtime,
}

/// 发送 Tick（根据模式选择策略）
async fn send_tick(
    tx: &mpsc::Sender<Tick>,
    tick: Tick,
    mode: BackpressureMode,
) -> Result<(), TickError> {
    match mode {
        BackpressureMode::Replay => {
            // 阻塞：保时间线一致性
            tx.send(tick).await.map_err(|_| TickError::ChannelClosed)?;
        }
        BackpressureMode::Realtime => {
            // 非阻塞：满了丢旧 Tick，保实时
            match tx.try_send(tick) {
                Ok(()) => Ok(()),
                Err(TrySendError::Full(_)) => {
                    tracing::warn!("[Producer] Channel full, drop old tick");
                    Ok(())  // 不报错，继续处理
                }
                Err(TrySendError::Closed(_)) => Err(TickError::ChannelClosed),
            }
        }
    }
}
```

### 配置项

```rust
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    // ... 其他字段
    /// 背压模式
    pub backpressure_mode: BackpressureMode,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            // ... 
            backpressure_mode: BackpressureMode::Replay, // 默认回放模式
        }
    }
}
```

---

## P3-3: 策略间隔不准

### 问题
按 Tick 数量算间隔，频率不固定时时间不准。

### 解决方案

```rust
// 位置: TradingEngine 新增字段

struct TradingEngine {
    // ... 其他字段
    /// 上次策略执行时间戳（毫秒）
    last_strategy_run_ts: i64,
    /// 策略执行间隔（毫秒）
    strategy_interval_ms: i64,
}

impl TradingEngine {
    /// 带间隔控制的决策
    fn decide_with_interval(&self, tick: &Tick) -> Option<TradingDecision> {
        let current_ts = tick.timestamp.timestamp_millis();
        let last_ts = self.last_strategy_run_ts;
        
        // 间隔检查
        if current_ts - last_ts < self.strategy_interval_ms {
            tracing::trace!(
                symbol = %tick.symbol,
                current_ts = %current_ts,
                last_ts = %last_ts,
                interval = %self.strategy_interval_ms,
                "Skip: strategy interval not reached"
            );
            return None;
        }
        
        // 执行策略
        let decision = self.decide_raw(tick)?;
        
        // 更新上次执行时间
        self.last_strategy_run_ts = current_ts;
        
        Some(decision)
    }
}
```

### 配置

```rust
const DEFAULT_STRATEGY_INTERVAL_MS: i64 = 100;  // 100ms 执行一次策略
```

---

## P3-4: RwLock 锁竞争

### 问题
多 Engine 同时更新账户时 RwLock 写锁竞争。

### 解决方案

```rust
/// 账户更新消息
enum AccountUpdate {
    OrderFilled { symbol: String, qty: Decimal, price: Decimal },
    OrderFailed { symbol: String },
    // ...
}

/// 单线程账户管理器
struct AccountManager {
    account: ShadowAccount,
    rx: mpsc::Receiver<AccountUpdate>,
}

impl AccountManager {
    /// 创建并启动
    fn spawn() -> (Self, mpsc::Sender<AccountUpdate>) {
        let (tx, rx) = mpsc::channel(1024);
        (Self { account: ShadowAccount::new(), rx }, tx)
    }
    
    /// 单线程循环
    async fn run(&mut self) {
        while let Some(update) = self.rx.recv().await {
            match update {
                AccountUpdate::OrderFilled { qty, price, .. } => {
                    self.account.apply_fill(qty, price);
                }
                AccountUpdate::OrderFailed { .. } => {
                    // 处理失败
                }
            }
        }
    }
}

/// TradingEngine 使用
impl TradingEngine {
    async fn submit_order(&mut self, order: OrderRequest) {
        let tx = self.account_tx.clone();
        
        match self.gateway.place_order(order).await {
            Ok(result) => {
                // 发送更新消息（异步，不阻塞）
                let _ = tx.send(AccountUpdate::OrderFilled {
                    symbol: order.symbol,
                    qty: result.filled_qty,
                    price: result.filled_price,
                }).await;
            }
            Err(e) => {
                let _ = tx.send(AccountUpdate::OrderFailed {
                    symbol: order.symbol,
                }).await;
            }
        }
    }
}
```

---

## P3-5: 崩溃恢复能力

### 问题
长回放崩溃后需从头重来。

### 解决方案

```rust
/// 检查点数据
#[derive(Serialize, Deserialize)]
struct Checkpoint {
    /// 最后处理的序列号
    last_sequence_id: u64,
    /// 引擎状态（JSON）
    engine_state: String,
    /// 检查点时间戳
    timestamp: i64,
}

impl TradingEngine {
    /// 检查点间隔（每 N 个 Tick 保存一次）
    const CHECKPOINT_INTERVAL: u64 = 1000;
    
    /// 处理 Tick（带检查点）
    async fn on_tick_with_checkpoint(&mut self, tick: Tick) {
        self.on_tick(tick).await;
        
        // 定期保存检查点
        if tick.sequence_id % Self::CHECKPOINT_INTERVAL == 0 {
            self.save_checkpoint().await;
        }
    }
    
    /// 保存检查点
    async fn save_checkpoint(&self) {
        let checkpoint = Checkpoint {
            last_sequence_id: self.last_processed_seq,
            engine_state: serde_json::to_string(&self.state).unwrap(),
            timestamp: Utc::now().timestamp(),
        };
        
        // 写入 WAL
        self.wal.write_checkpoint(&checkpoint).await;
        
        tracing::info!(seq = %checkpoint.last_sequence_id, "Checkpoint saved");
    }
    
    /// 恢复检查点
    async fn restore_from_checkpoint(&mut self, checkpoint: Checkpoint) {
        self.last_processed_seq = checkpoint.last_sequence_id;
        self.state = serde_json::from_str(&checkpoint.engine_state).unwrap();
        
        tracing::info!(seq = %checkpoint.last_sequence_id, "Checkpoint restored");
    }
}
```

### WAL 接口

```rust
trait WalWriter {
    async fn write_checkpoint(&self, checkpoint: &Checkpoint) -> Result<()>;
    async fn load_latest_checkpoint(&self) -> Result<Option<Checkpoint>>;
}
```

---

## 实施优先级建议

| 优先级 | 优化点 | 理由 |
|-------|-------|------|
| 1 | P3-2 背压场景适配 | 区分回放/实盘是基本需求 |
| 2 | P3-3 策略间隔不准 | 影响策略执行准确性 |
| 3 | P3-1 多品种路由 | 需要时才扩展 |
| 4 | P3-4 RwLock 锁竞争 | 高频场景才明显 |
| 5 | P3-5 崩溃恢复 | 回放不频繁时价值有限 |
