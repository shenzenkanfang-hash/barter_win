# Rust 量化交易系统 - 完整模块分析与文档

> 更新时间: 2026-03-24
> 状态: 完整文档

---

## 1. 整体架构说明

### 1.1 设计目标

本系统是一个基于 **Rust** 的高性能量化交易引擎，采用 **六层模块化架构**，实现：

- **市场数据处理**：WebSocket 实时行情、K线合成、波动率检测
- **指标计算**：EMA、RSI、Pine颜色、价格位置等金融指标
- **策略执行**：多周期策略（日线、分钟线）、信号聚合与分发
- **风控管理**：仓位限制、保证金池、熔断机制
- **订单执行**：统一交易所网关，支持 Mock/Real 切换
- **持久化**：SQLite 持久化 + 内存备份

### 1.2 模块划分原则

```
┌─────────────────────────────────────────────────────────────┐
│                         分层原则                             │
├─────────────────────────────────────────────────────────────┤
│  1. 每层只与上下层通信，通过接口交互                         │
│  2. 禁止跨层直接访问内部数据                                 │
│  3. 所有模块实现 Send + Sync，保证线程安全                  │
│  4. 使用依赖注入，而非直接实例化                             │
└─────────────────────────────────────────────────────────────┘
```

### 1.3 六层架构

```
┌──────────────────────────────────────────────────────────────┐
│                        a_common                              │
│     基础设施层: API/WS网关、配置、错误类型、通用工具         │
└──────────────────────────────────────────────────────────────┘
                              ↑
┌──────────────────────────────────────────────────────────────┐
│                      b_data_source                           │
│        数据层: 市场数据、K线合成、订单簿、波动率              │
└──────────────────────────────────────────────────────────────┘
                              ↑
┌──────────────────────────────────────────────────────────────┐
│                     c_data_process                           │
│              信号层: 指标计算、信号生成、Pine颜色             │
└──────────────────────────────────────────────────────────────┘
                              ↑
┌──────────────────────────────────────────────────────────────┐
│                      d_checktable                            │
│               检查层: 策略检查、同步并发执行                   │
└──────────────────────────────────────────────────────────────┘
                              ↑
┌──────────────────────────────────────────────────────────────┐
│                     e_risk_monitor                           │
│              风控层: 保证金池、风控规则、熔断                 │
└──────────────────────────────────────────────────────────────┘
                              ↑
┌──────────────────────────────────────────────────────────────┐
│                       f_engine                               │
│               引擎层: 核心运行时、订单执行、模式切换          │
└──────────────────────────────────────────────────────────────┘
                              ↑
┌──────────────────────────────────────────────────────────────┐
│                        g_test                               │
│                    测试层: 集成测试                           │
└──────────────────────────────────────────────────────────────┘
                              ↑
┌──────────────────────────────────────────────────────────────┐
│                      h_sandbox                               │
│                   沙盒层: 实验性代码                          │
└──────────────────────────────────────────────────────────────┘
```

---

## 2. 逐模块详细说明

---

### 2.1 a_common - 基础设施层

#### 模块职责
提供 API/WS 网关、配置、通用错误、数据模型等基础设施组件。所有其他模块共享的基础设施。

#### 目录结构
```
crates/a_common/src/
├── api/           # Binance REST API 网关
├── ws/            # WebSocket 连接器
├── config/        # 平台检测、路径配置
├── logs/          # 检查点日志
├── models/        # 通用数据模型
├── claint/        # 错误类型定义
├── util/          # 工具函数（Telegram通知等）
├── backup/        # 内存备份类型
├── exchange/      # 交易所通用类型
└── volatility/   # 波动率计算
```

#### 公开接口

**API 子模块** (a_common::api)
```rust
// Binance API 网关
pub struct BinanceApiGateway { ... }
impl BinanceApiGateway {
    pub fn new(api_key: &str, secret_key: &str, testnet: bool) -> Self;
    pub async fn fetch_symbol_rules(&self, symbol: &str) -> Result<SymbolRulesData, MarketError>;
    pub async fn get_account_info(&self) -> Result<BinanceAccountInfo, MarketError>;
    pub async fn get_position_risk(&self, symbol: &str) -> Result<BinancePositionRisk, MarketError>;
}

// 交易设置管理器
pub struct TradeSettingsManager { ... }
impl TradeSettingsManager {
    pub async fn get_trade_settings(&self, symbol: &str) -> Result<TradeSettings, MarketError>;
}

// 交易对规则服务
pub struct SymbolRulesService { ... }
impl SymbolRulesService {
    pub async fn get_rules(&self, symbol: &str) -> Result<ParsedSymbolRules, MarketError>;
    pub async fn get_leverage_brackets(&self, symbol: &str) -> Result<Vec<LeverageBracket>, MarketError>;
}
```

**WS 子模块** (a_common::ws)
```rust
// WebSocket 连接器
pub struct BinanceWsConnector { ... }
impl BinanceWsConnector {
    pub async fn connect(&mut self, url: &str) -> Result<(), MarketError>;
    pub async fn subscribe(&mut self, stream: &str) -> Result<(), MarketError>;
    pub fn on_message<F>(&mut self, callback: F) where F: FnMut(String) + Send + 'static;
}

// 交易数据流
pub struct BinanceTradeStream { ... }
impl BinanceTradeStream {
    pub fn next_trade(&self) -> impl Future<Output = Option<Trade>> + Send;
}
```

**配置子模块** (a_common::config)
```rust
// 平台检测
pub enum Platform { Windows, Linux }
impl Platform {
    pub fn detect() -> Self;
}

// 路径配置
pub struct Paths { ... }
impl Paths {
    pub fn data_dir(&self) -> PathBuf;
    pub fn backup_dir(&self) -> PathBuf;
    pub fn rules_dir(&self) -> PathBuf;
}
```

**日志子模块** (a_common::logs)
```rust
// 检查点日志记录器
pub trait CheckpointLogger: Send + Sync {
    fn log_checkpoint(&self, stage: Stage, result: &StageResult);
    fn get_last_checkpoint(&self) -> Option<(Stage, StageResult)>;
}
```

#### 内部结构

```rust
// 错误类型
#[derive(Debug, Clone, thiserror::Error)]
pub enum EngineError {
    #[error("Network error: {0}")]
    Network(String),
    #[error("API error: {0}")]
    Api(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum MarketError {
    #[error("Symbol not found: {0}")]
    SymbolNotFound(String),
    #[error("Rate limit exceeded")]
    RateLimitExceeded,
}
```

#### 依赖关系
- 无外部依赖（基础层）

---

### 2.2 b_data_source - 业务数据层

#### 模块职责
提供市场数据处理功能：数据订阅、K线合成、订单簿、波动率检测等。

#### 目录结构
```
crates/b_data_source/src/
├── api/
│   ├── data_feeder.rs     # 统一数据接口
│   ├── symbol_registry.rs  # 品种注册
│   ├── position.rs        # 持仓数据
│   ├── account.rs         # 账户数据
│   ├── trade_settings.rs  # 交易设置
│   └── data_sync.rs       # 数据同步
├── ws/
│   ├── kline_1m/          # 1分钟K线
│   ├── kline_1d/          # 日K线
│   ├── order_books/        # 订单簿
│   └── volatility/         # 波动率检测
├── models/
│   └── types.rs           # KLine、Tick、Period
├── recovery.rs             # 数据恢复
├── trader_pool.rs          # 品种池
├── replay_source.rs        # 历史数据回放
└── symbol_rules/          # 交易对规则
```

#### 公开接口

```rust
// 统一数据接口
pub trait DataFeeder: Send + Sync {
    fn next_tick(&self) -> impl Future<Output = Option<Tick>> + Send;
    fn current_price(&self, symbol: &str) -> Option<Decimal>;
    fn get_klines(&self, symbol: &str, period: &str) -> Vec<KLine>;
}

// 品种池
pub struct TraderPool { ... }
impl TraderPool {
    pub fn add_symbol(&self, symbol: &str, meta: SymbolMeta);
    pub fn get_meta(&self, symbol: &str) -> Option<SymbolMeta>;
    pub fn get_all_symbols(&self) -> Vec<String>;
}

// 历史数据回放
pub trait ReplaySource: Send + Sync {
    fn next_bar(&self) -> impl Future<Output = Option<KLine>> + Send;
    fn seek_to(&self, timestamp: DateTime<Utc>) -> Result<(), ReplayError>;
}

// 波动率管理器
pub struct VolatilityManager { ... }
impl VolatilityManager {
    pub fn detect(&self, symbol: &str) -> Option<SymbolVolatility>;
    pub fn update(&self, symbol: &str, kline: &KLine);
}
```

#### 数据模型

```rust
// Tick 数据
pub struct Tick {
    pub symbol: String,
    pub price: Decimal,
    pub qty: Decimal,
    pub timestamp: DateTime<Utc>,
    pub kline_1m: Option<KLine>,   // 当前1m K线
    pub kline_15m: Option<KLine>,  // 15m K线
    pub kline_1d: Option<KLine>,   // 日K线
}

// K线数据
pub struct KLine {
    pub symbol: String,
    pub period: Period,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
    pub timestamp: DateTime<Utc>,
}

// 周期枚举
pub enum Period {
    Minute(u8),  // 分钟周期，如 Minute(1), Minute(15)
    Day,         // 日线
}
```

#### K线合成器（核心组件）

```rust
pub struct KLineSynthesizer {
    pub symbol: String,
    pub period: Period,
    current: Option<KLine>,  // 私有，当前K线
}

impl KLineSynthesizer {
    // O(1) 增量更新，每次tick只更新当前K线
    pub fn update(&mut self, tick: &Tick) -> Option<KLine> {
        // 返回已完成K线（跨周期时）
    }
}
```

#### 依赖关系
```
b_data_source
    └── a_common (仅使用错误类型和配置)
```

---

### 2.3 c_data_process - 信号生成层

#### 模块职责
指标计算、信号生成、Pine颜色检测、分钟/日线市场状态判断。

#### 目录结构
```
crates/c_data_process/src/
├── pine_indicator_full.rs  # Pine v5 完整指标
├── types.rs               # 信号、市场状态类型
├── processor.rs           # 信号处理器
├── min/                   # 分钟级策略
│   ├── mod.rs
│   └── day.rs
└── day/                  # 日线级策略
    └── mod.rs
```

#### 公开接口

```rust
// Pine 颜色检测器
pub struct PineColorDetector { ... }
impl PineColorDetector {
    pub fn detect(
        &self,
        ema_diff: Decimal,
        rsi: Decimal,
        price_position: Decimal,
    ) -> PineColor;
}

// 指标处理器
pub struct SignalProcessor { ... }
impl SignalProcessor {
    pub fn process_min(
        &self,
        input: MinSignalInput,
        check_list: &CheckList,
    ) -> MinSignalOutput;

    pub fn process_day(
        &self,
        input: DaySignalInput,
        check_list: &CheckList,
    ) -> DaySignalOutput;
}

// EMA 计算
pub struct EMA { ... }
impl EMA {
    pub fn new(period: usize) -> Self;
    pub fn update(&mut self, value: Decimal) -> Decimal;  // O(1) 增量
}

// RSI 计算
pub struct RSI { ... }
impl RSI {
    pub fn new(period: usize) -> Self;
    pub fn update(&mut self, gain: Decimal, loss: Decimal) -> Decimal;
}
```

#### 核心数据类型

```rust
// 市场状态
pub enum MarketStatus {
    TREND,   // 趋势状态
    RANGE,   // 震荡状态
    PIN,     // 插针状态
    INVALID, // 数据无效
}

// Pine 颜色
pub enum PineColor {
    PureGreen,    // 纯绿
    LightGreen,   // 浅绿
    PureRed,      // 纯红
    LightRed,     // 浅红
    Purple,       // 紫色 (RSI极值)
    Neutral,      // 中性
}

// 策略信号
pub enum Signal {
    LongEntry,    // 可做多
    ShortEntry,    // 可做空
    LongHedge,     // 多头对冲
    ShortHedge,    // 空头对冲
    LongExit,      // 可平多
    ShortExit,     // 可平空
    ExitHighVol,   // 高波动退出
}

// 交易动作
pub enum TradingAction {
    Long,   // 做多
    Short,  // 做空
    Flat,   // 平仓
    Hedge,  // 对冲
    Wait,   // 等待
}

// 交易决策
pub struct TradingDecision {
    pub symbol: String,
    pub action: TradingAction,
    pub reason: String,
    pub confidence: u8,
    pub level: StrategyLevel,
    pub qty: Decimal,
    pub price: Decimal,
    pub timestamp: i64,
}
```

#### 依赖关系
```
c_data_process
    └── b_data_source (使用 KLine、Tick 数据)
```

---

### 2.4 d_checktable - 检查层

#### 模块职责
按周期组织的策略检查：高频15分钟、高频1分钟、低频1天。检查层异步并发执行，由引擎层统一调度。

#### 目录结构
```
crates/d_checktable/src/
├── check_table.rs  # 检查表
├── types.rs        # 检查类型
├── h_15m/          # 高频15分钟策略检查
│   └── mod.rs
└── l_1d/           # 低频1天策略检查
    └── mod.rs
```

#### 公开接口

```rust
// 检查表
pub struct CheckTable { ... }
impl CheckTable {
    pub fn new() -> Self;
    pub fn next_round_id(&self) -> u64;
    pub fn fill(&self, entry: CheckEntry);       // 写入检查结果
    pub fn get(&self, symbol: &str, strategy_id: &str, period: &str) -> Option<CheckEntry>;
    pub fn get_by_strategy(&self, strategy_id: &str) -> Vec<CheckEntry>;
    pub fn get_high_risk(&self) -> Vec<CheckEntry>;
    pub fn clear(&self);
}

// 检查表项
pub struct CheckEntry {
    pub symbol: String,
    pub strategy_id: String,
    pub period: String,
    pub ema_signal: Signal,
    pub rsi_value: Decimal,
    pub pine_color: PineColor,
    pub price_position: Decimal,
    pub final_signal: Signal,
    pub target_price: Decimal,
    pub quantity: Decimal,
    pub risk_flag: bool,
    pub timestamp: DateTime<Utc>,
}
```

#### 依赖关系
```
d_checktable
    └── c_data_process (使用 Signal、PineColor 类型)
```

---

### 2.5 e_risk_monitor - 风控层

#### 模块职责
保证金池管理、仓位限制、风控规则、熔断机制、动态杠杆。

#### 目录结构
```
crates/e_risk_monitor/src/
├── risk/
│   ├── common.rs       # 通用风控
│   ├── minute_risk.rs  # 分钟级风控计算
│   ├── pin.rs          # PIN风险杠杆
│   └── trend.rs        # 趋势风险限制
├── position/
│   ├── position_manager.rs   # 持仓管理
│   └── position_exclusion.rs # 持仓排除
├── persistence/
│   ├── sqlite_persistence.rs # SQLite持久化
│   └── disaster_recovery.rs   # 灾备恢复
├── shared/
│   ├── account_pool.rs      # 账户保证金池
│   ├── margin_config.rs    # 保证金配置
│   ├── market_status.rs    # 市场状态
│   ├── pnl_manager.rs      # 盈亏管理
│   └── round_guard.rs       # 回合守卫
└── lib.rs
```

#### 公开接口

**AccountPool（账户保证金池）**
```rust
pub struct AccountPool { ... }
impl AccountPool {
    // 创建
    pub fn new() -> Self;
    pub fn with_config(
        initial_balance: Decimal,
        circuit_threshold: Decimal,
        partial_circuit_threshold: Decimal,
    ) -> Self;

    // 状态查询（读锁）
    pub fn account(&self) -> parking_lot::RwLockReadGuard<'_, AccountInfo>;
    pub fn available(&self) -> Decimal;
    pub fn total_equity(&self) -> Decimal;
    pub fn circuit_state(&self) -> CircuitBreakerState;
    pub fn can_trade(&self, required_margin: Decimal) -> bool;

    // 资金操作（写锁）
    pub fn freeze(&self, amount: Decimal) -> Result<(), String>;
    pub fn unfreeze(&self, amount: Decimal);
    pub fn deduct_margin(&self, amount: Decimal) -> Result<(), String>;
    pub fn release_margin(&self, amount: Decimal);
    pub fn update_equity(&self, realized_pnl: Decimal, current_ts: i64);
    pub fn reset_circuit(&self);

    // 保证金计算
    pub fn get_account_margin(&self, level: StrategyLevel) -> AccountMargin;
}
```

**熔断状态**
```rust
pub enum CircuitBreakerState {
    Normal,   // 正常
    Partial,  // 部分熔断（限制开仓）
    Full,     // 完全熔断（禁止所有交易）
}
```

**风控检查器**
```rust
pub struct RiskPreChecker { ... }
impl RiskPreChecker {
    pub fn pre_check(&self, order: &OrderRequest, account: &AccountInfo) -> RiskCheckResult;
    pub fn post_check(&self, order: &ExecutedOrder, account: &AccountInfo) -> RiskCheckResult;
}
```

**分钟风控**
```rust
pub fn calculate_minute_open_notional(
    price: Decimal,
    qty: Decimal,
    leverage: u8,
) -> Decimal;

pub fn calculate_open_qty_from_notional(
    notional: Decimal,
    price: Decimal,
    leverage: u8,
) -> Decimal;
```

**持仓管理**
```rust
pub struct LocalPositionManager { ... }
impl LocalPositionManager {
    pub fn update_position(&self, symbol: &str, side: Direction, qty: Decimal, price: Decimal);
    pub fn get_position(&self, symbol: &str) -> Option<LocalPosition>;
    pub fn close_position(&self, symbol: &str) -> Option<(Decimal, Decimal)>;
}
```

#### 依赖关系
```
e_risk_monitor
    └── a_common (使用错误类型、备份类型)
```

---

### 2.6 f_engine - 引擎层

#### 模块职责
核心交易引擎、订单执行、策略调度、模式切换。

#### 目录结构
```
crates/f_engine/src/
├── interfaces/              # 统一接口层（核心）
│   ├── market_data.rs      # 市场数据接口
│   ├── strategy.rs          # 策略接口
│   ├── risk.rs              # 风控接口
│   └── execution.rs         # 执行接口
├── core/
│   ├── engine.rs           # 原始引擎
│   ├── engine_v2.rs        # v2引擎（基于接口）
│   ├── strategy_pool.rs     # 策略池
│   └── state.rs             # 状态管理
├── order/
│   ├── gateway.rs          # 交易所网关trait
│   └── order.rs             # 订单执行器
├── channel/
│   └── mode_switcher.rs    # 模式切换
├── strategy/
│   ├── strategy.rs          # 策略定义
│   └── signal.rs           # 信号处理
├── types.rs                 # 共享类型
└── lib.rs
```

#### 公开接口

**核心引擎 (engine_v2)**
```rust
pub struct TradingEngine<M, S, R, G> { ... }
where
    M: MarketDataProvider,
    S: StrategyExecutor,
    R: RiskChecker,
    G: ExchangeGateway,

impl<M, S, R, G> TradingEngine<M, S, R, G> {
    pub fn new(
        market_data: Arc<M>,
        strategy_executor: Arc<S>,
        risk_checker: Arc<R>,
        gateway: Arc<G>,
        symbol: String,
        initial_balance: Decimal,
        mode: TradingMode,
    ) -> Self;

    // 生命周期
    pub async fn start(&mut self) -> Result<(), EngineError>;
    pub fn stop(&mut self);
    pub fn pause(&mut self);
    pub fn resume(&mut self);

    // 主循环
    pub async fn run_loop(&mut self) -> Result<(), EngineError>;

    // 状态查询
    pub fn get_state(&self) -> EngineState;
    pub fn is_running(&self) -> bool;
    pub fn get_symbol_state(&self, symbol: &str) -> Option<SymbolState>;
}
```

**引擎状态**
```rust
pub enum EngineState {
    Initialized,
    Running,
    Paused,
    Stopped,
}

pub enum TradingMode {
    Live,     // 实盘交易
    Backtest, // 回测模式
    Replay,   // 回放模式
}
```

#### 接口层 (interfaces) - 核心设计

这是 **f_engine** 最重要的模块，定义了所有跨模块交互的接口：

**市场数据接口** (interfaces::market_data)
```rust
pub trait MarketDataProvider: Send + Sync {
    fn next_tick(&self) -> impl Future<Output = Option<MarketTick>> + Send;
    fn next_completed_kline(&self) -> impl Future<Output = Option<MarketTick>> + Send;
    fn current_price(&self, symbol: &str) -> Option<Decimal>;
    fn get_klines(&self, symbol: &str, period: &str) -> impl Future<Output = Vec<MarketKLine>> + Send;
    fn symbols(&self) -> Vec<String>;
}

pub trait VolatilityDetector: Send + Sync {
    fn detect_level(&self, symbol: &str) -> Option<VolatilityInfo>;
    fn update(&self, symbol: &str, kline: &MarketKLine);
    fn history(&self, symbol: &str) -> Vec<VolatilityInfo>;
}
```

**策略接口** (interfaces::strategy)
```rust
pub trait StrategyInstance: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn symbols(&self) -> Vec<String>;
    fn is_enabled(&self) -> bool;
    fn state(&self) -> StrategyState;
    fn on_bar(&self, bar: &MarketKLine) -> Option<TradingSignal>;
    fn on_tick(&self, tick: &MarketTick) -> Option<TradingSignal>;
    fn on_volatility_change(&self, volatility: &VolatilityInfo);
    fn set_enabled(&self, enabled: bool);
}

pub trait StrategyExecutor: Send + Sync {
    fn register(&self, strategy: Arc<dyn StrategyInstance>);
    fn unregister(&self, strategy_id: &str);
    fn dispatch(&self, bar: &MarketKLine) -> Vec<TradingSignal>;
    fn get_signal(&self, symbol: &str) -> Option<TradingSignal>;
    fn get_all_states(&self) -> Vec<StrategyState>;
}
```

**风控接口** (interfaces::risk)
```rust
pub trait RiskChecker: Send + Sync {
    fn pre_check(&self, order: &OrderRequest, account: &AccountInfo) -> RiskCheckResult;
    fn post_check(&self, order: &ExecutedOrder, account: &AccountInfo) -> RiskCheckResult;
    fn scan(&self, positions: &[PositionInfo], account: &AccountInfo) -> Vec<RiskWarning>;
    fn thresholds(&self) -> RiskThresholds;
}
```

**执行接口** (interfaces::execution)
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

#### 依赖关系
```
f_engine
    ├── a_common (错误类型)
    ├── b_data_source (市场数据)
    ├── c_data_process (指标)
    ├── d_checktable (检查表)
    └── e_risk_monitor (风控)
```

---

### 2.7 g_test - 测试层

#### 模块职责
集中管理所有 crate 的功能测试。

#### 目录结构
```
crates/g_test/src/
├── b_data_source/   # b_data_source 相关测试
└── strategy/       # 策略层黑盒测试
```

---

### 2.8 h_sandbox - 沙盒层

#### 模块职责
实验性代码、Mock 实现。

---

## 3. 模块间调用关系

### 3.1 数据流向

```
市场数据 (WebSocket/REST)
         │
         ▼
┌─────────────────┐
│  b_data_source  │ <- K线合成、Tick生成
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  c_data_process  │ <- 指标计算、信号生成
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  d_checktable   │ <- 策略检查（异步并发）
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  e_risk_monitor │ <- 风控预检、保证金检查
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│    f_engine     │ <- 订单执行、状态更新
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│   交易所网关    │ <- Mock 或 Real
└─────────────────┘
```

### 3.2 接口调用示例

**f_engine -> b_data_source**
```rust
// 通过 MarketDataProvider 接口
let tick = self.market_data.next_tick().await;

// 不直接调用：
let tick = b_data_source::ws::some_function() // 禁止
```

**f_engine -> c_data_process**
```rust
// 通过 StrategyExecutor 接口
let signals = self.strategy_executor.dispatch(&kline);

// 不直接调用：
let signals = c_data_process::min::process() // 禁止
```

**f_engine -> e_risk_monitor**
```rust
// 通过 RiskChecker 接口
let result = self.risk_checker.pre_check(&order, &account);

// 不直接调用：
let result = e_risk_monitor::risk::pre_check() // 禁止
```

**f_engine -> 交易所**
```rust
// 通过 ExchangeGateway 接口
let order_result = self.gateway.place_order(order)?;

// 不直接调用：
let result = some_exchange::place_order() // 禁止
```

### 3.3 禁止直接访问的位置

| 调用方 | 被调用模块 | 禁止操作 |
|--------|-----------|---------|
| f_engine | b_data_source | 直接访问 ws::kline_1m::KLineSynthesizer |
| f_engine | c_data_process | 直接访问 min::MinStrategy |
| f_engine | e_risk_monitor | 直接访问 risk::common::RiskPreChecker |
| 任何模块 | f_engine | 直接访问 core::engine::TradingEngine 内部状态 |

---

## 4. 安全与封装总结

### 4.1 数据封装

| 模块 | 数据封装 | 访问方式 |
|------|---------|---------|
| a_common | 全部公开 | 自由访问（基础设施） |
| b_data_source | K线/Tick公开 | 通过 DataFeeder trait |
| c_data_process | Signal/Decision公开 | 通过 SignalProcessor |
| d_checktable | CheckEntry公开 | 通过 CheckTable |
| e_risk_monitor | AccountPool 写锁保护 | 通过 AccountPool 方法 |
| f_engine | 内部状态 RwLock 保护 | 通过公开的 TradingEngine 方法 |

### 4.2 接口强制

```rust
// 所有跨模块调用必须通过 trait 接口
pub struct TradingEngine<M, S, R, G>
where
    M: MarketDataProvider,      // 接口约束
    S: StrategyExecutor,        // 接口约束
    R: RiskChecker,             // 接口约束
    G: ExchangeGateway,         // 接口约束
{
    // 私有字段，外部无法直接访问
    market_data: Arc<M>,
    strategy_executor: Arc<S>,
    risk_checker: Arc<R>,
    gateway: Arc<G>,
}
```

### 4.3 Rust 安全特性

| 特性 | 使用位置 |
|------|---------|
| #![forbid(unsafe_code)] | 所有 lib.rs 顶部 |
| Send + Sync | 所有 Trait 接口 |
| parking_lot::RwLock | AccountPool、SymbolState |
| Arc<> | 跨线程共享接口实现 |
| Result<,> 错误处理 | 所有公开接口 |

### 4.4 高内聚低耦合

```
高内聚：
- a_common: 只做基础设施
- b_data_source: 只做数据处理
- c_data_process: 只做指标计算
- d_checktable: 只做检查
- e_risk_monitor: 只做风控
- f_engine: 只做调度

低耦合：
- 层间通过 trait 接口通信
- 无直接依赖引用
- 可独立测试每个模块
```

---

## 5. 架构原则回顾

```
┌─────────────────────────────────────────────────────────────────┐
│                     核心架构原则                                   │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  1. 接口强制规范                                                 │
│     └── 所有跨模块调用必须通过 Trait 接口                          │
│                                                                  │
│  2. 模块隔离                                                     │
│     └── 禁止直接访问其他模块内部数据                              │
│                                                                  │
│  3. 依赖注入                                                     │
│     └── 核心组件通过构造函数注入                                  │
│                                                                  │
│  4. 高频路径无锁                                                 │
│     └── Tick接收、指标更新、策略判断无锁                          │
│                                                                  │
│  5. 增量计算 O(1)                                                │
│     └── EMA、RSI、K线合成都是增量更新                            │
│                                                                  │
│  6. 熔断保护                                                     │
│     └── AccountPool 实现三级熔断机制                              │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

---

## 6. 技术栈汇总

| 组件 | 技术 | 用途 |
|------|------|------|
| Runtime | Tokio | 异步 IO，多线程任务调度 |
| 状态管理 | FnvHashMap | O(1) 查找 |
| 同步原语 | parking_lot | 比 std RwLock 更高效 |
| 数值计算 | rust_decimal | 金融计算避免浮点精度问题 |
| 时间处理 | chrono | DateTime<Utc> |
| 错误处理 | thiserror | 清晰的错误类型层次 |
| 日志 | tracing | 结构化日志 info!/warn!/error! |
| 序列化 | serde | Serialize/Deserialize |

---

## 7. 编译与测试

### 7.1 编译命令
```bash
# 设置 Rust 编译器
set RUSTC=C:\Users\char\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin\rustc.exe

# 检查所有 crate
cargo check --all

# 测试所有 crate
cargo test --all
```

### 7.2 模块测试状态

| 测试包 | 状态 | 说明 |
|--------|------|------|
| g_test | 通过 | 72 个集成测试 |
| 所有 crate | 通过 | 少量警告，无错误 |
