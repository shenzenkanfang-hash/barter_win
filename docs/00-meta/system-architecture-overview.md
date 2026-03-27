---
系统全景图 - Rust 量化交易系统
版本: 2026-03-27
状态: 活跃
---

# 系统全景图

## 第一层：物理结构

系统由九个物理层级构成。最底层是 **a_common** 工具层，为所有上层提供通用能力。

```
a_common          → 基础工具层（错误类型、配置、日志）
b_data_source     → 数据源层（WS、REST API、存储）
c_data_process    → 数据处理层（信号处理、波动率计算）
d_checktable      → 交易记录层（订单管理、状态机、WAL）
e_risk_monitor    → 风控监控层（风控检查、阈值管理）
f_engine          → 交易引擎层（策略决策、订单生成）
g_test            → 测试层（集成测试）
h_sandbox         → 沙盒层（模拟环境、回放）
x_data            → 外部数据层（仓位、资金）
```

文件组织结构（`crates/` 目录）：
- `a_common/src/lib.rs` - 基础类型导出
- `b_data_source/src/api/data_feeder.rs` - DataFeeder 统一数据接口
- `c_data_process/src/processor.rs` - SignalProcessor 信号处理
- `d_checktable/src/h_15m/trader.rs` - Trader 品种交易主循环
- `e_risk_monitor/src/` - 风控检查器实现
- `f_engine/src/core/engine.rs` - EventDrivenEngine 事件驱动引擎
- `h_sandbox/src/lib.rs` - 沙盒模块入口

---

## 第二层：逻辑架构

六层架构的逻辑划分遵循 **数据流单向** 原则，从数据源到引擎，职责逐层抽象。

### 层职责

| 层级 | 名称 | 核心职责 | 关键文件 |
|-----|------|---------|---------|
| a | 工具层 | 错误类型、配置、宏定义 | `a_common/src/` |
| b | 数据源层 | 数据注入、订阅、K线合成 | `b_data_source/src/api/` |
| c | 数据处理层 | 信号计算、波动率 | `c_data_process/src/` |
| d | 检查表层 | 订单状态机、WAL持久化 | `d_checktable/src/h_15m/` |
| e | 风控层 | 风控阈值、预检 | `e_risk_monitor/src/` |
| f | 引擎层 | 策略决策、订单生成 | `f_engine/src/core/` |

### 调用关系

```
沙盒层 (h_sandbox)
    ├── ShadowBinanceGateway → 模拟交易所响应
    ├── StreamTickGenerator → 历史数据回放
    └── ShadowRiskChecker → 影子风控

        ↓ push_tick(tick: Tick)
        
数据层 (b_data_source)
    └── DataFeeder → 统一数据接口
        ├── subscribe_1m(symbol, tx)
        ├── subscribe_15m(symbol, tx)
        └── push_tick(tick)

        ↓ Tick 流入 Channel

引擎层 (f_engine)
    └── EventDrivenEngine → 单事件循环
        ├── run(tick_rx) → while let Some(tick) = tick_rx.recv().await
        ├── on_tick(tick) → 串行处理链
        ├── decide() → 策略决策
        └── submit_order() → 异步下单

        ↓ 检查风控

风控层 (e_risk_monitor)
    └── RiskChecker trait
        ├── pre_check(order, account)
        └── scan(positions, account)
```

---

## 第三层：数据流动

市场数据从进入系统到触发交易的完整旅程：

### Tick 生命周期

```
T0: StreamTickGenerator::next()
    位置: crates/h_sandbox/src/historical_replay/mod.rs
    行为: 从历史数据读取 K线，转换为 Tick

T1: tick_tx.send(tick).await
    类型: tokio::sync::mpsc::Sender<Tick>
    背压: channel(1024) 满时 send() 阻塞

T2: tick_rx.recv().await
    位置: src/sandbox_main.rs:298
    行为: 阻塞等待，无轮询

T3: on_tick(tick) 串行执行
    ├── update_indicators(&tick)  → 增量计算 EMA/RSI
    ├── decide(&tick)             → 策略信号生成
    ├── check_risk(&decision)     → 风控预检
    └── submit_order(order)       → 异步下单

T4: 返回循环，继续 recv().await
```

### 数据形态变化

```
原始数据 (CSV/Shard)
    ↓ TickToWsConverter
Tick { symbol, price, kline_1m, kline_15m }
    ↓ DataFeeder::push_tick()
Channel: mpsc::Sender<Tick>
    ↓
EventDrivenEngine::run()
    ↓
TradingDecision { action, price, qty, reason }
    ↓
OrderRequest { symbol, side, order_type, qty, price }
```

---

## 第四层：执行模型

系统采用 **生产者-消费者** 并行模型：

```
┌─────────────────────────────────────────────────────────────┐
│  tokio::join! {                                          │
│      // 生产者（1个 async 块）                             │
│      async {                                             │
│          while let Some(tick) = generator.next() {        │
│              tick_tx.send(tick).await;  // 背压点         │
│          }                                               │
│      },                                                  │
│                                                             │
│      // 消费者（1个 TradingEngine::run）                    │
│      async {                                             │
│          engine.run(tick_rx).await;                       │
│      }                                                   │
│  }                                                       │
└─────────────────────────────────────────────────────────────┘
```

### 并发约束

| 约束 | 实现 | 位置 |
|-----|------|------|
| 零 spawn | 无 tokio::spawn | sandbox_main.rs |
| 零 sleep | 无 tokio::time::sleep | sandbox_main.rs |
| 零轮询 | recv().await 阻塞 | sandbox_main.rs:298 |
| 串行保序 | 单 async 块内同步执行 | sandbox_main.rs:300 |
| 背压控制 | channel send().await | sandbox_main.rs:587 |

### Channel 配置

```rust
// 位置: src/sandbox_main.rs:574
let (tick_tx, tick_rx) = mpsc::channel(1024);
//       ↑ 容量 1024，用于背压控制
```

---

## 第五层：接口契约

### DataFeeder 接口

```rust
// 位置: crates/b_data_source/src/api/data_feeder.rs

/// 订阅 1m Tick 数据流
pub fn subscribe_1m(&self, symbol: &str, tx: mpsc::Sender<Tick>)

/// 订阅 15m Tick 数据流  
pub fn subscribe_15m(&self, symbol: &str, tx: mpsc::Sender<Tick>)

/// 推送 Tick（自动广播给订阅者）
pub fn push_tick(&self, tick: Tick)
```

### EventDrivenEngine 接口

```rust
// 位置: crates/f_engine/src/core/engine.rs

/// 创建引擎
pub fn new(symbol: String) -> Self

/// 单事件循环
pub async fn run(&mut self, tick_rx: mpsc::Receiver<Tick>)

/// 获取统计
pub fn stats(&self) -> &EngineStats
```

### RiskChecker 接口

```rust
// 位置: crates/f_engine/src/interfaces/risk.rs

pub trait RiskChecker: Send + Sync {
    fn pre_check(&self, order: &OrderRequest, account: &ExchangeAccount) -> RiskCheckResult;
    fn post_check(&self, order: &ExecutedOrder, account: &ExchangeAccount) -> RiskCheckResult;
    fn scan(&self, positions: &[PositionInfo], account: &ExchangeAccount) -> Vec<RiskWarning>;
    fn thresholds(&self) -> RiskThresholds;
}
```

---

## 第六层：状态管理

### 共享存储模式

沙盒层通过 **共享 Store 实例** 同步数据：

```rust
// 位置: crates/h_sandbox/src/context.rs

pub struct SandboxContext {
    /// 共享市场数据存储
    pub store: Arc<MarketDataStore>,
    /// 共享账户模拟
    pub account: Arc<RwLock<ShadowAccount>>,
}
```

### 引擎内部状态

```rust
// 位置: crates/f_engine/src/core/engine.rs

struct EngineState {
    /// 指标缓存（按品种）
    indicators: HashMap<String, IndicatorData>,
    /// 持仓状态
    positions: HashMap<String, PositionState>,
    /// 统计
    stats: EngineStats,
}

#[derive(Debug, Clone, Default)]
struct IndicatorData {
    ema5: Option<Decimal>,
    ema20: Option<Decimal>,
    rsi: Option<Decimal>,
    price_history: Vec<Decimal>,
}
```

### 原子操作

```rust
// 位置: crates/d_checktable/src/h_15m/trader.rs

is_running: AtomicBool,    // 运行状态
last_order_ms: AtomicU64, // 频率限制
```

---

## 第七层：边界处理

### 数据缺失处理

| 场景 | 处理方式 | 依据 |
|-----|---------|------|
| K线未就绪 | 返回 None，策略跳过 | `crates/b_data_source/src/api/data_feeder.rs` |
| 指标计算不足 | 返回 None，策略跳过 | `f_engine/src/core/engine.rs:200-210` |
| 账户查询失败 | 返回 Err，订单跳过 | `f_engine/src/core/engine.rs:256-260` |

### 错误传播链

```
沙盒注入假数据? → 禁止（沙盒边界规则）
    ↓

数据缺失
    ↓
Trader 返回 None
    ↓
Engine 跳过该 Tick
    ↓
继续处理下一个 Tick
```

### 风控拦截

```rust
// 位置: crates/f_engine/src/core/engine.rs:255-270

fn check_risk(&self, decision: &TradingDecision) -> Option<OrderRequest> {
    // 构造订单
    let order = OrderRequest { ... };
    
    // 简化风控：只检查数量
    if order.qty <= Decimal::ZERO {
        return None;
    }
    
    Some(order)
}
```

---

## 第八层：设计原则

### 核心设计原则

1. **事件驱动优于轮询**
   - `recv().await` 阻塞等待，而非 `loop + sleep`
   - 背压由 channel 自动处理

2. **串行优于并发**
   - 单事件循环，无 spawn
   - Tick 处理顺序天然保序

3. **沙盒边界清晰**
   - 沙盒只注入数据，不计算指标
   - 业务逻辑在引擎内执行

4. **向后兼容**
   - 旧接口标记 `#[deprecated]`
   - 新接口独立实现

### 废弃接口清单

| 接口 | 位置 | 替代方案 |
|-----|------|---------|
| `DataFeeder::ws_get_1m()` | b_data_source | `subscribe_1m()` |
| `SignalProcessor::start_loop()` | c_data_process | `cleanup_expired()` |
| `Trader::start_gc_task()` | d_checktable | `gc_pending()` |
| `TraderManager::start_trader()` | f_engine | `EventDrivenEngine::run()` |

### 关键权衡

| 权衡点 | 选择 | 理由 |
|-------|------|------|
| 多品种 vs 单品种 | 当前多品种 Manager，逐步迁移单品种 Engine | 向后兼容 |
| 同步 vs 异步执行 | on_tick 内同步，submit_order 异步 | 简化状态管理 |
| Trait vs 具体类型 | EventDrivenEngine 使用具体类型 | 简化 API |

---

## 附录：关键文件索引

| 文件 | 作用 | 行号 |
|-----|------|------|
| `src/sandbox_main.rs` | 沙盒入口，事件循环 | 298 |
| `crates/b_data_source/src/api/data_feeder.rs` | 数据订阅接口 | 85-130 |
| `crates/f_engine/src/core/engine.rs` | 事件驱动引擎 | 90-130 |
| `crates/h_sandbox/src/gateway/mod.rs` | 影子网关 | - |
| `crates/d_checktable/src/h_15m/trader.rs` | 交易主循环 | 1341 |
| `crates/c_data_process/src/processor.rs` | 信号处理 | 567 |
