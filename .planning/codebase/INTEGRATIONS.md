================================================================================
Binance 交易所集成
================================================================================

Binance 是系统唯一的外部交易所，所有交易对 USDT 合约。

--------------------------------------------------------------------------------
API 端点
--------------------------------------------------------------------------------

现货 API
--------
基础URL: https://api.binance.com
端点:
  - GET /api/v3/exchangeInfo      交易所信息 (交易对规则)
  - GET /api/v3/account           账户信息
  - GET /api/v3/positionRisk     持仓风险
  - GET /api/v3/klines            历史K线

USDT 合约 API
-------------
基础URL: https://fapi.binance.com
端点:
  - GET  /fapi/v1/klines              历史K线
  - GET  /fapi/v1/leverageBracket      杠杆档位
  - POST /fapi/v1/leverage             设置杠杆
  - POST /fapi/v1/positionMode        设置持仓模式
  - GET  /fapi/v2/account             账户信息
  - GET  /fapi/v2/positionRisk        持仓风险
  - GET  /fapi/v1/commissionRate      手续费率

测试网
------
账户API: https://testnet.binancefuture.com
说明: 实盘行情 + 测试网账户组合用于模拟交易

WebSocket 端点
--------------
基础URL: wss://stream.binancefuture.com/ws/
说明: 多路复用 WebSocket，订阅格式: <symbol>@<stream>

支持的 Stream:
  - <symbol>@trade           实时成交
  - <symbol>@kline_<interval> K线 (1m, 5m, 15m, 1h, 1d 等)
  - <symbol>@depth           订单簿深度

--------------------------------------------------------------------------------
API 网关组件 (BinanceApiGateway)
--------------------------------------------------------------------------------

模块位置: a_common/src/api/binance_api.rs

功能:
  - fetch_symbol_rules()       获取交易对规则 (价格/数量精度, 手续费等)
  - fetch_all_usdt_symbol_rules() 批量获取所有 USDT 交易对规则
  - fetch_account_info()       获取账户信息
  - fetch_position_risk()      获取持仓风险
  - fetch_leverage_brackets()  获取杠杆档位
  - fetch_futures_account()   获取合约账户信息
  - fetch_futures_positions() 获取持仓信息
  - change_leverage()          设置杠杆倍数
  - change_position_mode()     设置持仓模式 (双向/单向)
  - get_commission_rate()      获取手续费率

--------------------------------------------------------------------------------
历史数据客户端 (HistoryApiClient)
--------------------------------------------------------------------------------

模块位置: a_common/src/api/kline_fetcher.rs

功能:
  - fetch_klines()  拉取历史K线
  - fetch_batch()   批量获取多个品种K线

配置:
  - 最大重试次数: 3次
  - 初始退避: 100ms
  - 最大退避: 5秒
  - Jitter: 0.5 + random(0, 0.5)
  - 并发限制: 5个请求 (Semaphore)

--------------------------------------------------------------------------------
WebSocket 连接器 (BinanceWsConnector)
--------------------------------------------------------------------------------

模块位置: a_common/src/ws/binance_ws.rs

功能:
  - connect()              建立连接
  - subscribe()           订阅 stream
  - unsubscribe()         退订 stream
  - reconnect_with_backoff() 指数退避重连

消息类型:
  - BinanceTradeMsg       成交消息
  - BinanceKlineMsg      K线消息
  - BinanceDepthMsg      订单簿深度

重连策略:
  - 5s -> 10s -> 20s -> ... -> 120s (最大)
  - 最多重试 10 次

--------------------------------------------------------------------------------
限速器 (RateLimiter)
--------------------------------------------------------------------------------

模块位置: a_common/src/api/binance_api.rs

限制类型:
  - REQUEST_WEIGHT: 每分钟请求权重限制
  - ORDERS: 每分钟订单数限制

策略:
  - 从 exchangeInfo 解析限制值
  - 从响应 Header 更新已用权重 (x-mbx-used-weight-1m, x-mbx-order-count-1m)
  - 80% 阈值触发等待

================================================================================
数据库 (SQLite)
================================================================================

位置: e_risk_monitor/src/persistence/sqlite_persistence.rs
      c_data_process/src/strategy_state/db.rs

服务: SqliteRecordService

表结构:

account_snapshots
  - id, ts, account_id, total_equity, available, frozen_margin, unrealized_pnl, margin_ratio

exchange_positions
  - id, ts, symbol, side, qty, avg_price, unrealized_pnl, margin_used

local_positions
  - id, ts, symbol, strategy_id, direction, qty, avg_price, entry_ts, remark

channel_events
  - id, ts, event, from_channel, to_channel, tr_ratio, ma5_in_20d_pos, pine_color, details

risk_events
  - id, ts, event_type, symbol, order_id, reason, available_before, margin_ratio_before, action_taken, details

indicator_events
  - id, ts, symbol, event, tr_ratio_5d_20d, tr_ratio_20d_60d, pos_norm_20, ma5_in_20d_pos, ma20_in_60d_pos, pine_color_20_50, pine_color_100_200, pine_color_12_26, channel_type, details

orders
  - order_id (PK), symbol, side, qty, price, status, created_at, filled_at

sync_log
  - id, sync_type, source, target, timestamp, details

================================================================================
Redis 缓存
================================================================================

位置: b_data_source/src/recovery.rs
包: redis 0.27 (tokio-comp, connection-manager)

用途: 仅作为灾备恢复
  - 程序崩溃后能快速恢复交易判断
  - 不用等待 15min 窗口结束
  - 不用重新 warm-up 指标

存储内容:
  - K线数据快照
  - 指标快照
  - 高波动窗口标记
  - 最后 checkpoint 时间戳

组件:
  - RedisRecovery    Redis 恢复接口
  - CheckpointData   检查点数据结构
  - CheckpointManager 检查点管理器

================================================================================
外部依赖总览
================================================================================

交易所
------
Binance (USDT 合约)
  - REST API (现货 + 合约)
  - WebSocket API (合约)
  - 测试网 (testnet.binancefuture.com)

数据库
------
SQLite 0.32 (bundled)
  - 本地持久化存储
  - 账户/持仓/订单/事件记录

缓存
----
Redis 0.27
  - 仅用于灾备恢复
  - 非生产数据存储

代理
----
支持 HTTP_PROXY / http_proxy 环境变量
  - reqwest HTTP 客户端
  - WebSocket 连接

================================================================================
认证与安全
================================================================================

API 认证
--------
说明: 当前代码中 API 网关仅用于获取公开数据 (交易对规则、K线等)
      账户操作 (下单、查询持仓) 在 MockApiGateway 中模拟
      实盘交易需要添加签名认证

TLS/SSL
-------
native-tls 0.2 提供 TLS 支持
  - reqwest HTTPS 支持
  - tokio-tungstenite WSS 支持

================================================================================
数据流向图
================================================================================

[实盘/回放]
    |
    v
[WebSocket] --> BinanceWsConnector --> MarketDataStore
    |                                      |
    |                                      v
    |                               [指标计算]
    |                                      |
    v                                      v
[Binance API] --> BinanceApiGateway --> SymbolRegistry
    |                       |
    |                       v
    |               [交易规则缓存]
    |
    v
[Engine] --> [Risk Check] --> [Order Execution]
                              |
                              v
                        [MockApiGateway / Real API]
                              |
                              v
                        [Account/Position Update]

================================================================================
配置示例
================================================================================

环境变量:
  HTTP_PROXY=http://127.0.0.1:7890
  RUST_LOG=info
  RUST_BACKTRACE=1

数据目录:
  symbols_rules/     交易对规则 JSON
  memory_backup/     内存备份 (账户快照、持仓、指标)
  kline_data/        K线数据缓存
