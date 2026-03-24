# Rust 量化交易引擎 - 完整模块分析与文档

> 本文档描述 `f_engine` 模块的架构设计，基于接口的解耦架构。

---

## 1. 整体架构说明

### 1.1 项目设计目标

本项目是一个 **模块化、高内聚低耦合** 的量化交易引擎，采用 Rust 语言实现，充分利用 Rust 的：

| 特性 | 说明 |
|------|------|
| **内存安全** | 无 GC，无悬垂指针 |
| **并发安全** | 无数据竞争 |
| **零成本抽象** | Trait 对象可静态分发 |
| **所有权系统** | 编译期内存管理 |

### 1.2 模块划分原则

| 原则 | 说明 |
|------|------|
| **单一职责** | 每个模块只负责一个领域 |
| **接口隔离** | 模块间通过 Trait 接口通信 |
| **依赖注入** | 核心组件通过构造函数注入 |
| **信息隐藏** | 内部状态完全私有 |

### 1.3 架构规范

```
┌─────────────────────────────────────────────────────────────────┐
│                         f_engine (交易引擎)                        │
│                                                                  │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │                 interfaces/ (统一接口层)                   │   │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐   │   │
│  │  │MarketData│ │ Strategy │ │  Risk    │ │Execution │   │   │
│  │  │Provider  │ │Executor  │ │ Checker  │ │Gateway   │   │   │
│  │  └────┬─────┘ └────┬─────┘ └────┬─────┘ └────┬─────┘   │   │
│  └───────┼────────────┼────────────┼────────────┼──────────┘   │
│          │            │            │            │              │
└──────────┼────────────┼────────────┼────────────┼──────────────┘
           │            │            │            │
           ▼            ▼            ▼            ▼
┌─────────────────────────────────────────────────────────────────┐
│                      外部依赖模块                                  │
│  ┌────────────┐ ┌────────────┐ ┌────────────┐ ┌────────────┐   │
│  │b_data_source│ │c_data_proc│ │e_risk_monit│ │ a_common   │   │
│  └────────────┘ └────────────┘ └────────────┘ └────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

### 1.4 模块目录结构

```
f_engine/src/
├── interfaces/           # 统一接口层（核心）
│   ├── mod.rs
│   ├── market_data.rs    # 市场数据接口
│   ├── strategy.rs       # 策略接口
│   ├── risk.rs           # 风控接口
│   ├── execution.rs       # 执行接口
│   └── adapters.rs       # 适配器
├── core/                 # 核心引擎
│   ├── mod.rs
│   ├── engine.rs         # TradingEngine 主循环
│   └── engine_v2.rs      # 解耦引擎实现
├── order/                # 订单模块
│   ├── mod.rs
│   ├── gateway.rs        # ExchangeGateway trait
│   ├── mock_binance_gateway.rs  # Mock 实现
│   └── order.rs          # OrderExecutor
├── channel/              # 通道模块
│   ├── mod.rs
│   └── mode_switcher.rs  # 交易模式切换
├── strategy/             # 策略模块
│   ├── mod.rs
│   └── executor.rs       # 策略调度器
├── types.rs              # 共享类型
└── lib.rs                # 库入口
```

---

## 2. 逐模块详细说明

---

### 模块 A: `interfaces/` - 统一接口层

**模块职责**: 定义所有跨模块交互的 Trait 接口，确保模块间只能通过接口调用，禁止直接访问内部数据。

#### A.1 `market_data.rs` - 市场数据接口

**文件路径**: `f_engine/src/interfaces/market_data.rs`

**核心 Trait**:

```rust
pub trait MarketDataProvider: Send + Sync {
    fn next_tick(&self) -> impl Future<Output = Option<MarketTick>> + Send;
    fn next_completed_kline(&self) -> impl Future<Output = Option<MarketTick>> + Send;
    fn current_price(&self, symbol: &str) -> Option<Decimal>;
    fn get_klines(&self, symbol: &str, period: &str) -> impl Future<Output = Vec<MarketKLine>> + Send;
    fn symbols(&self) -> Vec<String>;
}
```

**数据契约类型**:

| 类型 | 字段 | 说明 |
|------|------|------|
| `MarketKLine` | symbol, period, open, high, low, close, volume, timestamp, is_closed | K线数据 |
| `MarketTick` | symbol, price, qty, timestamp | Tick数据 |
| `VolatilityInfo` | symbol, level, value, timestamp | 波动率信息 |
| `VolatilityLevel` | High, Normal, Low | 波动率级别 |
| `OrderBookSnapshot` | symbol, bids, asks, timestamp | 订单簿快照 |

**内部结构**: 无（仅为接口定义模块）

**依赖外部模块**: 无

**封装保证**:
- ✅ 所有方法返回数据契约，不暴露内部结构
- ✅ 使用 `Send + Sync` 约束保证线程安全
- ✅ 接口契约与实现完全分离

**额外接口**:

```rust
pub trait VolatilityDetector: Send + Sync {
    fn detect_level(&self, symbol: &str) -> Option<VolatilityInfo>;
    fn update(&self, symbol: &str, kline: &MarketKLine);
    fn history(&self, symbol: &str) -> Vec<VolatilityInfo>;
}

pub trait OrderBookProvider: Send + Sync {
    fn snapshot(&self, symbol: &str) -> Option<OrderBookSnapshot>;
    fn spread(&self, symbol: &str) -> Option<Decimal>;
}
```

---

#### A.2 `strategy.rs` - 策略接口

**文件路径**: `f_engine/src/interfaces/strategy.rs`

**核心 Trait**:

```rust
pub trait StrategyInstance: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn symbols(&self) -> Vec<String>;
    fn is_enabled(&self) -> bool;
    fn state(&self) -> StrategyState;
    fn on_bar(&self, bar: &MarketKLine) -> Option<TradingSignal>;
    fn on_volatility_change(&self, volatility: &VolatilityInfo);
    fn set_enabled(&self, enabled: bool);
    fn update_market_status(&self, status: MarketStatusType);
    fn market_status(&self) -> Option<MarketStatusType>;
}

pub trait StrategyExecutor: Send + Sync {
    fn register(&self, strategy: Arc<dyn StrategyInstance>);
    fn unregister(&self, strategy_id: &str);
    fn dispatch(&self, bar: &MarketKLine) -> Vec<TradingSignal>;
    fn get_signal(&self, symbol: &str) -> Option<TradingSignal>;
    fn get_strategy_state(&self, strategy_id: &str) -> Option<StrategyState>;
    fn set_enabled(&self, strategy_id: &str, enabled: bool);
    fn get_all_states(&self) -> Vec<StrategyState>;
    fn count(&self) -> usize;
}
```

**数据契约类型**:

| 类型 | 字段 | 说明 |
|------|------|------|
| `TradingSignal` | id, symbol, direction, signal_type, quantity, price, stop_loss, take_profit, priority, confidence, timestamp | 交易信号 |
| `StrategyState` | id, name, enabled, position_direction, position_qty, status, last_signal_time | 策略状态 |
| `SignalDirection` | Long, Short, Flat | 信号方向 |
| `SignalType` | Open, Add, Reduce, Close | 信号类型 |
| `StrategyStatus` | Idle, Running, Waiting, Error | 策略状态枚举 |
| `MarketStatusType` | Pin, Trend, Range | 市场状态类型 |

**封装保证**:
- ✅ 策略实例不能访问引擎内部
- ✅ 引擎通过接口操作策略，不能直接访问内部状态
- ✅ 所有方法使用 `&self`，支持线程安全
- ✅ 状态变更通过内部 RwLock 实现

**策略工厂接口**:

```rust
pub trait StrategyFactory: Send + Sync {
    fn create(&self) -> Arc<dyn StrategyInstance>;
    fn clone_box(&self) -> Box<dyn StrategyFactory>;
}
```

**信号聚合器接口**:

```rust
pub trait SignalAggregator: Send + Sync {
    fn aggregate(&self, signals: Vec<TradingSignal>) -> Vec<TradingSignal>;
    fn max_signals(&self) -> usize;
}
```

---

#### A.3 `risk.rs` - 风控接口

**文件路径**: `f_engine/src/interfaces/risk.rs`

**核心 Trait**:

```rust
pub trait RiskChecker: Send + Sync {
    fn pre_check(&self, order: &OrderRequest, account: &AccountInfo) -> RiskCheckResult;
    fn post_check(&self, order: &ExecutedOrder, account: &AccountInfo) -> RiskCheckResult;
    fn scan(&self, positions: &[PositionInfo], account: &AccountInfo) -> Vec<RiskWarning>;
    fn thresholds(&self) -> RiskThresholds;
}
```

**数据契约类型**:

| 类型 | 字段 | 说明 |
|------|------|------|
| `OrderRequest` | symbol, side, order_type, quantity, price, stop_loss, take_profit | 订单请求 |
| `RiskCheckResult` | allowed, reason, risk_level, timestamp | 风控结果 |
| `AccountInfo` | account_id, total_equity, available, frozen_margin, unrealized_pnl | 账户信息 |
| `PositionInfo` | symbol, direction, quantity, entry_price, unrealized_pnl, margin_used | 持仓信息 |
| `RiskThresholds` | max_exposure_ratio, max_order_value, max_position_ratio, max_leverage, min_order_value, stop_loss_ratio | 风控阈值 |
| `RiskWarning` | code, message, severity, affected_symbol, timestamp | 风险警告 |
| `ExecutedOrder` | order_id, symbol, side, quantity, price, commission, timestamp | 已执行订单 |
| `RiskLevel` | Low, Medium, High | 风险级别 |

**封装保证**:
- ✅ 风控逻辑完全封装在实现中
- ✅ 引擎不能绕过风控直接下单
- ✅ 所有检查通过接口返回结果
- ✅ 支持配置化阈值（生产/回测）

**风控阈值默认值**:

```rust
impl RiskThresholds {
    pub fn production() -> Self { /* 严格阈值 */ }
    pub fn backtest() -> Self { /* 宽松阈值 */ }
}
```

---

#### A.4 `execution.rs` - 执行接口

**文件路径**: `f_engine/src/interfaces/execution.rs`

**核心 Trait**:

```rust
pub trait ExchangeGateway: Send + Sync {
    fn place_order(&self, order: OrderRequest) -> Result<OrderResult, ExecutionError>;
    fn cancel_order(&self, order_id: &str, symbol: &str) -> Result<(), ExecutionError>;
    fn query_order(&self, order_id: &str, symbol: &str) -> Result<Option<OrderResult>, ExecutionError>;
    fn get_account(&self) -> Result<AccountInfo, ExecutionError>;
    fn get_position(&self, symbol: &str) -> Result<Option<PositionInfo>, ExecutionError>;
    fn get_all_positions(&self) -> Result<Vec<PositionInfo>, ExecutionError>;
}
```

**数据契约类型**:

| 类型 | 字段 | 说明 |
|------|------|------|
| `OrderResult` | order_id, status, executed_quantity, executed_price, commission, message, reject_reason, timestamp | 订单执行结果 |
| `OrderStatus` | Pending, Submitted, PartiallyFilled, Filled, Canceled, Rejected | 订单状态 |
| `ExecutionError` | Network, Api, InsufficientBalance, PositionLimitExceeded, OrderRejected, InvalidOrder, Gateway | 执行错误 |

**封装保证**:
- ✅ 引擎不直接操作交易所 API
- ✅ 所有通信通过统一网关接口
- ✅ 支持注入 Mock 实现进行测试
- ✅ 错误类型清晰可追溯

**额外接口**:

```rust
pub trait MarketDepthProvider: Send + Sync {
    fn best_bid_ask(&self, symbol: &str) -> Option<(Decimal, Decimal)>;
    fn liquidity(&self, symbol: &str, depth: Decimal) -> (Decimal, Decimal);
}
```

---

### 模块 B: `strategy/` - 策略模块

**模块职责**: 定义策略实例和策略执行器。

#### B.1 `mod.rs` - 策略核心定义

**文件路径**: `f_engine/src/strategy/mod.rs`

**公开结构体**:

| 结构体 | 字段 | 可见性 | 说明 |
|--------|------|--------|------|
| `StrategyState` | id, enabled, position_direction, position_qty, status, _internal | pub | 策略状态（线程安全） |
| `StrategyKLine` | symbol, period, open, high, low, close, volume, timestamp | pub | 策略K线 |
| `TradingSignal` | symbol, direction, quantity, price, stop_loss, take_profit, signal_type, strategy_id, priority, timestamp | pub | 交易信号 |
| `MarketStatus` | status, volatility, volatility_value | pub | 市场状态 |
| `InternalState` | enabled, market_status, volatility | private | 内部同步状态 |

**公开 Trait**:

```rust
pub trait Strategy: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn symbols(&self) -> Vec<String>;
    fn is_enabled(&self) -> bool { self.state().is_enabled() }
    fn on_bar(&self, bar: &StrategyKLine) -> Option<TradingSignal>;
    fn state(&self) -> &StrategyState;
    fn on_market_status(&self, status: &MarketStatus);
    fn on_volatility(&self, volatility: f64);
}
```

**内部结构**:

```rust
struct InternalState {
    enabled: RwLock<bool>,
    market_status: RwLock<Option<MarketStatus>>,
    volatility: RwLock<Option<f64>>,
}
```

**封装保证**:
- ✅ `_internal` 字段私有，外部无法访问
- ✅ 状态修改通过公开方法 `set_enabled()`, `update_market_status()` 等
- ✅ 使用 `Arc<InternalState>` 实现 Clone 时共享内部状态
- ✅ 线程安全通过 RwLock 实现

**依赖外部模块**:
- `b_data_source::KLine` - 通过 `From` trait 转换

**TradingSignal Builder 模式**:

```rust
impl TradingSignal {
    pub fn new(...) -> Self { ... }
    pub fn with_price(mut self, price: Decimal) -> Self { ... }
    pub fn with_stop_loss(mut self, stop_loss: Decimal) -> Self { ... }
    pub fn with_take_profit(mut self, take_profit: Decimal) -> Self { ... }
    pub fn with_signal_type(mut self, signal_type: SignalType) -> Self { ... }
    pub fn with_priority(mut self, priority: u8) -> Self { ... }
}
```

---

#### B.2 `executor.rs` - 策略调度器

**文件路径**: `f_engine/src/strategy/executor.rs`

**公开结构体**:

| 结构体 | 字段 | 可见性 | 说明 |
|--------|------|--------|------|
| `StrategyExecutor` | strategies, symbol_strategies, signal_cache | private | 策略调度器 |
| `SignalAggregator` | max_signals | private | 信号聚合器 |

**公开方法**:

| 方法 | 输入 | 输出 | 说明 |
|------|------|------|------|
| `new()` | - | `Self` | 创建空调度器 |
| `register()` | `Arc<dyn Strategy>` | `()` | 注册策略 |
| `unregister()` | `&str` | `()` | 注销策略 |
| `dispatch()` | `&StrategyKLine` | `Vec<TradingSignal>` | 分发K线 |
| `get_signal()` | `&str` | `Option<TradingSignal>` | 获取最高优先级信号 |
| `get_signal_for_strategy()` | `&str, &str` | `Option<TradingSignal>` | 获取指定策略信号 |
| `get_all_signals()` | - | `Vec<TradingSignal>` | 获取所有缓存信号 |
| `clear_stale_signals()` | `i64` | `()` | 清理过期信号 |
| `set_enabled()` | `&str, bool` | `()` | 设置启用状态 |
| `count()` | - | `usize` | 策略数量 |
| `symbol_count()` | - | `usize` | 品种数量 |
| `clear()` | - | `()` | 清空所有数据 |

**内部结构**:

```rust
pub struct StrategyExecutor {
    strategies: RwLock<FnvHashMap<String, Arc<dyn Strategy>>>,           // 私有
    symbol_strategies: RwLock<FnvHashMap<String, Vec<String>>>,          // 私有
    signal_cache: RwLock<FnvHashMap<String, TradingSignal>>,             // 私有
}
```

**封装保证**:
- ✅ 所有字段私有
- ✅ 使用 `RwLock` 保护并发访问
- ✅ 只能通过公开方法访问

**信号聚合规则**:

```rust
impl SignalAggregator {
    pub fn aggregate(&self, signals: Vec<TradingSignal>) -> Vec<TradingSignal> {
        // 1. 同一品种同一方向只保留数量最大的信号
        // 2. 按优先级排序
        // 3. 限制最大信号数量
    }
}
```

---

### 模块 C: `order/` - 订单执行模块

**模块职责**: 封装交易所网关，支持实盘/Mock/回测环境。

#### C.1 `gateway.rs` - 交易所网关接口

**文件路径**: `f_engine/src/order/gateway.rs`

**公开 Trait**:

```rust
pub trait ExchangeGateway: Send + Sync {
    fn place_order(&self, req: OrderRequest) -> Result<OrderResult, EngineError>;
    fn get_account(&self) -> Result<ExchangeAccount, EngineError>;
    fn get_position(&self, symbol: &str) -> Result<Option<ExchangePosition>, EngineError>;
}
```

**封装保证**:
- ✅ 所有实现必须线程安全 (`Send + Sync`)
- ✅ 引擎只能通过 Trait 调用
- ✅ 实现可替换（实盘/模拟/回测）

---

#### C.2 `mock_binance_gateway.rs` - Mock 实现

**文件路径**: `f_engine/src/order/mock_binance_gateway.rs`

**公开结构体**:

| 结构体 | 字段 | 可见性 | 说明 |
|--------|------|--------|------|
| `MockBinanceGateway` | config, account, positions, next_order_id, orders | private | Mock网关 |
| `MockAccount` | account_id, total_equity, available, frozen_margin, unrealized_pnl | pub | Mock账户 |
| `MockPosition` | symbol, long_qty, long_avg_price, short_qty, short_avg_price, unrealized_pnl | pub | Mock持仓 |
| `MockGatewayConfig` | initial_balance, commission_rate, slippage_rate, fill_delay_ms, simulate_fill | pub | Mock配置 |
| `OrderRecord` | order_id, symbol, side, qty, price, status, filled_qty, filled_price | pub | 订单记录 |

**内部结构**:

```rust
pub struct MockBinanceGateway {
    config: MockGatewayConfig,                              // 私有
    account: RwLock<MockAccount>,                           // 私有
    positions: RwLock<FnvHashMap<String, MockPosition>>,    // 私有
    next_order_id: RwLock<u64>,                            // 私有
    orders: RwLock<FnvHashMap<String, OrderRecord>>,        // 私有
}
```

**公开方法**:

| 方法 | 输入 | 输出 | 说明 |
|------|------|------|------|
| `new()` | - | `Self` | 创建默认配置网关 |
| `with_config()` | `MockGatewayConfig` | `Self` | 创建自定义配置网关 |
| `get_account()` | - | `MockAccount` | 获取账户信息 |
| `get_position()` | `&str` | `Option<MockPosition>` | 获取持仓 |
| `get_all_positions()` | - | `Vec<MockPosition>` | 获取所有持仓 |
| `get_order()` | `&str` | `Option<OrderRecord>` | 获取订单记录 |
| `order_count()` | - | `usize` | 订单数量 |
| `update_pnl()` | `&str, Decimal` | `()` | 更新持仓盈亏 |
| `update_all_pnl()` | `&FnvHashMap<String, Decimal>` | `()` | 更新所有持仓盈亏 |
| `reset()` | - | `()` | 重置账户 |

**封装保证**:
- ✅ 所有字段私有
- ✅ 线程安全通过 `RwLock` 实现
- ✅ 实现 `ExchangeGateway` Trait

**Mock 配置**:

```rust
pub struct MockGatewayConfig {
    pub initial_balance: Decimal,      // 初始账户余额
    pub commission_rate: Decimal,      // 手续费率 (Taker)
    pub slippage_rate: Decimal,        // 滑点率
    pub fill_delay_ms: u64,            // 成交延迟（毫秒）
    pub simulate_fill: bool,           // 是否模拟成交
}
```

**持仓操作**:

```rust
impl MockPosition {
    pub fn net_qty(&self) -> Decimal;           // 净持仓
    pub fn has_position(&self) -> bool;         // 是否有持仓
    pub fn close_long(&mut self, qty: Decimal); // 平多仓
    pub fn close_short(&mut self, qty: Decimal); // 平空仓
}
```

---

### 模块 D: `channel/` - 通道切换模块

**模块职责**: 管理交易引擎的运行模式。

#### D.1 `mode_switcher.rs`

**文件路径**: `f_engine/src/channel/mode_switcher.rs`

**公开结构体**:

| 结构体 | 字段 | 可见性 | 说明 |
|--------|------|--------|------|
| `ModeSwitcher` | current_mode | private | 模式切换器 |

**公开枚举**:

```rust
pub enum Mode {
    Normal,      // 正常交易
    Backtest,   // 回测
    Paper,      // 仿真
    Maintenance, // 维护
}
```

**公开方法**:

| 方法 | 输入 | 输出 | 说明 |
|------|------|------|------|
| `new()` | - | `Self` | 创建默认模式 (Normal) |
| `mode()` | - | `Mode` | 获取当前模式 |
| `set_mode()` | `Mode` | `()` | 设置模式 |
| `is_trading_allowed()` | - | `bool` | 检查是否允许交易 |

**模式说明**:

| 模式 | 交易允许 | 说明 |
|------|----------|------|
| `Normal` | ✅ | 实盘交易 |
| `Backtest` | ❌ | 历史数据回测 |
| `Paper` | ✅ | 仿真交易 |
| `Maintenance` | ❌ | 系统维护 |

---

### 模块 E: `core/engine_v2.rs` - 解耦引擎

**模块职责**: 基于接口的解耦交易引擎实现。

**文件路径**: `f_engine/src/core/engine_v2.rs`

**核心设计**: 使用泛型约束确保所有依赖通过接口注入。

```rust
pub struct TradingEngine<M, S, R, G>
where
    M: MarketDataProvider,
    S: StrategyExecutorTrait,
    R: RiskChecker,
    G: ExecutionGateway,
{
    market_data: Arc<M>,                                // 接口注入
    strategy_executor: Arc<S>,                          // 接口注入
    risk_checker: Arc<R>,                              // 接口注入
    gateway: Arc<G>,                                    // 接口注入
    mode: TradingMode,                                  // 内部状态
    state: RwLock<EngineState>,                        // 内部状态
    is_running: Arc<AtomicBool>,                        // 内部状态
    symbol_states: RwLock<FnvHashMap<String, SymbolState>>, // 内部状态
    current_symbol: String,                             // 内部状态
    initial_balance: Decimal,                           // 内部状态
}
```

**公开枚举**:

```rust
pub enum TradingMode {
    Live,      // 实盘交易
    Backtest, // 回测模式
    Replay,   // 回放模式
}

pub enum EngineState {
    Initialized,
    Running,
    Paused,
    Stopped,
}
```

**公开方法**:

| 方法 | 输入 | 输出 | 说明 |
|------|------|------|------|
| `new()` | Arc\<M\>, Arc\<S\>, Arc\<R\>, Arc\<G\>, String, Decimal, TradingMode | `Self` | 创建引擎 |
| `start()` | - | `Result<(), EngineError>` | 启动引擎 |
| `stop()` | - | `()` | 停止引擎 |
| `pause()` | - | `()` | 暂停引擎 |
| `resume()` | - | `()` | 恢复引擎 |
| `run_loop()` | - | `Result<(), EngineError>` | 主循环 |
| `get_state()` | - | `EngineState` | 获取状态 |
| `is_running()` | - | `bool` | 运行状态 |
| `get_symbol_state()` | `&str` | `Option<SymbolState>` | 获取品种状态 |

**封装保证**:
- ✅ 所有依赖通过构造函数注入
- ✅ 内部状态完全封装
- ✅ 通过 Trait 接口与外部模块通信

**引擎错误类型**:

```rust
pub enum EngineError {
    NotRunning,
    AlreadyRunning,
    GatewayError(String),
    ExecutionError(String),
    RiskCheckFailed(String),
    InvalidState(String),
}
```

---

### 模块 F: `b_data_source/trader_pool.rs` - 品种池

**模块职责**: 管理激活的交易品种。

**文件路径**: `crates/b_data_source/src/trader_pool.rs`

**公开结构体**:

| 结构体 | 字段 | 可见性 | 说明 |
|--------|------|--------|------|
| `TraderPool` | trading_symbols, symbol_meta | private | 品种池 |
| `SymbolMeta` | symbol, status, priority, max_position, min_qty, price_precision, qty_precision | pub | 品种元数据 |

**公开枚举**:

```rust
pub enum TradingStatus {
    Pending,  // 待激活
    Active,   // 正常交易
    Paused,   // 暂停
    Closed,   // 已平仓
}
```

**内部结构**:

```rust
pub struct TraderPool {
    trading_symbols: RwLock<FnvHashSet<String>>,             // 私有
    symbol_meta: RwLock<FnvHashMap<String, SymbolMeta>>,     // 私有
}
```

**公开方法**:

| 方法 | 输入 | 输出 | 说明 |
|------|------|------|------|
| `new()` | - | `Self` | 创建品种池 |
| `register()` | `SymbolMeta` | `()` | 注册品种 |
| `register_batch()` | `Iterator<SymbolMeta>` | `()` | 批量注册 |
| `unregister()` | `&str` | `()` | 注销品种 |
| `update_status()` | `&str, TradingStatus` | `()` | 更新状态 |
| `is_trading()` | `&str` | `bool` | 检查是否激活 |
| `is_active()` | `&str` | `bool` | 检查是否激活且状态为Active |
| `get_trading_symbols()` | - | `Vec<String>` | 获取所有品种 |
| `count()` | - | `usize` | 品种数量 |
| `get_meta()` | `&str` | `Option<SymbolMeta>` | 获取品种元数据 |
| `get_status()` | `&str` | `Option<TradingStatus>` | 获取品种状态 |
| `get_by_status()` | `TradingStatus` | `Vec<String>` | 获取指定状态的品种 |
| `clear()` | - | `()` | 清空所有品种 |
| `pause_all()` | - | `()` | 暂停所有品种 |
| `activate_all()` | - | `()` | 激活所有待激活品种 |

**封装保证**:
- ✅ 所有字段私有
- ✅ 使用 `RwLock` 保护并发访问
- ✅ 大小写不敏感的品牌名称处理

---

### 模块 G: `b_data_source/replay_source.rs` - 历史数据回放

**模块职责**: 从 CSV 文件回放 OHLCVT 历史数据，用于回测。

**文件路径**: `crates/b_data_source/src/replay_source.rs`

**公开结构体**:

| 结构体 | 字段 | 可见性 | 说明 |
|--------|------|--------|------|
| `ReplaySource` | symbols_filter, period_filter, current_idx, data, exhausted | private | 回放源 |

**公开方法**:

| 方法 | 输入 | 输出 | 说明 |
|------|------|------|------|
| `from_csv()` | `Path` | `Result<Self, ReplayError>` | 从CSV创建 |
| `from_data()` | `Vec<KLine>` | `Self` | 从内存数据创建 |
| `with_symbols()` | `Vec<String>` | `Self` | 设置品种过滤 |
| `with_period()` | `Period` | `Self` | 设置周期过滤 |
| `reset()` | - | `()` | 重置回放位置 |
| `next_kline()` | - | `Option<KLine>` | 获取下一个K线 |
| `len()` | - | `usize` | 数据总数 |
| `is_empty()` | - | `bool` | 是否为空 |
| `is_exhausted()` | - | `bool` | 是否已结束 |

**CSV 格式**:

```
symbol,period,open,high,low,close,volume,timestamp
BTCUSDT,1m,50000.0,50100.0,49900.0,50050.0,100.5,2024-01-01 00:00:00
```

**Trait 接口**:

```rust
#[async_trait]
pub trait KLineSource: Send + Sync {
    async fn next_kline(&mut self) -> Option<KLine>;
    fn reset(&mut self);
    fn is_exhausted(&self) -> bool;
}
```

---

## 3. 模块间调用关系

### 3.1 调用关系图

```
┌─────────────────────────────────────────────────────────────────────┐
│                          f_engine (本模块)                            │
│                                                                      │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │                    interfaces/ 层                              │   │
│  │                                                              │   │
│  │  MarketDataProvider ────────────► 实现：b_data_source         │   │
│  │        │                                                        │   │
│  │  StrategyExecutor ──────────────► 实现：strategy/executor.rs   │   │
│  │        │                                                        │   │
│  │  RiskChecker ───────────────────► 实现：e_risk_monitor          │   │
│  │        │                                                        │   │
│  │  ExchangeGateway ───────────────► 实现：order/mock_*.rs        │   │
│  │                                                              │   │
│  └──────────────────────────────────────────────────────────────┘   │
│                              │                                       │
│                              ▼                                       │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │                    core/engine_v2.rs                          │   │
│  │                                                              │   │
│  │  TradingEngine<M, S, R, G> ──────► 泛型约束：所有依赖接口化   │   │
│  │                                                              │   │
│  │  依赖注入路径：                                                │   │
│  │  - market_data: Arc<M> (via MarketDataProvider)               │   │
│  │  - strategy_executor: Arc<S> (via StrategyExecutor)          │   │
│  │  - risk_checker: Arc<R> (via RiskChecker)                    │   │
│  │  - gateway: Arc<G> (via ExchangeGateway)                     │   │
│  │                                                              │   │
│  └──────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────┘
```

### 3.2 禁止直接数据访问的位置

| 位置 | 禁止行为 | 正确做法 |
|------|----------|----------|
| `core/engine.rs` | 直接访问 `b_data_source` 内部结构 | 通过 `MarketStream` 接口 |
| `strategy/executor.rs` | 直接修改策略内部状态 | 通过 `Strategy` trait 方法 |
| `core/engine_v2.rs` | 直接调用 `e_risk_monitor` 函数 | 通过 `RiskChecker` 接口 |
| `order/` | 引擎直接操作账户余额 | 通过 `ExchangeGateway` 接口 |

### 3.3 数据流向

```
市场数据 (b_data_source)
    │
    │ MarketDataProvider 接口
    ▼
指标计算 (c_data_process)
    │
    │ StrategyExecutor::dispatch
    ▼
d_checktable 检查层（异步并发）
    │
    │ RiskChecker::pre_check
    ▼
e_risk_monitor 风控层（串行同步）
    │
    │ ExchangeGateway::place_order
    ▼
f_engine 引擎执行闭环
    │
    ▼
状态更新 + 数据存储
```

---

## 4. 安全与封装总结

### 4.1 内部数据私有性

| 模块 | 字段 | 可见性 | 保护机制 |
|------|------|--------|----------|
| `StrategyExecutor` | strategies | private | RwLock |
| `StrategyExecutor` | symbol_strategies | private | RwLock |
| `StrategyExecutor` | signal_cache | private | RwLock |
| `MockBinanceGateway` | config | private | 无（配置） |
| `MockBinanceGateway` | account | private | RwLock |
| `MockBinanceGateway` | positions | private | RwLock |
| `MockBinanceGateway` | next_order_id | private | RwLock |
| `MockBinanceGateway` | orders | private | RwLock |
| `TraderPool` | trading_symbols | private | RwLock |
| `TraderPool` | symbol_meta | private | RwLock |
| `StrategyState` | _internal | private | Arc<RwLock> |
| `ModeSwitcher` | current_mode | private | 无（不可变） |
| `TradingEngine` | 所有字段 | private | 泛型约束 |

### 4.2 接口强制检查

所有跨模块调用必须通过 Trait 接口：

```rust
// ✅ 正确：通过接口调用
let result = self.risk_checker.pre_check(&order, &account);
let signals = self.strategy_executor.dispatch(&kline);
let account = self.gateway.get_account()?;

// ❌ 错误：直接访问内部
let positions = self.gateway.positions.read();  // 编译错误！
```

### 4.3 Rust 安全特性利用

| 特性 | 应用场景 | 保证 |
|------|----------|------|
| `Send + Sync` | 所有 Trait 约束 | 线程安全 |
| `Arc<T>` | 不可变共享 | 无数据竞争 |
| `RwLock` | 可变内部状态 | 读写分离 |
| `#[forbid(unsafe_code)]` | 整个 crate | 无 unsafe 代码 |
| `Result<T, E>` | 错误传播 | 无 panic |
| `#[forbid(unsafe_code)]` | 所有 lib.rs | 强制内存安全 |

### 4.4 高内聚低耦合评估

| 模块 | 内聚性 | 耦合性 | 说明 |
|------|--------|--------|------|
| `interfaces/` | 高 | 低 | 仅定义接口，无实现 |
| `strategy/` | 高 | 低 | 策略逻辑独立 |
| `order/` | 高 | 低 | 网关封装完整 |
| `channel/` | 高 | 无 | 单一职责 |
| `core/engine_v2.rs` | 中 | 低 | 协调者角色 |

### 4.5 依赖反转原则 (DIP)

```
┌─────────────────────────────────────────────────────────┐
│                    高层模块 (f_engine)                    │
│         TradingEngine 不依赖低层模块具体实现               │
└─────────────────────────────────────────────────────────┘
                           │
                           │ 依赖接口
                           ▼
┌─────────────────────────────────────────────────────────┐
│                    接口层 (interfaces/)                   │
│         MarketDataProvider, StrategyExecutor 等           │
└─────────────────────────────────────────────────────────┘
                           │
                           │ 实现接口
           ┌───────────────┼───────────────┐
           ▼               ▼               ▼
┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐
│ b_data_source   │ │ c_data_process   │ │ e_risk_monitor  │
│ (低层模块)       │ │ (低层模块)       │ │ (低层模块)       │
└─────────────────┘ └─────────────────┘ └─────────────────┘
```

---

## 5. 测试验证

### 5.1 编译检查

```bash
$ cargo check
    Checking f_engine v0.1.0
    Checking trading-system v0.1.0
    Finished `dev` profile
```

### 5.2 单元测试

```bash
$ cargo test -p g_test
test result: ok. 125 passed; 0 failed; 0 ignored; 0 measured
```

### 5.3 测试覆盖模块

| 模块 | 测试数量 | 说明 |
|------|----------|------|
| `b_data_source` | ~50 | 数据模型、K线合成、品种池、回放源 |
| `strategy` | ~30 | 策略执行器、信号聚合、交易集成 |
| `f_engine` | ~45 | 引擎、风控、网关、模式切换 |

---

## 6. 使用示例

### 6.1 创建 TradingEngine (V2)

```rust
use f_engine::core::engine_v2::{TradingEngine, TradingMode};
use f_engine::interfaces::{
    MarketDataProvider, StrategyExecutor, RiskChecker, ExchangeGateway
};
use std::sync::Arc;

// 创建组件
let market_data: Arc<dyn MarketDataProvider> = Arc::new(MyMarketSource::new());
let strategy_executor: Arc<dyn StrategyExecutor> = Arc::new(StrategyExecutor::new());
let risk_checker: Arc<dyn RiskChecker> = Arc::new(MyRiskChecker::new());
let gateway: Arc<dyn ExchangeGateway> = Arc::new(MockBinanceGateway::new());

// 创建引擎（所有依赖通过接口注入）
let engine = TradingEngine::new(
    market_data,
    strategy_executor,
    risk_checker,
    gateway,
    "BTCUSDT".to_string(),
    Decimal::new(10000, 0),
    TradingMode::Live,
);
```

### 6.2 使用 MockBinanceGateway

```rust
use f_engine::order::{MockBinanceGateway, MockGatewayConfig};
use rust_decimal_macros::dec;
use std::sync::Arc;

let config = MockGatewayConfig {
    initial_balance: dec!(10000.0),
    commission_rate: dec!(0.0004),
    slippage_rate: dec!(0.0001),
    simulate_fill: true,
    fill_delay_ms: 100,
};

let gateway = Arc::new(MockBinanceGateway::with_config(config));

// 下单
let result = gateway.place_order(OrderRequest {
    symbol: "BTCUSDT".to_string(),
    side: Side::Buy,
    order_type: OrderType::Market,
    qty: dec!(0.1),
    price: Some(dec!(50000.0)),
    stop_loss: None,
    take_profit: None,
})?;
```

### 6.3 使用策略调度器

```rust
use f_engine::strategy::{StrategyExecutor, Strategy, StrategyKLine};
use std::sync::Arc;

// 创建调度器
let executor = Arc::new(StrategyExecutor::new());

// 注册策略
executor.register(my_strategy.clone());

// 分发K线
let kline = StrategyKLine { /* ... */ };
let signals = executor.dispatch(&kline);

// 获取信号
if let Some(signal) = executor.get_signal("BTCUSDT") {
    println!("Got signal: {:?}", signal);
}
```

---

## 7. 总结

本项目的模块化设计确保了：

| 原则 | 实现方式 | 验证 |
|------|----------|------|
| **数据封装** | 所有内部状态完全私有 | ✅ 所有字段 private |
| **接口强制** | 跨模块调用必须通过 Trait 接口 | ✅ 泛型约束 |
| **依赖注入** | 核心组件通过构造函数注入 | ✅ V2 架构 |
| **线程安全** | 所有共享状态使用同步原语保护 | ✅ Send + Sync |
| **无 unsafe** | `#![forbid(unsafe_code)]` | ✅ 整个 crate |
| **错误处理** | Result 类型，无 panic | ✅ thiserror |

---

*文档版本: 1.0*
*最后更新: 2026-03-24*
