# ShadowBinanceGateway 劫持模式设计文档

## 1. 目标

实现一个**劫持模式**的模拟网关：
- 账户/持仓/下单 API → 使用本地模拟账户
- 行情 API（K线/Ticker等）→ 转发到真实 Binance API

```
┌─────────────────────────────────────────────────────────────┐
│                      ShadowBinanceGateway                    │
│                    (劫持模式模拟网关)                        │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   ┌─────────────────────────────────────────────────────┐   │
│   │              模拟层 (本地计算)                        │   │
│   │  ┌─────────────┐  ┌──────────────┐  ┌───────────┐  │   │
│   │  │ MockAccount │  │MockPositions │  │MockOrders │  │   │
│   │  │  账户余额   │  │   持仓数据   │  │  订单记录 │  │   │
│   │  └─────────────┘  └──────────────┘  └───────────┘  │   │
│   │         ↓                  ↓                ↓        │   │
│   │  fetch_account()   fetch_positions()  place_order()│   │
│   └─────────────────────────────────────────────────────┘   │
│                           │                                │
│                           │ 行情请求                       │
│                           ↓                                │
│   ┌─────────────────────────────────────────────────────┐   │
│   │              转发层 (真实 Binance)                    │   │
│   │  fetch_klines() / fetch_ticker() / fetch_depth()   │   │
│   └─────────────────────────────────────────────────────┘   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

## 2. 文件结构

```
h_sandbox/src/
├── lib.rs                    # [修改] 添加模块导出
├── mock_binance_gateway.rs    # [保留] 现有的完整模拟网关
├── shadow_gateway.rs          # [新增] 劫持模式网关
└── shadow_account.rs         # [新增] 模拟账户核心逻辑

# 文档
h_sandbox/docs/
└── shadow_gateway_design.md   # 本文档
```

## 3. API 劫持对照表

| Binance API 端点 | 处理方式 | 返回数据源 |
|-----------------|---------|-----------|
| `GET /fapi/v2/account` | **劫持** | MockAccount 本地计算 |
| `GET /fapi/v2/positionRisk` | **劫持** | MockPosition 本地计算 |
| `POST /fapi/v1/order` | **劫持** | 模拟订单执行 |
| `GET /fapi/v1/klines` | **转发** | 真实 Binance API |
| `GET /fapi/v1/ticker/24hr` | **转发** | 真实 Binance API |
| `GET /fapi/v1/depth` | **转发** | 真实 Binance API |
| `GET /api/v3/exchangeInfo` | **转发** | 真实 Binance API |
| `GET /fapi/v1/leverageBracket` | **转发** | 真实 Binance API |

## 4. 返回数据格式（与 Binance 完全一致）

### 4.1 账户响应 `FuturesAccountResponse`

```rust
// 与 binance_api.rs 中的定义完全一致
#[derive(Debug, Clone, Deserialize)]
pub struct FuturesAccountResponse {
    #[serde(rename = "totalMarginBalance")]
    pub total_margin_balance: String,      // 总保证金余额
    #[serde(rename = "totalUnrealizedProfit")]
    pub total_unrealized_profit: String,  // 未实现盈亏
    #[serde(rename = "availableBalance")]
    pub available_balance: String,         // 可用余额
    #[serde(rename = "totalMaintMargin")]
    pub total_maint_margin: String,       // 维持保证金
    #[serde(rename = "updateTime")]
    pub update_time: i64,
    pub assets: Vec<FuturesAsset>,
}
```

**本地计算规则**：
- `total_margin_balance` = wallet_balance（从配置初始化）
- `total_unrealized_profit` = 所有持仓的未实现盈亏之和
- `available_balance` = 总权益 - 冻结保证金
- `total_maint_margin` = sum(持仓数量 * 标记价格 * 0.5%)

### 4.2 持仓响应 `FuturesPositionResponse`

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct FuturesPositionResponse {
    pub symbol: String,
    #[serde(rename = "positionSide")]
    pub position_side: String,            // LONG / SHORT
    #[serde(rename = "positionAmt")]
    pub position_amt: String,            // 持仓数量
    #[serde(rename = "entryPrice")]
    pub entry_price: String,             // 入场价格
    #[serde(rename = "markPrice")]
    pub mark_price: String,              // 标记价格（外部注入）
    #[serde(rename = "unrealizedProfit")]
    pub unrealized_profit: String,       // 未实现盈亏
    pub leverage: String,
    #[serde(rename = "marginRatio")]
    pub margin_ratio: String,
}
```

**本地计算规则**：
- `unrealized_profit` = `(markPrice - entryPrice) * positionAmt` (多头)
- `unrealized_profit` = `(entryPrice - markPrice) * positionAmt` (空头)

### 4.3 订单响应 `FuturesOrderResponse`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuturesOrderResponse {
    pub order_id: i64,
    pub symbol: String,
    pub side: String,              // BUY / SELL
    pub position_side: String,      // LONG / SHORT
    pub order_type: String,         // MARKET / LIMIT
    pub orig_quantity: String,      // 原始数量
    pub executed_qty: String,       // 成交数量
    pub avg_price: String,          // 成交均价
    pub status: String,             // NEW / FILLED / PARTIALLY_FILLED / CANCELLED
    pub transact_time: i64,
}
```

## 5. ShadowAccount 核心逻辑

### 5.1 数据结构

```rust
/// 模拟账户状态
pub struct ShadowAccount {
    /// 钱包余额（包含已实现盈亏）
    wallet_balance: Decimal,
    /// 手续费率
    fee_rate: Decimal,
    /// 持仓（Hedge 模式）
    positions: FnvHashMap<String, ShadowPosition>,
    /// 当前价格映射（外部注入）
    price_map: FnvHashMap<String, Decimal>,
    /// 初始保证金
    initial_balance: Decimal,
}

pub struct ShadowPosition {
    symbol: String,
    /// 多头数量
    long_qty: Decimal,
    long_entry_price: Decimal,
    long_margin: Decimal,
    /// 空头数量
    short_qty: Decimal,
    short_entry_price: Decimal,
    short_margin: Decimal,
}
```

### 5.2 核心计算

```rust
impl ShadowAccount {
    /// 开仓
    pub fn open(&mut self, symbol: &str, side: Side, qty: Decimal, price: Decimal, leverage: i32) {
        let notional = qty * price;
        let margin = notional / leverage as u32;  // 初始保证金
        let fee = notional * self.fee_rate;        // 手续费

        // 扣除手续费
        self.wallet_balance -= fee;

        // 更新持仓
        match side {
            Side::Buy => { /* 增加多头 */ }
            Side::Sell => { /* 增加空头 */ }
        }
    }

    /// 平仓
    pub fn close(&mut self, symbol: &str, side: Side, qty: Decimal, price: Decimal) {
        // 计算已实现盈亏
        // 释放保证金
        // 扣除手续费
    }

    /// 更新价格（计算未实现盈亏）
    pub fn update_price(&mut self, symbol: &str, price: Decimal) {
        self.price_map.insert(symbol.to_string(), price);
        // 重算所有该 symbol 持仓的未实现盈亏
    }

    /// 爆仓检测
    pub fn check_liquidation(&mut self) -> bool {
        // Margin Balance < Maintenance Margin → 强平
    }
}
```

## 6. ShadowBinanceGateway 接口设计

### 6.1 结构体

```rust
pub struct ShadowBinanceGateway {
    /// 模拟账户
    account: ShadowAccount,
    /// 下一个订单ID
    next_order_id: AtomicU64,
    /// 真实API网关（用于行情等）
    real_gateway: BinanceApiGateway,
}

impl ShadowBinanceGateway {
    pub fn new(initial_balance: Decimal) -> Self {
        Self {
            account: ShadowAccount::new(initial_balance),
            next_order_id: AtomicU64::new(1),
            real_gateway: BinanceApiGateway::new_futures(),
        }
    }

    /// 注入外部价格（用于计算未实现盈亏）
    pub fn update_price(&self, symbol: &str, price: Decimal) {
        self.account.update_price(symbol, price);
    }
}
```

### 6.2 劫持的 API（用自己的）

```rust
impl ShadowBinanceGateway {
    /// 获取账户信息
    pub async fn fetch_futures_account(&self) -> Result<FuturesAccountResponse, EngineError> {
        // 从 ShadowAccount 计算
    }

    /// 获取持仓信息
    pub async fn fetch_futures_positions(&self) -> Result<Vec<FuturesPositionResponse>, EngineError> {
        // 从 ShadowAccount 计算
    }

    /// 下单
    pub async fn place_order(&self, req: OrderRequest) -> Result<FuturesOrderResponse, EngineError> {
        // 模拟订单执行
    }
}
```

### 6.3 转发的 API（用真实的）

```rust
impl ShadowBinanceGateway {
    /// 获取K线数据
    pub async fn fetch_klines(&self, symbol: &str, interval: &str, limit: u32) 
        -> Result<Vec<KlineData>, EngineError> 
    {
        self.real_gateway.fetch_klines(symbol, interval, limit).await
    }

    /// 获取24小时ticker
    pub async fn fetch_ticker_24hr(&self, symbol: &str) 
        -> Result<Ticker24hrResponse, EngineError> 
    {
        self.real_gateway.fetch_ticker_24hr(symbol).await
    }

    /// 获取深度数据
    pub async fn fetch_depth(&self, symbol: &str, limit: u32) 
        -> Result<DepthResponse, EngineError> 
    {
        self.real_gateway.fetch_depth(symbol, limit).await
    }
}
```

## 7. 集成方式

### 7.1 替换网关（最小改动）

```rust
// 原来
let gateway = BinanceApiGateway::new_futures();

// 现在
let gateway: Box<dyn AccountGateway> = if is_simulation {
    Box::new(ShadowBinanceGateway::new(initial_balance))
} else {
    Box::new(BinanceApiGateway::new_futures())
};
```

### 7.2 外部价格注入

```rust
// 每次收到行情tick，更新到ShadowAccount
shadow_gateway.update_price("BTCUSDT", tick.price);

// ShadowAccount 自动计算所有持仓的未实现盈亏
```

## 8. 数据流

```
1. 策略生成信号
       ↓
2. 调用 gateway.place_order()
       ↓
3. ShadowBinanceGateway 模拟成交
   - 计算手续费
   - 更新 MockPosition
   - 生成订单响应
       ↓
4. 返回标准 Binance 格式的订单响应
       ↓
5. 策略接收成交回报
       ↓
6. 行情tick更新
       ↓
7. shadow_gateway.update_price(symbol, price)
       ↓
8. 自动重算未实现盈亏
       ↓
9. 下次 fetch_account() 返回最新权益
```

## 9. 实现清单

| 文件 | 操作 | 说明 |
|-----|------|------|
| `h_sandbox/src/lib.rs` | 修改 | 添加 `shadow_gateway` 模块导出 |
| `h_sandbox/src/shadow_account.rs` | 新增 | 模拟账户核心逻辑 |
| `h_sandbox/src/shadow_gateway.rs` | 新增 | 劫持模式网关实现 |

## 10. 与现有 mock_binance_gateway.rs 的关系

```
h_sandbox/
├── mock_binance_gateway.rs    # 完整模拟（包含风控、事件记录、SQLite持久化）
│                              # 适合回测
│
└── shadow_gateway.rs          # 劫持模式（只劫持账户/持仓/下单）
                               # 行情走真实API，适合半模拟半实盘测试
```

**区别**：
- `mock_binance_gateway.rs`: 100% 模拟，包括风控逻辑
- `shadow_gateway.rs`: 只劫持账户/持仓/下单，行情用真实API

## 11. 使用场景

| 场景 | 使用网关 |
|-----|---------|
| 回测 | `mock_binance_gateway.rs` |
| 策略开发/调试 | `shadow_gateway.rs` |
| 实盘 | `BinanceApiGateway` |
| 模拟实盘（真实行情+模拟账户） | `shadow_gateway.rs` |

## 12. 注意事项

1. **价格注入时机**：需要在每次行情更新时调用 `update_price()`
2. **订单频率**：模拟账户不做频率限制，实盘需要
3. **强平逻辑**：Cross Margin 模式，Margin Balance < Maintenance Margin 时强平
4. **滑点**：模拟订单默认无滑点，可配置添加
