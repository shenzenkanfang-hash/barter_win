---
对应代码: 全项目 crates/ 目录
最后验证: 2026-03-28
状态: 活跃
---

# 系统全景图 - 八层架构

## 第一层：物理结构

系统由 9 个 crate 构成物理分层，按依赖关系从底层到顶层排列：

```
crates/
├── a_common/          # 通用工具层（无依赖）
├── b_data_source/     # 数据源层
├── c_data_process/    # 数据处理层
├── d_checktable/      # 检查层
├── e_risk_monitor/    # 风控层
├── f_engine/          # 引擎层
├── g_test/            # 测试层
├── h_sandbox/         # 沙盒层
└── x_data/            # 数据账户层
```

每个 crate 包含独立的 `src/lib.rs` 和 `Cargo.toml`，形成明确的物理边界。

---

## 第二层：逻辑架构

六层架构的逻辑划分与物理分层对应：

| 层级 | 名称 | Crate | 核心职责 |
|------|------|-------|---------|
| 1 | 工具层 | `a_common` | 波动率计算、数学工具 |
| 2 | 数据层 | `b_data_source` | 市场数据接收、存储、分发 |
| 3 | 信号层 | `c_data_process` | 指标计算（EMA、RSI） |
| 4 | 检查层 | `d_checktable` | 交易检查、资金验证 |
| 5 | 风控层 | `e_risk_monitor` | 风险检查、仓位管理 |
| 6 | 引擎层 | `f_engine` | 策略决策、订单协调 |

沙盒层 `h_sandbox` 独立于六层之外，负责模拟和数据注入。

---

## 第三层：数据流动

市场数据从进入系统到触发交易的完整旅程：

```
[交易所] 
    ↓ WebSocket
[b_data_source::KLineFeed]
    ↓ push_tick()
[MarketDataStore] 
    ↓ get_1m() / get_15m()
[c_data_process::VolatilityCalc]
    ↓ calculate()
[Indicators { ema5, ema20, rsi }]
    ↓ decide()
[f_engine::TradingEngine]
    ↓ check_risk()
[e_risk_monitor::ShadowRiskChecker]
    ↓ place_order()
[h_sandbox::ShadowBinanceGateway]
    ↓ simulate_fill()
[ShadowAccount]
```

关键数据形态：
- `Tick`: 原始报价 `crates/b_data_source/src/models/types.rs:23`
- `KLine`: 聚合K线 `crates/b_data_source/src/models/types.rs:45`
- `Indicators`: 计算指标 `src/sandbox_main.rs:180`

---

## 第四层：执行模型

系统采用并行执行模型，关键结构在 `src/sandbox_main.rs:750-880`：

```
[tokio::join!]
    ├── [生产者任务] StreamTickGenerator::next()
    │       ↓ tick_tx.send()
    │       mpsc::channel(1024)
    │
    └── [消费者任务] TradingEngine::run()
            ↓ tick_rx.recv().await
            on_tick(tick)
                ├── update_indicators()   // O(1) 增量
                ├── decide()              // EMA 金叉/死叉
                ├── check_risk()          // 风控检查
                └── submit_order()        // 异步下单
```

关键约束（`src/sandbox_main.rs:10-25`）：
- `tokio::spawn`: 0 个（全部直接 await）
- `tokio::sleep`: 0 个（事件驱动）
- 背压模式 `BackpressureMode`: Replay 阻塞 / Realtime 非阻塞

---

## 第五层：接口契约

组件间交互的功能约定：

### DataFeeder 接口
```rust
// crates/b_data_source/src/api/data_feeder.rs:45
trait DataFeeder {
    async fn push_tick(&mut self, tick: Tick) -> Result<()>;
    fn subscribe_1m(&mut self, tx: mpsc::Sender<Arc<Tick>>);
    fn subscribe_15m(&mut self, tx: mpsc::Sender<Arc<Tick>>);
}
```

### Gateway 接口
```rust
// crates/h_sandbox/src/simulator/gateway.rs:30
trait ExchangeGateway {
    async fn place_order(&self, req: OrderRequest) -> Result<OrderResult>;
    fn get_account(&self) -> Result<AccountInfo>;
}
```

### 引擎接口
```rust
// src/sandbox_main.rs:420
impl TradingEngine {
    async fn run(&mut self, rx: mpsc::Receiver<Tick>);
    async fn on_tick(&mut self, tick: Tick);
    fn decide(&self, tick: &Tick) -> Option<TradingDecision>;
    fn check_risk(&mut self, decision: &TradingDecision) -> Option<OrderRequest>;
}
```

---

## 第六层：状态管理

系统使用无锁数据结构避免竞争：

### DashMap 替代 RwLock
```rust
// src/sandbox_main.rs:235
struct EngineState {
    indicators: DashMap<String, Indicators>,  // 无锁
    position: DashMap<String, PositionState>, // 无锁
    stats: EngineStats,
}
```

### AtomicI64 策略间隔
```rust
// src/sandbox_main.rs:237-240
struct EngineState {
    last_strategy_run_ts: AtomicI64,  // Acquire 读 / Release 写
    strategy_interval_ms: i64,       // 默认 100ms
}
```

### CheckpointManager 崩溃恢复
```rust
// src/sandbox_main.rs:228-280
trait CheckpointManager {
    fn save_checkpoint(&self, checkpoint: &Checkpoint);
    fn load_checkpoint(&self) -> Option<Checkpoint>;
}
const CHECKPOINT_INTERVAL: u64 = 1000;  // 每1000 Tick保存
```

---

## 第七层：边界处理

系统对异常情况的处理策略：

| 边界情况 | 处理方式 | 日志级别 |
|---------|---------|---------|
| 指标数据不足 | 跳过 Tick，返回 None | trace |
| 风控检查失败 | 返回 None，记录 warn | warn |
| 账户获取失败 | 跳过该决策 | warn |
| Channel 满（Replay） | 阻塞等待 | info |
| Channel 满（Realtime） | 丢弃新 Tick | warn |
| Tick 重复 | 检查 sequence_id 跳过 | debug |
| 慢 Tick | 超过 10ms 记录 | warn |

关键错误日志点：
- `src/sandbox_main.rs:450` - "Skip tick: no indicators yet"
- `src/sandbox_main.rs:510` - "[Risk] 获取账户失败"
- `src/sandbox_main.rs:630` - "[Producer] Channel full"
- `src/sandbox_main.rs:460` - "[Engine] 跳过重复 Tick"

---

## 第八层：设计原则

### 核心设计思想

1. **零轮询驱动**
   所有等待使用 `recv().await` 阻塞，不使用 `sleep` 轮询

2. **串行保序**
   每个 Tick 完整处理后才接收下一个，保证状态一致性

3. **单事件循环**
   引擎内部无并发，所有操作在单一 async 任务内串行执行

4. **增量计算**
   指标更新使用 O(1) 增量算法，不重新遍历历史数据
   ```rust
   // src/sandbox_main.rs:195
   fn calc_ema(prices: &[Decimal], period: usize) -> Decimal {
       let k = dec!(2) / Decimal::from(period + 1);
       let mut ema = prices[0];
       for price in prices.iter().skip(1) {
           ema = *price * k + ema * (Decimal::ONE - k);
       }
       ema
   }
   ```

5. **可观测性优先**
   所有错误、跳过、警告都有结构化日志
   ```rust
   tracing::warn!(
       symbol = %symbol,
       seq = %tick.sequence_id,
       total_ms = %total_ms,
       "[Engine] 慢 Tick 告警"
   );
   ```

### 关键权衡

| 权衡点 | 选择 | 理由 |
|-------|------|------|
| 锁 vs 无锁 | DashMap 替代 RwLock | 消除写锁竞争 |
| clone vs Arc | Arc<Tick> 多品种路由 | 克隆 ~200ns → ~2ns |
| 阻塞 vs 非阻塞 | 可配置背压模式 | 兼容回放和实盘 |
| 数量间隔 vs 时间间隔 | AtomicI64 时间戳控制 | 频率固定 |

---

## 附录：关键常量

| 常量 | 值 | 位置 |
|-----|-----|------|
| `MAX_PRICE_HISTORY` | 50 | `src/sandbox_main.rs:252` |
| `SLOW_TICK_THRESHOLD_MS` | 10 | `src/sandbox_main.rs:256` |
| `DEFAULT_STRATEGY_INTERVAL_MS` | 100 | `src/sandbox_main.rs:262` |
| `CHECKPOINT_INTERVAL` | 1000 | `src/sandbox_main.rs:265` |
| `DEFAULT_SYMBOL` | "HOTUSDT" | `src/sandbox_main.rs:57` |
| `DEFAULT_FUND` | 10000.0 | `src/sandbox_main.rs:58` |
