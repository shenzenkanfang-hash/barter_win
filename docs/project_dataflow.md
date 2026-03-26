# 项目全层数据流说明书

## 一、整体分层架构

```
┌─────────────────────────────────────────────────────────────────┐
│                     1. 数据源层 (b_data_source)                   │
│   Binance API ──→ WebSocket 实时行情                             │
└─────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│                     2. DataFeeder 数据层                          │
│   latest_ticks: HashMap<Symbol, Tick>                           │
│   Tick { kline_1m, kline_15m, kline_1d }                     │
└─────────────────────────────────────────────────────────────────┘
                              ↓ ws_get_1m() 按需拉取
┌─────────────────────────────────────────────────────────────────┐
│                     3. 引擎层 (f_engine)                        │
│   TradingEngineV2.process_tick()                               │
└─────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│                     4. 策略层 (c_data_process)                    │
│   StrategyQuery → 策略执行 → TradingDecision                     │
└─────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│                     5. 风控层 (e_risk_monitor)                   │
│   RiskChecker.pre_check() → RiskCheckResult                   │
└─────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│                     6. 网关层 (f_engine/order)                   │
│   ExchangeGateway.place_order() → OrderResult                   │
└─────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│                     7. 账户/持仓层 (simulator/account)           │
│   Account.apply_open() / apply_close()                          │
└─────────────────────────────────────────────────────────────────┘
```

---

## 二、逐层数据流详解

---

### 【1. 数据源层】b_data_source

**职责：**
- 封装 Binance API 和 WebSocket 连接
- 提供统一的数据获取接口

**需要数据：**
- 历史K线（API拉取）
- 实时K线（WS订阅）

**获取方式：**

| 方式 | 接口 | 说明 |
|-----|------|------|
| REST API | `BinanceApiGateway::fetch_klines()` | 拉取历史K线 |
| WebSocket | `Kline1mStream::new()` | 订阅实时1m K线 |

**数据格式：**

```rust
// API K线格式 (Vec<serde_json::Value>)
[[open_time, "50000", "50010", "49990", "50020", "100", close_time, ...], ...]

// WS K线消息 (BinanceKlineMsg)
{
    "e": "kline",
    "s": "BTCUSDT",
    "k": {
        "t": 1672515780000,    // kline start time
        "T": 1672515839999,    // kline close time
        "s": "BTCUSDT",
        "i": "1m",
        "o": "50000",
        "c": "50020",
        "h": "50050",
        "l": "49990",
        "v": "100",
        "x": true             // is_closed
    }
}
```

**数据转换：**
- API raw → 解析 → `KLine` 结构体
- WS JSON → 解析 → `BinanceKlineMsg` → `KLine`

**输出给：**
- `DataFeeder.latest_ticks`
- `VolatilityManager`

---

### 【2. DataFeeder 数据层】b_data_source/api

**职责：**
- 统一市场数据存储和查询接口
- 共享内存，供其他模块按需拉取

**需要数据：**
- Tick（含K线）
- 账户/持仓（通过网关）

**获取方式：**
- WS数据 → `push_tick()` 存入
- API数据 → `fetch_klines()` 拉取

**数据格式：**

```rust
pub struct Tick {
    pub symbol: String,
    pub price: Decimal,
    pub qty: Decimal,
    pub timestamp: DateTime<Utc>,
    pub kline_1m: Option<KLine>,   // 当前1m K线
    pub kline_15m: Option<KLine>,  // 15m K线
    pub kline_1d: Option<KLine>,    // 日K线
}

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
```

**拉取接口：**

| 接口 | 说明 |
|-----|------|
| `ws_get_1m(symbol)` | 获取最新1m K线 |
| `ws_get_15m(symbol)` | 获取最新15m K线 |
| `ws_get_1d(symbol)` | 获取最新日K线 |

**输出给：**
- `TradingEngineV2.process_tick()`
- 策略层（指标计算）

---

### 【3. 引擎层】f_engine/core/engine_v2.rs

**职责：**
- 接收 Tick 数据
- 协调策略、风控、订单执行
- 管理引擎状态

**需要数据：**

| 数据 | 获取方式 |
|-----|---------|
| Tick/K线 | `ws_get_1m()` 拉取 |
| 账户信息 | `gateway.get_account()` |
| 持仓信息 | `gateway.get_position()` |
| 风控配置 | `RiskChecker` trait |

**入口方法：**

```rust
pub fn process_tick(
    &self,
    symbol: &str,
    price: Decimal,
    volatility: Decimal,
    current_position_qty: Decimal,
    current_position_price: Decimal,
) -> Result<Option<OrderInfo>, TradingError>
```

**V1.4 执行流程：**

```
process_tick()
    ↓
1. 触发器检查 (trigger_manager)
    ↓
2. StrategyQuery + 策略执行
    ↓
3. 风控预检 (RiskChecker.pre_check)
    ↓
4. 品种级抢锁
    ↓
5. 风控精校 (RiskManager.lock_check)
    ↓
6. 冻结资金 + 下单 (gateway.place_order)
    ↓
7. 状态对齐
```

**数据输出：**

```rust
// 订单信息
pub struct OrderInfo {
    pub order_id: String,
    pub symbol: String,
    pub side: OrderSide,
    pub qty: Decimal,
    pub price: Decimal,
    pub status: OrderStatus,
}
```

**输出给：**
- 策略层（StrategyQuery 构建）
- 风控层（RiskChecker）
- 网关层（ExchangeGateway）

---

### 【4. 策略层】c_data_process

**职责：**
- 计算指标（EMA/RSI/PineColor）
- 生成交易信号
- 做出交易决策

**需要数据：**

| 数据 | 获取方式 |
|-----|---------|
| 历史K线 | `DataFeeder.ws_get_1m()` |
| 账户余额 | `ExchangeAccount` |

**核心结构：**

```rust
// 策略查询
pub struct StrategyQuery {
    pub symbol: String,
    pub available_fund: Decimal,
    pub volatility: Decimal,
    pub current_position_qty: Decimal,
    pub current_position_price: Decimal,
    // ... 指标数据
}

// 交易决策
pub enum TradingDecision {
    Buy(SignalInfo),
    Sell(SignalInfo),
    Hold,
}
```

**指标计算：**

| 指标 | 说明 |
|-----|------|
| EMA | 指数移动平均线 |
| RSI | 相对强弱指数 |
| PineColor | 趋势颜色信号 |
| PricePosition | 价格位置 |

**输出给：**
- 引擎层（TradingDecision）
- 风控层（作为 RiskCheckResult 依据）

---

### 【5. 风控层】e_risk_monitor / f_engine/interfaces

**职责：**
- 下单前风控检查
- 下单后风控检查
- 定期风险扫描

**接口：**

```rust
pub trait RiskChecker: Send + Sync {
    fn pre_check(&self, order: &OrderRequest, account: &ExchangeAccount) -> RiskCheckResult;
    fn post_check(&self, order: &ExecutedOrder, account: &ExchangeAccount) -> RiskCheckResult;
    fn scan(&self, positions: &[PositionInfo], account: &ExchangeAccount) -> Vec<RiskWarning>;
    fn thresholds(&self) -> RiskThresholds;
}
```

**需要数据：**

| 数据 | 获取方式 |
|-----|---------|
| 订单请求 | 引擎传入 |
| 账户余额 | `ExchangeAccount` |
| 持仓信息 | `PositionInfo` |

**风控检查项：**

| 检查项 | 说明 |
|-----|------|
| 余额检查 | 可用余额 >= 保证金 |
| 持仓限制 | 总持仓 < 限额 |
| 价格偏离 | 下单价格偏离市价 |
| 订单频率 | 下单间隔限制 |

**输出给：**
- 引擎层（RiskCheckResult）
- 订单执行（RejectReason）

---

### 【6. 网关层】f_engine/order/gateway.rs

**职责：**
- 统一订单执行接口
- 账户/持仓查询
- 屏蔽交易所差异

**接口：**

```rust
pub trait ExchangeGateway: Send + Sync {
    fn place_order(&self, req: OrderRequest) -> Result<OrderResult, EngineError>;
    fn get_account(&self) -> Result<ExchangeAccount, EngineError>;
    fn get_position(&self, symbol: &str) -> Result<Option<ExchangePosition>, EngineError>;
}
```

**实现类：**

| 实现 | 位置 | 用途 |
|-----|------|------|
| `MockBinanceGateway` | f_engine/src/order/ | 本地测试 |
| `ShadowBinanceGateway` | h_sandbox/src/gateway/ | 沙盒模拟 |
| 真实 Binance | a_common 或 b_data_source | 生产环境 |

**ShadowBinanceGateway（沙盒）：**

```rust
// 账户模拟
pub struct Account {
    initial_balance: Decimal,
    available: Decimal,
    frozen_margin: Decimal,
    positions: FnvHashMap<String, Position>,
}

// 持仓模拟
pub struct Position {
    symbol: String,
    long_qty: Decimal,
    long_entry_price: Decimal,
    short_qty: Decimal,
    short_entry_price: Decimal,
}
```

**数据格式：**

```rust
pub struct ExchangeAccount {
    pub account_id: String,
    pub total_equity: Decimal,
    pub available: Decimal,
    pub frozen_margin: Decimal,
    pub unrealized_pnl: Decimal,
    pub update_ts: i64,
}

pub struct ExchangePosition {
    pub symbol: String,
    pub long_qty: Decimal,
    pub long_avg_price: Decimal,
    pub short_qty: Decimal,
    pub short_avg_price: Decimal,
    pub unrealized_pnl: Decimal,
    pub margin_used: Decimal,
}

pub struct OrderResult {
    pub order_id: String,
    pub status: OrderStatus,
    pub filled_qty: Decimal,
    pub filled_price: Decimal,
    pub commission: Decimal,
    pub reject_reason: Option<RejectReason>,
}
```

**输出给：**
- 引擎层（订单结果）
- 账户层（状态更新）

---

### 【7. 账户/持仓层】h_sandbox/simulator

**职责：**
- 管理账户余额
- 管理持仓
- 计算盈亏

**核心方法：**

```rust
// 开仓
pub fn apply_open(&mut self, symbol: &str, side: Side, qty: Decimal, price: Decimal, leverage: Decimal);

// 平仓
pub fn apply_close(&mut self, symbol: &str, side: Side, qty: Decimal, price: Decimal) -> Decimal;

// 扣除手续费
pub fn deduct_fee(&mut self, fee: Decimal);
```

**需要数据：**

| 数据 | 获取方式 |
|-----|---------|
| 价格 | WS实时价格 |
| 订单 | 网关成交回报 |

**输出给：**
- 网关层（get_account / get_position）

---

### 【8. 沙盒层】sandbox_main / h_sandbox

**职责：**
- 模拟WS推送（StreamTickGenerator）
- 模拟API请求（ShadowBinanceGateway）

**两端拦截：**

```
┌─────────────────────────────────────────────────┐
│              正常业务层                           │
│   TradingEngineV2 → 策略 → 风控 → 订单          │
└─────────────────────────────────────────────────┘
         ↑ 上行请求                    ↓ 下行响应
┌─────────────────────────────────────────────────┐
│         ShadowBinanceGateway                   │
│   place_order() / get_account() / get_position │
└─────────────────────────────────────────────────┘
         ↑                                ↓
┌─────────────────────────────────────────────────┐
│         StreamTickGenerator                     │
│   → push_tick() → DataFeeder                  │
└─────────────────────────────────────────────────┘
```

**数据格式转换：**

```
API K线 → StreamTickGenerator → 60 ticks/K线
                                   ↓
                            Tick {
                                kline_1m: Some(KLine {...}),
                                ...
                            }
                                   ↓
                            DataFeeder.push_tick()
```

**运行参数：**

| 参数 | 说明 | 示例 |
|-----|------|------|
| `--symbol` | 交易对 | HOTUSDT |
| `--start` | 起始日期 | 2025-10-09 |
| `--end` | 结束日期 | 2025-10-11 |
| `--fund` | 初始资金 | 10000 |
| `--fast` | 快速模式 | 无延迟 |

---

## 三、全流程闭环数据流

### 正常模式（生产）

```
1. Binance WS ──→ Kline1mStream ──→ 解析 ──→ KLine
                                                        ↓
2. DataFeeder.latest_ticks[symbol] = Tick { kline_1m }
                                                        ↓
3. 主循环 ──→ DataFeeder.ws_get_1m() ──→ Tick
                                                        ↓
4. TradingEngineV2.process_tick()
    ├── 触发器检查
    ├── StrategyQuery ──→ 策略 ──→ TradingDecision
    ├── RiskChecker.pre_check()
    └── gateway.place_order()
                                                        ↓
5. BinanceGateway ──→ Binance API ──→ 真实成交
                                                        ↓
6. OrderResult ──→ 状态更新 ──→ Account
```

### 沙盒模式（测试）

```
1. fetch_klines_from_api() ──→ 历史K线 ──→ StreamTickGenerator
                                                              ↓
2. 每根K线生成60个 Tick ──→ push_tick() ──→ DataFeeder
                                                              ↓
3. 主循环 ──→ DataFeeder.ws_get_1m() ──→ Tick
                                                              ↓
4. TradingEngineV2.process_tick()
    ├── 触发器检查
    ├── StrategyQuery ──→ 策略 ──→ TradingDecision
    ├── RiskChecker.pre_check()
    └── gateway.place_order()
                                                              ↓
5. ShadowBinanceGateway ──→ 模拟成交 ──→ OrderResult
                                                              ↓
6. OrderResult ──→ 状态更新 ──→ Account
```

---

## 四、数据流向汇总表

| 步骤 | 数据类型 | 从 | 到 | 方式 |
|-----|---------|----|----|-----|
| 1 | KLine | Binance WS | Kline1mStream | WebSocket |
| 2 | Tick | Kline1mStream | DataFeeder | push_tick() |
| 3 | KLine | DataFeeder | Engine | ws_get_1m() |
| 4 | StrategyQuery | Engine | 策略层 | 函数调用 |
| 5 | TradingDecision | 策略层 | Engine | 返回值 |
| 6 | RiskCheckResult | 风控层 | Engine | trait方法 |
| 7 | OrderResult | 网关层 | Engine | trait方法 |
| 8 | Account | Account | 网关层 | 内部状态 |

---

## 五、关键接口映射

| 上层调用 | 下层实现 | 接口 |
|---------|---------|-----|
| Engine | DataFeeder | `ws_get_1m(symbol)` |
| Engine | ExchangeGateway | `place_order(req)` |
| Engine | RiskChecker | `pre_check(order, account)` |
| Sandbox | DataFeeder | `push_tick(tick)` |
| Sandbox | ShadowBinanceGateway | `place_order(req)` |

---

## 六、TradeManager 异步任务架构（备选模式）

TradeManager 是一种备选架构，采用异步任务模式，与 TradingEngineV2 的同步模式不同。

### 6.1 四层自运行架构

```
┌─────────────────────────────────────────────────────────────────────┐
│  数据源层（后台自运行）                                            │
│  StreamTickGenerator → push_tick → DataFeeder                     │
└─────────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────────┐
│  指标层（后台自运行）                                              │
│  从 DataFeeder 获取 K 线 → 计算指标 → IndicatorCache               │
└─────────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────────┐
│  引擎层（监控波动率触发任务）                                       │
│  监控波动率 → 波动率 > 阈值 → spawn_task                         │
└─────────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────────┐
│  策略层（每个任务独立）                                            │
│  从 DataFeeder 获取价格                                            │
│  从 IndicatorCache 获取指标                                        │
│  策略计算 → 风控 → 下单                                            │
└─────────────────────────────────────────────────────────────────────┘
```

### 6.2 引擎层 vs 策略层职责

| 功能 | 引擎层 | 策略层 |
|-----|-------|--------|
| 波动率监控 | ✅ | ❌ |
| 触发任务 | ✅ | ❌ |
| 任务注册/移除 | ✅ | ❌ |
| 心跳检查 | ✅ | ❌ |
| 持久化 | ✅ | ❌ |
| 获取价格 | ❌ | ✅ 从 DataFeeder |
| 获取指标 | ❌ | ✅ 从 IndicatorCache |
| 策略计算 | ❌ | ✅ |
| 风控检查 | ❌ | ✅ |
| 下单执行 | ❌ | ✅ |

### 6.3 两种模式对比

| 特性 | TradingEngineV2 | TradeManager |
|-----|-----------------|--------------|
| 执行模式 | 同步，每 tick 处理 | 异步，任务独立循环 |
| 数据流 | Tick → 引擎 → 策略 → 风控 → 网关 | 引擎监控波动率触发任务，任务自行拉取数据 |
| 适用场景 | 高频、精确控制 | 多品种、长时间运行 |
| 实现位置 | `f_engine/src/core/engine_v2.rs` | `src/sandbox_main.rs` |

---

## 七、文档更新记录

| 日期 | 更新内容 |
|-----|---------|
| 2026-03-26 | 初始版本，完整分层数据流 |
| 2026-03-26 | 新增 TradeManager 异步任务架构章节 |
