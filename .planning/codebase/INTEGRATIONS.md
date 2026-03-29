================================================================================
INTEGRATIONS.md - 外部集成文档
================================================================================

项目: barter-rs 量化交易系统
路径: D:\Rust项目\barter-rs-main
最后更新: 2026-03-29

================================================================================
1. BINANCE REST API
================================================================================

1.1 API 网关 (BinanceApiGateway)
--------------------------------------------------------------------------------
位置: crates/a_common/src/api/binance_api.rs

功能: 币安 REST API 统一网关
- 限速管理 (RateLimiter)
- 交易规则获取 (SymbolRulesFetcher)
- 账户信息查询
- 持仓信息查询
- 杠杆设置

构造函数:
  BinanceApiGateway::new()           - 现货 API
  BinanceApiGateway::new_futures()   - USDT 合约实盘
  BinanceApiGateway::new_futures_with_testnet() - 实盘行情 + 测试网账户

1.2 REST API 端点
--------------------------------------------------------------------------------

账户相关:
  GET /api/v3/account
    - 获取账户信息
    - 返回: BinanceAccountInfo { account_type, can_trade, can_withdraw, can_deposit }

  GET /api/v3/positionRisk
    - 获取持仓风险信息
    - 参数: symbol (交易对)
    - 返回: PositionRisk { symbol, position_side, quantity, entry_price, mark_price, unrealized_pnl, leverage }

合约账户:
  GET /fapi/v2/account
    - 获取 USDT 合约账户信息
    - 返回: FuturesAccountResponse { total_margin_balance, available_balance, assets }

  GET /fapi/v2/positionRisk
    - 获取 USDT 合约持仓信息
    - 返回: Vec<FuturesPositionResponse>

交易规则:
  GET /api/v3/exchangeInfo
    - 获取交易所规则 (所有交易对)
    - 返回: BinanceExchangeInfo { rate_limits, symbols[] }

  GET /fapi/v1/leverageBracket
    - 获取杠杆档位
    - 返回: Vec<LeverageBracket> { symbol, bracket, max_leverage, min_notional }

  GET /fapi/v1/commissionRate
    - 获取交易手续费率
    - 参数: symbol
    - 返回: (maker_fee, taker_fee)

持仓管理:
  POST /fapi/v1/positionMode
    - 设置持仓模式 (双向/单向)
    - 参数: dualSidePosition

  POST /fapi/v1/leverage
    - 设置交易对杠杆倍数
    - 参数: symbol, leverage (1-125)

K线数据:
  GET /api/v3/klines
    - 获取历史 K线
    - 参数: symbol, interval, startTime, endTime, limit
    - 返回: Vec<Vec<serde_json::Value>>

1.3 限速器 (RateLimiter)
--------------------------------------------------------------------------------
位置: crates/a_common/src/api/binance_api.rs

限制类型:
  REQUEST_WEIGHT - 请求权重 (默认 2400/分钟)
  ORDERS         - 订单计数 (默认 1200/分钟)

策略:
  - 从响应 Header 实时获取已用权重 (x-mbx-used-weight-1m, x-mbx-order-count-1m)
  - 80% 阈值触发等待
  - 60 秒窗口自动重置

================================================================================
2. BINANCE WEBSOCKET STREAMS
================================================================================

2.1 WebSocket 连接器 (BinanceWsConnector)
--------------------------------------------------------------------------------
位置: crates/a_common/src/ws/binance_ws.rs

连接地址: wss://stream.binancefuture.com/ws/

连接类型:
  单 stream: BinanceWsConnector::new(symbol)
    - URL: wss://stream.binancefuture.com/ws/{symbol}@trade

  多 stream: BinanceWsConnector::new_multi(url, streams)
    - URL: wss://stream.binancefuture.com/ws
    - streams: ["btcusdt@kline_1m", "btcusdt@depth"]

2.2 WebSocket 消息类型
--------------------------------------------------------------------------------

AggTrade (逐笔成交):
  订阅: {symbol}@trade
  消息: BinanceTradeMsg {
    e: "trade",           // 事件类型
    E: 123456789,         // 事件时间
    s: "BTCUSDT",         // 交易对
    t: 12345,             // 交易 ID
    p: "50000.00",        // 价格
    q: "1.5",             // 数量
    T: 123456789,         // 交易时间
    m: true               // 是否买方主动卖
  }

Kline (K线):
  订阅: {symbol}@kline_{interval}
  消息: BinanceKlineMsg {
    e: "kline",           // 事件类型
    E: 123456789,         // 事件时间
    s: "BTCUSDT",         // 交易对
    k: KlineData {
      t: 123456780000,    // K线开始时间
      T: 123456789999,    // K线结束时间
      s: "BTCUSDT",       // 交易对
      i: "1m",            // K线周期
      o: "50000.00",      // 开盘价
      c: "50050.00",      // 收盘价
      h: "50080.00",      // 最高价
      l: "49990.00",      // 最低价
      v: "100.5",         // 成交量
      n: 50,              // 成交笔数
      x: false            // 是否收盘
    }
  }

Depth (订单簿):
  订阅: {symbol}@depth{levels}
  消息: BinanceDepthMsg {
    e: "depthUpdate",     // 事件类型
    E: 123456789,         // 事件时间
    s: "BTCUSDT",         // 交易对
    U: 1,                 // 最初更新 ID
    u: 50,                // 当前更新 ID
    b: [["50000.00","1.5"]],  // 买方深度
    a: [["50010.00","2.0"]]   // 卖方深度
  }

2.3 重连机制
--------------------------------------------------------------------------------
策略: 指数退避
  5s -> 10s -> 20s -> ... -> 120s (最大)
  最多重试 10 次

重连后自动重新订阅之前的 streams

================================================================================
3. SYMBOLRULESFETCHER
================================================================================

位置: crates/a_common/src/api/binance_api.rs

类型别名: pub type SymbolRulesFetcher = BinanceApiGateway;

功能:
  fetch_symbol_rules(symbol)                    - 获取单个交易对规则
  fetch_all_usdt_symbol_rules()                 - 批量获取所有 USDT 交易对规则
  fetch_and_save_all_usdt_symbol_rules()        - 获取并保存到文件
  enrich_with_leverage_brackets(&mut rules)     - 用杠杆档位丰富规则数据

SymbolRulesData 结构:
  {
    symbol: String,           // 交易对
    price_precision: u8,      // 价格精度
    quantity_precision: u8,   // 数量精度
    tick_size: Decimal,       // 价格步长
    min_qty: Decimal,         // 最小数量
    step_size: Decimal,       // 数量步长
    min_notional: Decimal,    // 最小名义价值
    max_notional: Decimal,    // 最大名义价值
    leverage: i32,            // 当前杠杆
    max_leverage: i32,        // 最大可用杠杆
    maker_fee: Decimal,       // Maker 费率
    taker_fee: Decimal        // Taker 费率
  }

================================================================================
4. PLATFORM DETECTION
================================================================================

位置: crates/a_common/src/config/platform.rs

4.1 Platform 枚举
--------------------------------------------------------------------------------
pub enum Platform {
  Windows,  // Windows 环境
  Linux,    // Linux 环境
}

检测逻辑:
  #[cfg(target_os = "windows")] -> Platform::Windows
  #[cfg(target_os = "linux")]   -> Platform::Linux
  其他 -> Platform::Linux

4.2 路径配置 (Paths)
--------------------------------------------------------------------------------
自动检测平台并选择对应路径:

| 数据类型         | Windows              | Linux                  |
|-----------------|---------------------|------------------------|
| 内存备份目录      | E:/shm/backup/       | /dev/shm/backup/       |
| 磁盘同步目录      | E:/backup/sync/      | data/backup/           |
| SQLite 数据库    | E:/backup/trading_events.db | data/trading_events.db |
| CSV 输出目录     | E:/backup/output/    | output/                |
| 交易规则目录      | E:/shm/backup/symbols_rules/ | /dev/shm/backup/symbols_rules/ |

4.3 存储方案
--------------------------------------------------------------------------------
1. SQLite 持久化 (硬盘)
   - 交易事件历史
   - 账户快照

2. 内存备份 (高速内存盘)
   - K线数据
   - Depth 数据
   - Trades 数据
   - 有效交易品种列表
   - 交易所信息 (exchange_info.json)

================================================================================
5. HTTP 客户端配置
================================================================================

位置: crates/a_common/src/api/binance_api.rs (new_http_client())

配置:
  超时: 30 秒 (timeout)
  连接超时: 15 秒 (connect_timeout)
  User-Agent: Mozilla/5.0 Chrome/120.0.0.0

代理支持:
  - 从环境变量 HTTP_PROXY / http_proxy 读取
  - 自动应用于 HTTPS 请求

================================================================================
