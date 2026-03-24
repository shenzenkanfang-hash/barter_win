# 交易系统架构详细设计

> **最后更新**: 2026-03-24
> 
> **配套文档**: [../architecture.md](../architecture.md)（总体架构）

---

## 概述

本系统采用**三层解耦 + 策略外部化**架构：
1. **数据层** - 统一价格输入
2. **引擎层** - K线合成、策略调度、风控执行
3. **账户层** - 订单执行网关

**核心设计原则**：
- Tick 输入，品种触发
- 策略是外部的，引擎只负责调度
- 接口统一，输入输出标准化

---

## 核心数据流

```
TraderPool (品种池)
       │
       ▼
MarketStream.next_tick()
       │
       ▼
TradingEngine.on_tick()
       │
       ├─▶ 分钟 K 线合成 (1m)
       │         │
       │         ▼
       │         指标计算 → 信号生成
       │
       └─▶ 日线 K 线合成 (1d) → 趋势判断
                   │
                   ▼
            CheckTable (并发检查)
                   │
                   ▼
            RiskPreChecker (风控预检)
                   │
                   ▼
            ExchangeGateway.submit_order()
```

---

## 一、数据层 (DataSource)

### 1.1 统一价格格式 OHLCVT

```rust
pub struct OHLCVT {
    pub symbol: String,      // 交易对
    pub period: Period,      // 周期
    pub open: Decimal,       // 开盘价
    pub high: Decimal,       // 最高价
    pub low: Decimal,        // 最低价
    pub close: Decimal,      // 收盘价
    pub volume: Decimal,     // 成交量
    pub timestamp: DateTime<Utc>,  // 时间戳
}

pub enum Period {
    Minute(u8),  // M1, M5, M15, M30
    Hour(u8),    // H1, H4, H6, H12
    Day,         // 日线
}
```

### 1.2 数据源接口

```rust
#[async_trait]
pub trait DataSource: Send + Sync {
    /// 获取下一条 Tick
    async fn next_tick(&self) -> Option<Tick>;

    /// 重置（用于回放重新开始）
    fn reset(&self);

    /// 获取数据源类型
    fn source_type(&self) -> DataSourceType;
}

pub enum DataSourceType {
    RealWs,     // 真实 WebSocket
    Mock,       // 模拟数据
    Replay,     // 历史回放
}
```

### 1.3 实现类型

| 类型 | 说明 |
|------|------|
| `BinanceWsSource` | 连接 Binance 真实 WebSocket，接收实时期价 |
| `MockSource` | 生成随机价格 Tick，用于测试 |
| `ReplaySource` | 从 CSV 文件回放历史数据 |

---

## 二、品种池 (TraderPool)

### 2.1 设计目的

- **只交易激活的品种**：不是全市场扫描，而是只处理 TraderPool 中注册的品牌
- **按需订阅**：MarketStream 只订阅 TraderPool 中的品种 Tick
- **动态管理**：支持运行时添加/移除交易品种

### 2.2 接口设计

```rust
pub struct TraderPool {
    /// 激活的交易品种
    trading_symbols: RwLock<HashSet<String>>,
    /// 品种元数据
    symbol_meta: RwLock<HashMap<String, SymbolMeta>>,
}

impl TraderPool {
    /// 注册交易品种
    pub fn register(&self, symbol: &str, meta: SymbolMeta);

    /// 注销交易品种
    pub fn unregister(&self, symbol: &str);

    /// 获取所有激活品种
    pub fn get_trading_symbols(&self) -> HashSet<String>;

    /// 检查品种是否激活
    pub fn is_trading(&self, symbol: &str) -> bool;

    /// 更新品种状态
    pub fn update_status(&self, symbol: &str, status: TradingStatus);
}
```

### 2.3 品种状态

```rust
pub enum TradingStatus {
    Pending,     // 待激活
    Active,      // 正常交易
    Paused,      // 暂停
    Closed,      // 已平仓
}
```

---

## 三、策略接口 (Strategy Trait)

### 3.1 核心原则

**引擎不管具体策略逻辑，只负责：**
- 接收价格数据
- 分发给策略
- 收集交易信号
- 执行风控
- 执行订单

**策略负责：**
- 计算指标
- 生成交易信号
- 决定仓位数量

### 3.2 策略接口

```rust
/// 策略接口 - 引擎与策略的唯一交互方式
#[async_trait]
pub trait Strategy: Send + Sync {
    /// 策略 ID
    fn id(&self) -> &str;

    /// 关注的品种列表
    fn symbols(&self) -> Vec<String>;

    /// 处理 K 线，生成交易信号
    ///
    /// 输入：K 线数据
    /// 输出：交易信号（包含方向和仓位）
    async fn on_bar(&self, bar: &KLine) -> Option<TradingSignal>;

    /// 获取策略当前状态
    fn state(&self) -> &StrategyState;
}

/// 交易信号 - 策略输出
pub struct TradingSignal {
    pub symbol: String,           // 品种
    pub direction: Direction,    // 方向：Long / Short / Flat
    pub quantity: Decimal,       // 仓位数量（策略决定）
    pub price: Option<Decimal>,   // 目标价格（市价单为 None）
    pub stop_loss: Option<Decimal>, // 止损价
    pub take_profit: Option<Decimal>, // 止盈价
    pub signal_type: SignalType, // 信号类型
    pub timestamp: DateTime<Utc>,
}

/// 方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Long,   // 做多
    Short,  // 做空
    Flat,   // 平仓
}

/// 信号类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalType {
    Open,   // 开仓
    Add,    // 加仓
    Close,  // 平仓
}
```

### 3.3 策略实现示例

```rust
/// 示例：简单突破策略
pub struct BreakoutStrategy {
    id: String,
    symbols: Vec<String>,
    lookback: usize,
}

#[async_trait]
impl Strategy for BreakoutStrategy {
    fn id(&self) -> &str { &self.id }
    fn symbols(&self) -> Vec<String> { self.symbols.clone() }

    async fn on_bar(&self, bar: &KLine) -> Option<TradingSignal> {
        // 1. 计算指标（省略）
        let high = self.calc_high(bar)?;
        let low = self.calc_low(bar)?;

        // 2. 生成信号
        if bar.close > high {
            Some(TradingSignal {
                symbol: bar.symbol.clone(),
                direction: Direction::Long,
                quantity: self.calc_quantity(bar)?,  // 策略决定仓位
                price: None,
                stop_loss: Some(low),
                take_profit: None,
                signal_type: SignalType::Open,
                timestamp: bar.timestamp,
            })
        } else if bar.close < low {
            Some(TradingSignal {
                symbol: bar.symbol.clone(),
                direction: Direction::Short,
                quantity: self.calc_quantity(bar)?,
                price: None,
                stop_loss: Some(high),
                take_profit: None,
                signal_type: SignalType::Open,
                timestamp: bar.timestamp,
            })
        } else {
            None
        }
    }
}
```

### 3.4 引擎数据流

```
                    ┌─────────────────┐
                    │   价格数据      │
                    │  (Tick/KLine)  │
                    └────────┬────────┘
                             │
                             ▼
                    ┌─────────────────┐
                    │   品种过滤       │
                    │ (TraderPool)   │
                    └────────┬────────┘
                             │
         ┌───────────────────┼───────────────────┐
         │                   │                   │
         ▼                   ▼                   ▼
┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐
│    Strategy A   │ │    Strategy B   │ │    Strategy C   │
│   (趋势策略)    │ │   (突破策略)     │ │   (网格策略)    │
└────────┬────────┘ └────────┬────────┘ └────────┬────────┘
         │                   │                   │
         ▼                   ▼                   ▼
┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐
│ TradingSignal A │ │ TradingSignal B │ │ TradingSignal C  │
│ (仓位: 0.5 BTC) │ │ (仓位: 0.3 ETH) │ │ (仓位: 1.0 BNB) │
└────────┬────────┘ └────────┬────────┘ └────────┬────────┘
         │                   │                   │
         └───────────────────┼───────────────────┘
                             │
                             ▼
                    ┌─────────────────┐
                    │   CheckTable    │
                    │   (并发检查)    │
                    └────────┬────────┘
                             │
                             ▼
                    ┌─────────────────┐
                    │  RiskPreChecker │
                    │   (风控预检)    │
                    └────────┬────────┘
                             │
                             ▼
                    ┌─────────────────┐
                    │  ExchangeGateway │
                    │   (订单执行)    │
                    └─────────────────┘
```
```

---

## 四、账户层 (ExchangeGateway)

### 4.1 接口设计

```rust
#[async_trait]
pub trait ExchangeGateway: Send + Sync {
    /// 下单
    async fn submit_order(&self, order: Order) -> OrderResult;

    /// 撤单
    async fn cancel_order(&self, order_id: &str) -> Result<()>;

    /// 查询持仓
    async fn query_position(&self, symbol: &str) -> Position;

    /// 查询账户余额
    async fn query_balance(&self) -> Balance;

    /// 同步持仓（用于灾备恢复）
    async fn sync_positions(&self) -> Result<Vec<Position>>;
}
```

### 4.2 实现类型

| 类型 | 说明 |
|------|------|
| `BinanceRealGateway` | 实盘账户，连接 Binance 现货/期货 |
| `BinanceTestnetGateway` | 测试网账户，连接 Binance Testnet |
| `MockGateway` | 模拟账户，所有操作在内存中执行 |

### 4.3 MockGateway 功能

```rust
pub struct MockGateway {
    /// 账户余额
    balance: RwLock<HashMap<String, Decimal>>,
    /// 持仓
    positions: RwLock<HashMap<String, Position>>,
    /// 订单簿
    orders: RwLock<HashMap<String, Order>>,
    /// 成交模拟配置
    config: MockConfig,
}

pub struct MockConfig {
    /// 是否模拟成交
    simulate_fill: bool,
    /// 成交延迟（毫秒）
    fill_delay_ms: u64,
    /// 滑点率
    slippage_rate: Decimal,
}
```

---

## 五、执行流程

### 5.1 Tick 处理流程

```
1. MarketStream.next_tick() → Tick
       │
       ▼
2. TraderPool.is_trading(tick.symbol)?
       │
       │ No → 丢弃
       │ Yes → 继续
       ▼
3. TradingEngine.on_tick(tick)
       │
       ├─▶ KLineSynthesizer.update(tick)  // 更新 K 线
       │
       ├─▶ if 1m K 线闭合:
       │         │
       │         ▼
       │    on_minute_bar() → 指标计算 → 信号生成
       │
       └─▶ if 日 K 线闭合:
                 │
                 ▼
            on_day_bar() → 趋势判断
```

### 5.2 日线级 vs 分钟级

| 级别 | 运行周期 | 职责 |
|------|----------|------|
| **日线级** | 最低 | 趋势判断、止损设置 |
| **分钟级** | 1m/15m | 信号生成、入场点 |

**注意**：日线是最大周期，但系统运行是分钟级触发的。

### 5.3 品种触发机制

```
策略决定交易品种:
    │
    ▼
TraderPool.register("BTCUSDT")
    │
    ▼
MarketStream.subscribe("BTCUSDT")
    │
    ▼
只接收 BTCUSDT 的 Tick
    │
    ▼
其他品种数据丢弃
```

---

## 六、数据保存格式

### 6.1 OHLCVT CSV 格式

```csv
symbol,period,open,high,low,close,volume,timestamp
BTCUSDT,1m,50000.0,50100.0,49900.0,50050.0,12.5,2026-03-24T10:00:00Z
BTCUSDT,1m,50050.0,50200.0,50000.0,50100.0,15.3,2026-03-24T10:01:00Z
BTCUSDT,1d,49500.0,51000.0,49000.0,50500.0,12500.0,2026-03-24T00:00:00Z
```

### 6.2 文件组织

```
data/
├── kline_1m/
│   └── {symbol}.csv
├── kline_1d/
│   └── {symbol}.csv
├── indicators/
│   └── {symbol}.csv
└── trades/
    └── {symbol}.csv
```

---

## 七、最终架构图

```
┌─────────────────────────────────────────────────────────────────────────┐
│                            main.rs                                      │
│                       (程序入口/组件组装)                               │
└─────────────────────────────────┬───────────────────────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                         数据层 b_data_source                             │
│                                                                          │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐                 │
│  │ BinanceWs    │  │ MockSource   │  │ ReplaySource │                 │
│  │ (实时WS)     │  │ (模拟数据)   │  │ (历史回放)   │                 │
│  └──────┬───────┘  └──────────────┘  └──────────────┘                 │
│         │                                                           │
│         ▼                                                           │
│  ┌──────────────────────────────────────────────────────────┐         │
│  │           MarketStream (统一 Tick 接口)                   │         │
│  └─────────────────────────────────┬────────────────────────┘         │
└───────────────────────────────────┼─────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                         品种池 TraderPool                               │
│                                                                          │
│  • register(symbol) / unregister(symbol)                               │
│  • 只处理激活品种的 Tick                                               │
└─────────────────────────────────┬───────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                    引擎层 f_engine (核心控制器)                         │
│                                                                          │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                    K 线合成器                                     │   │
│  │  Tick → 1m K 线 → 15m K 线 → 1d K 线                          │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                  │                                      │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                    策略调度器 (StrategyExecutor)                  │   │
│  │                                                                  │   │
│  │    策略是外部的！引擎只调用:                                     │   │
│  │    async fn on_bar(bar: &KLine) → Option<TradingSignal>        │   │
│  │                                                                  │   │
│  │    ┌──────────────┐  ┌──────────────┐  ┌──────────────┐        │   │
│  │    │ Strategy A   │  │ Strategy B   │  │ Strategy C   │        │   │
│  │    │ (趋势策略)   │  │ (突破策略)   │  │ (网格策略)   │        │   │
│  │    └──────────────┘  └──────────────┘  └──────────────┘        │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                  │                                      │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                    CheckTable (并发检查)                         │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                  │                                      │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                    RiskPreChecker (风控预检)                      │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                  │                                      │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                    OrderExecutor (订单执行)                       │   │
│  └─────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────┬───────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                        账户层 a_common                                 │
│                                                                          │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐                 │
│  │ RealGateway  │  │ TestGateway  │  │ MockGateway  │                 │
│  │ (实盘)       │  │ (测试网)     │  │ (模拟账户)   │                 │
│  └──────────────┘  └──────────────┘  └──────────────┘                 │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 八、模块职责表

| 模块 | crate | 职责 |
|------|-------|------|
| **数据源** | `b_data_source` | 统一价格输入（WS/模拟/回放） |
| **品种池** | `b_data_source` | 管理激活品种，过滤 Tick |
| **K线合成** | `c_data_process` | Tick → 1m/15m/1d K线 |
| **策略接口** | `f_engine` | 定义 `Strategy::on_bar()` trait |
| **策略调度** | `f_engine` | 分发 K线，收集信号，策略与引擎解耦 |
| **并发检查** | `d_checktable` | 多策略资源竞争检测 |
| **风控预检** | `e_risk_monitor` | 资金/持仓/波动率检查 |
| **订单执行** | `f_engine` | 订单转换和发送 |
| **账户网关** | `a_common` | 订单执行（实盘/测试/模拟） |

---

## 九、波动率通道（系统级属性）

系统内置波动率检测，自动分流执行参数：

```
                    ┌──────────────────┐
                    │   日线判断        │
                    │  波动率级别       │
                    └────────┬─────────┘
                             │
              ┌──────────────┴──────────────┐
              │                             │
              ▼                             ▼
     ┌────────────────┐            ┌────────────────┐
     │  高波动通道     │            │  低波动通道     │
     │  间隔周期: 短   │            │  间隔周期: 长   │
     │  追涨杀跌      │            │  高抛低吸       │
     └────────┬───────┘            └────────┬───────┘
              │                             │
              └──────────────┬──────────────┘
                             │
                    ┌────────▼─────────┐
                    │   统一交易执行   │
                    │  (同一套逻辑)    │
                    └─────────────────┘
```

**通道由系统决定，策略只负责响应价格信号**。

---

## 十、待完成

| 组件 | 状态 | 说明 |
|------|------|------|
| `OHLCVT` 数据结构 | ✅ 已有 | `KLine` 即 OHLCVT |
| `MarketStream` trait | ✅ 已有 | 需确认统一性 |
| `TraderPool` | ⚠️ 待实现 | 品种触发管理 |
| `BinanceWsSource` | ✅ 已有 | Binance WS 连接 |
| `MockSource` | ⚠️ 待增强 | 完善模拟价格生成 |
| `ReplaySource` | ❌ 待实现 | CSV 回放功能 |
| `ExchangeGateway` | ✅ 已有 | 需确认接口完整性 |
| `MockGateway` | ⚠️ 待增强 | 完善模拟账户 |

---

## 版本

| 日期 | 版本 | 说明 |
|------|------|------|
| 2026-03-24 | v2.0 | 策略外部化解耦架构 |
