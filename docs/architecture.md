# 交易系统架构文档

> **最后更新**: 2026-03-24
> 
> **核心原则**: Tick 输入，品种触发，策略解耦，接口统一

---

## 一、最终架构

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
│  │           async fn next_tick() → Option<Tick>            │         │
│  └──────────────────────────────────────────────────────────┘         │
└─────────────────────────────────┬───────────────────────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                         品种池 TraderPool                               │
│                                                                          │
│  • register(symbol) / unregister(symbol)                               │
│  • 只处理激活品种的 Tick，非激活品种直接丢弃                           │
│  • 引擎通过 TraderPool 过滤 Tick                                      │
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
│  │    async fn on_bar(bar: &KLine) → Option<TradingSignal>         │   │
│  │                                                                  │   │
│  │    ┌──────────────┐  ┌──────────────┐  ┌──────────────┐        │   │
│  │    │ Strategy A   │  │ Strategy B   │  │ Strategy C   │        │   │
│  │    │ (趋势策略)   │  │ (突破策略)   │  │ (网格策略)   │        │   │
│  │    └──────────────┘  └──────────────┘  └──────────────┘        │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                  │                                      │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                    CheckTable (并发检查)                         │   │
│  │  • 多策略并发执行时共享资源竞争检测                             │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                  │                                      │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                    RiskPreChecker (风控预检)                      │   │
│  │  • 资金检查 / 持仓限制 / 波动率检查                              │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                  │                                      │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                    OrderExecutor (订单执行)                       │   │
│  │  • Action → Order → ExchangeGateway                              │   │
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
│                                                                          │
│  #[async_trait]                                                         │
│  pub trait ExchangeGateway: Send + Sync {                              │
│      async fn submit_order(order: Order) → OrderResult                  │
│      async fn query_position(symbol: &str) → Position                  │
│      async fn query_balance() → Balance                               │
│  }                                                                     │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 二、模块职责表

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

## 三、核心接口定义

### 3.1 策略接口 (Strategy Trait)

```rust
#[async_trait]
pub trait Strategy: Send + Sync {
    fn id(&self) -> &str;
    fn symbols(&self) -> Vec<String>;
    async fn on_bar(&self, bar: &KLine) -> Option<TradingSignal>;
    fn state(&self) -> &StrategyState;
}

pub struct TradingSignal {
    pub symbol: String,
    pub direction: Direction,      // Long / Short / Flat
    pub quantity: Decimal,          // 仓位（策略决定）
    pub price: Option<Decimal>,
    pub stop_loss: Option<Decimal>,
    pub take_profit: Option<Decimal>,
    pub signal_type: SignalType,    // Open / Add / Close
    pub timestamp: DateTime<Utc>,
}
```

**引擎职责**: 接价格 → 分发 K线 → 收集信号 → 风控 → 执行
**策略职责**: 算指标 → 定方向 → 定仓位

### 3.2 数据源接口 (MarketStream Trait)

```rust
#[async_trait]
pub trait MarketStream: Send + Sync {
    async fn next_tick(&self) -> Option<Tick>;
    fn reset(&self);
}
```

### 3.3 账户网关接口 (ExchangeGateway Trait)

```rust
#[async_trait]
pub trait ExchangeGateway: Send + Sync {
    async fn submit_order(&self, order: Order) -> OrderResult;
    async fn cancel_order(&self, order_id: &str) -> Result<()>;
    async fn query_position(&self, symbol: &str) -> Position;
    async fn query_balance(&self) -> Balance;
}
```

---

## 四、执行流程

### 4.1 Tick 处理流程

```
MarketStream.next_tick() → Tick
         │
         ▼
TraderPool.is_trading(tick.symbol)?
         │
    No → 丢弃
    Yes → 继续
         │
         ▼
TradingEngine.on_tick(tick)
         │
    ├─▶ KLineSynthesizer.update()  // 更新 K 线
    │
    ├─▶ if 1m K 线闭合:
    │         │
    │         ▼
    │    策略调度器分发到各策略
    │         │
    │         ▼
    │    Strategy::on_bar() → TradingSignal
    │         │
    │         ▼
    │    CheckTable + RiskPreChecker
    │         │
    │         ▼
    │    ExchangeGateway.submit_order()
    │
    └─▶ if 日 K 线闭合:
              │
              ▼
         趋势判断 + 日线策略
```

### 4.2 品种触发机制

```
策略决定交易品种
         │
         ▼
TraderPool.register("BTCUSDT")
         │
         ▼
MarketStream.subscribe("BTCUSDT")  // 只订阅激活品种
         │
         ▼
只接收 BTCUSDT 的 Tick
         │
         ▼
其他品种数据丢弃
```

---

## 五、数据格式

### 5.1 OHLCVT 统一格式

```csv
symbol,period,open,high,low,close,volume,timestamp
BTCUSDT,1m,50000.0,50100.0,49900.0,50050.0,12.5,2026-03-24T10:00:00Z
BTCUSDT,1d,49500.0,51000.0,49000.0,50500.0,12500.0,2026-03-24T00:00:00Z
```

### 5.2 文件组织

```
data/
├── kline_1m/{symbol}.csv
├── kline_1d/{symbol}.csv
├── indicators/{symbol}.csv
└── trades/{symbol}.csv
```

---

## 六、波动率通道（系统级属性）

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

## 七、当前系统状态

| 组件 | 状态 | 说明 |
|------|------|------|
| `MarketStream` | ✅ 已有 | WS/模拟/回放 |
| `TraderPool` | ⚠️ 待实现 | 品种触发管理 |
| `KLineSynthesizer` | ✅ 已有 | 1m/15m/1d 合成 |
| `Strategy Trait` | ⚠️ 待定义 | 需独立 crate |
| `StrategyExecutor` | ⚠️ 待实现 | 策略调度器 |
| `CheckTable` | ✅ 已有 | 并发检查 |
| `RiskPreChecker` | ✅ 已有 | 风控预检 |
| `OrderExecutor` | ✅ 已有 | 订单执行 |
| `ExchangeGateway` | ✅ 已有 | 实盘/测试/模拟 |

---

## 八、详细文档

- [交易系统架构详细设计](./docs/architecture/TRADING_ARCHITECTURE.md)
- [六层架构说明](./CLAUDE.md)
- [Redo Keys](./REDIS_KEYS.md)

---

## 版本历史

| 日期 | 版本 | 说明 |
|------|------|------|
| 2026-03-24 | v2.0 | 重构为策略解耦架构 |
| 2026-03-20 | v1.0 | 初始架构 |

```
交易所 WS/Kline1mStream/Kline1dStream/DepthStream
         ↓
    Tick 数据
```

**Tick 数据结构**：
- `symbol` - 品种，如 "BTCUSDT"
- `price` - 最新成交价
- `qty` - 成交量
- `timestamp` - 时间戳

**作用**：负责从交易所（Binance）WebSocket 接收实时行情数据，分发到指标计算层。

---

## 二、信号处理层 (c_data_process) — 指标计算和信号生成

### 2.1 分钟级指标计算 (`Indicator1m`)

接收 Tick → 合成 1m K线 → 增量计算指标：

| 指标 | 含义 |
|------|------|
| `tr_ratio_10min_1h` | 10分钟TR / 1小时ATR，比率 >1 表示波动放大 |
| `tr_ratio_60min_5h` | 1小时TR / 5小时ATR |
| `zscore_14_1m` | 14周期Z分数（均值回归信号） |
| `zscore_1h_1m` | 1小时窗口Z分数 |
| `pos_norm_60` | 60周期价格位置 0-100，>80 超买，<20 超卖 |
| `velocity` | 价格变化速度（一阶导数） |
| `acceleration` | 速度变化率（二阶导数） |
| `power` | 动量强度 |

### 2.2 日线级指标计算 (`BigCycleCalculator`)

| 指标 | 含义 |
|------|------|
| `tr_ratio_5d_20d` | 5日TR / 20日ATR |
| `tr_ratio_20d_60d` | 20日TR / 60日ATR |
| `pine_color_12_26` | MACD 颜色（快线-慢线关系） |
| `pine_color_20_50` | 中周期颜色 |
| `pine_color_100_200` | 长周期颜色 |
| `pos_norm_20` | 20日价格位置 |

---

## 三、检查层 (d_checktable) — 信号生成和决策

### 3.1 市场状态判断 (`MinMarketStatusGenerator`)

```
输入: tr_ratio_15min, tr_base_60min, zscore, price_position
         ↓
   波动率等级判断
   ├── HIGH:   tr_ratio_15min > 13%
   ├── LOW:    tr_ratio_15min < 3%
   └── NORMAL: 其他
         ↓
   市场状态判断 (优先级)
   ├── PIN:    tr_base_60min > 15% AND 插针条件 >= 2
   ├── RANGE:  volatility=LOW AND tr_ratio<1 AND |zscore|<0.5
   └── TREND:  其他
```

**PIN 插针条件**（4选2触发）：
1. `|zscore| > 2`
2. `tr_ratio_15min > 13%`
3. `price_position > 90% 或 < 10%`
4. `tr_base_60min > 20%`

### 3.2 分钟级信号生成 (`MinSignalGenerator`)

**7个插针条件**（满足 >=4 触发）：

| # | 条件 | 阈值 |
|---|------|------|
| 1 | extreme_zscore | `|zscore_14_1m| > 2` 或 `|zscore_1h_1m| > 2` |
| 2 | extreme_vol | `tr_ratio_60min_5h > 1` 或 `tr_ratio_10min_1h > 1` |
| 3 | extreme_pos | `pos_norm_60 > 80` 或 `< 20` |
| 4 | extreme_speed | `acc_percentile_1h > 90` |
| 5 | extreme_bg_color | Pine背景色 = "纯绿" 或 "纯红" |
| 6 | extreme_bar_color | Pine柱状色 = "纯绿" 或 "纯红" |
| 7 | extreme_price_deviation | `|horizontal_position| == 100` |

**信号决策逻辑**：

```
┌─────────────────────────────────────────────────────────────┐
│                    做多入场 (LongEntry)                      │
│  前置: tr_base_60min > 15%                                  │
│  价格偏离方向: < 0 (价格偏低)                                │
│  插针条件: >= 4                                             │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│                    做空入场 (ShortEntry)                     │
│  前置: tr_base_60min > 15%                                  │
│  价格偏离方向: > 0 (价格偏高)                               │
│  插针条件: >= 4                                             │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│                    做多出场 (LongExit)                       │
│  插针条件: >= 4                                             │
│  位置极端: pos_norm_60 > 80                                  │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│                    做空出场 (ShortExit)                      │
│  插针条件: >= 4                                             │
│  位置极端: pos_norm_60 < 20                                  │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│                    多头对冲 (LongHedge)                      │
│  前置: tr_base_60min < 15% (低波动)                         │
│  价格偏离方向: < 0                                          │
│  6个条件满足 >= 4                                           │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│                    空头对冲 (ShortHedge)                     │
│  前置: tr_base_60min < 15%                                  │
│  价格偏离方向: > 0                                          │
│  6个条件满足 >= 4                                           │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│                 高波动退出 (ExitHighVol)                     │
│  前置: tr_base_60min < 15%                                  │
│  3个条件满足 >= 2:                                          │
│    - cond1: tr_ratio_60min_5h < 1 AND tr_ratio_10min_1h < 1 │
│    - cond2: 20 < pos_norm_60 < 80                           │
│    - cond3: 10 < |horizontal_position| <= 90               │
└─────────────────────────────────────────────────────────────┘
```

---

## 四、风控层 (e_risk_monitor) — 硬性规则检查

### 4.1 预检 (`RiskPreChecker`)

**检查顺序**（任一失败则拒绝订单）：

```
订单价值 = qty × price
         ↓
保证金比率检查: 订单价值 <= 账户可用余额 × 0.95
         ↓
单笔限额检查: 订单价值 <= 1000 USDT
         ↓
风险敞口检查: ...（可根据配置扩展）
```

### 4.2 复检 (`RiskReChecker`)

下单后再次验证订单执行结果是否符合预期。

### 4.3 仓位管理 (`LocalPositionManager`)

管理本地持仓状态：
- 按方向（多/空）记录持仓
- 计算持仓成本、盈亏
- 处理加仓、平仓逻辑

---

## 五、引擎层 (f_engine) — 交易执行和状态管理

### 5.1 主循环 (`TradingEngine::on_tick`)

```
收到 Tick
    │
    ▼
K线增量更新 (KLineSynthesizer)
    │
    ▼
价格位置更新 (update_price_position)
    │
    ▼
┌─────────────────────────────────────────┐
│   1m K线完成？                           │
│   是 → on_minute_bar()                   │
│   否 → 仅价格位置更新                    │
└─────────────────────────────────────────┘
```

### 5.2 分钟级策略执行 (`on_minute_bar`)

```
检查 ModeSwitcher.is_trading_allowed()
    ├── 不允许 → 直接返回（可能处于 Maintenance/Paper 模式）
    │
    ▼
遍历所有交易品种 (symbol_states)
    │
    ▼
并发检查每个品种:
    │
    ├── 检查启动状态 (Fresh / Recovery)
    ├── 检查超时 (1m_timeout)
    ├── 获取信号缓存 (signal_processor.get_min_signal)
    ├── 检查信号年龄 (age > timeout_secs → 丢弃)
    │
    ▼
执行决策 (execute_decision)
    │
    ├── 风控预检 (pre_trade_check)
    ├── 转换 Action → Side (Long→Buy, Short→Sell, Flat→平仓)
    │
    ▼
调用 OrderExecutor.execute()
    │
    ▼
发送订单到 ExchangeGateway
```

### 5.3 模式切换 (`ModeSwitcher`)

| 模式 | 说明 | is_trading_allowed |
|------|------|-------------------|
| `Normal` | 正常交易 | ✅ |
| `Paper` | 仿真交易 | ✅ |
| `Backtest` | 回测模式 | ❌ |
| `Maintenance` | 维护模式 | ❌ |

### 5.4 交易锁 (`TradeLock`)

防止同一 Tick 被重复处理：
- 记录上次处理的时间戳 `timestamp`
- 新 Tick 的 `timestamp <= lock.timestamp` → 丢弃

---

## 六、执行流程图

```
┌──────────────────────────────────────────────────────────────────┐
│                        交易所 WebSocket                           │
│                     (Kline1mStream 等)                           │
└────────────────────────────┬─────────────────────────────────────┘
                             ↓
┌──────────────────────────────────────────────────────────────────┐
│                      b_data_source: Tick                          │
│                    (市场数据层)                                    │
└────────────────────────────┬─────────────────────────────────────┘
                             ↓
┌──────────────────────────────────────────────────────────────────┐
│                   c_data_process: SignalProcessor                  │
│                     (信号处理层)                                   │
│  ┌─────────────────┐         ┌─────────────────┐                 │
│  │  Indicator1m    │         │ BigCycleCalc    │                 │
│  │  (分钟级指标)   │         │  (日线级指标)    │                 │
│  └────────┬────────┘         └────────┬────────┘                 │
│           ↓                           ↓                           │
│  ┌─────────────────────────────────────────────┐                  │
│  │         d_checktable: 信号生成器             │                  │
│  │  MinMarketStatusGenerator → 市场状态(PIN/TREND/RANGE)          │
│  │  MinSignalGenerator → 交易信号(Long/Short/Hedge/Exit)         │
│  └──────────────────────────┬──────────────────┘                  │
└─────────────────────────────┼────────────────────────────────────┘
                              ↓
┌──────────────────────────────────────────────────────────────────┐
│                      e_risk_monitor                               │
│                      (风控层)                                     │
│  RiskPreChecker ──→ 订单价值 / 保证金比率 / 单笔限额             │
│  RiskReChecker  ──→ 下单后复检                                    │
│  PositionManager ──→ 持仓管理                                     │
└──────────────────────────────┬───────────────────────────────────┘
                               ↓
┌──────────────────────────────────────────────────────────────────┐
│                       f_engine                                    │
│                     (引擎执行层)                                   │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐   │
│  │ ModeSwitcher   │  │  TradeLock     │  │ SymbolState    │   │
│  │ (模式切换)      │  │  (交易锁)      │  │ (品种状态)      │   │
│  └─────────────────┘  └─────────────────┘  └─────────────────┘   │
│                              ↓                                    │
│                    OrderExecutor                                  │
│                         ↓                                        │
│                   ExchangeGateway                                 │
│                (下单到交易所)                                     │
└──────────────────────────────────────────────────────────────────┘
```

---

## 七、信号决策速查表

| 信号 | 前置条件 | 价格偏离 | 插针条件 | 位置条件 |
|------|---------|---------|---------|---------|
| **LongEntry** | tr>15% | <0 | >=4 | - |
| **ShortEntry** | tr>15% | >0 | >=4 | - |
| **LongExit** | - | - | >=4 | >80 |
| **ShortExit** | - | - | >=4 | <20 |
| **LongHedge** | tr<15% | <0 | >=4(6条件) | - |
| **ShortHedge** | tr<15% | >0 | >=4(6条件) | - |
| **ExitHighVol** | tr<15% | - | >=2(3条件) | 20-80 |

---

## 八、当前系统状态

✅ **已完成**：
- 数据层（WS 接收、K线合成）
- 指标层（EMA、RSI、TR、PineColor、ZScore）
- 信号生成（MinSignalGenerator、DaySignalGenerator）
- 风控层（预检、复检、仓位管理）
- 引擎层（Tick 主循环、订单执行、模式切换）

⚠️ **简化/缺失**（暂不关注）：
- Pipeline 编排层（已删除 Legacy）
- 完整的 `TradingEngine` 状态机（当前较简化）
- 灾备恢复、持久化服务（代码存在但未完全集成）
- `check_daily_strategies_batch` 死代码
