================================================================================
外部集成文档 - barter-rs 量化交易系统
================================================================================

项目: barter-rs Quantitative Trading System
版本: 0.1.0
更新: 2026-03-25

================================================================================
一、Binance 交易所集成
================================================================================

1. WebSocket 实时数据 (a_common/ws/binance_ws.rs)
--------------------------------------------------------------------------------

连接地址:
  - 实盘: wss://stream.binancefuture.com/ws/
  - 测试网: wss://stream.binancefuture.com/ws/

支持的数据流:
  - Trade Stream: <symbol>@trade
    * BinanceTradeMsg: event_type, event_time, symbol, trade_id,
      price, quantity, trade_time, is_buyer_maker

  - Kline Stream: <symbol>@kline_<interval>
    * BinanceKlineMsg: event_type, event_time, symbol, kline
    * KlineData: kline_start_time, kline_close_time, symbol,
      interval, first_trade_id, last_trade_id, open, close,
      high, low, volume, num_trades, is_closed

  - Depth Stream: <symbol>@depth
    * BinanceDepthMsg: event_type, event_time, symbol,
      first_update_id, final_update_id, bids, asks
    * PriceLevel: price, qty

重连策略:
  - 指数退避: 5s -> 10s -> 20s -> ... -> 120s (最大)
  - 自动重连直到成功

2. REST API (a_common/api/binance_api.rs)
--------------------------------------------------------------------------------

API 网关配置:

  BinanceApiGateway::new()           # 现货 API
  BinanceApiGateway::new_futures()    # USDT 合约 (实盘行情 + 实盘账户)
  BinanceApiGateway::new_futures_with_testnet()  # 实盘行情 + 测试网账户

API 端点:

  市场数据 (market_api_base):
    - https://api.binance.com          # 现货
    - https://fapi.binance.com         # USDT 合约

  账户数据 (account_api_base):
    - https://api.binance.com         # 现货
    - https://fapi.binance.com        # USDT 合约
    - https://testnet.binancefuture.com  # 测试网

  核心接口:
    GET /api/v3/exchangeInfo          # 交易对规则
    GET /api/v3/account               # 账户信息
    GET /api/v3/positionRisk          # 持仓风险
    GET /fapi/v1/leverageBracket      # 杠杆档位
    GET /fapi/v2/account              # USDT 合约账户
    GET /fapi/v2/positionRisk         # USDT 合约持仓
    GET /fapi/v1/commissionRate       # 手续费率
    POST /fapi/v1/positionMode        # 设置持仓模式
    POST /fapi/v1/leverage            # 设置杠杆倍数

  RateLimiter 限速器:
    - REQUEST_WEIGHT: 默认 2400/分钟 (合约)
    - ORDERS: 默认 1200/分钟 (合约)
    - 80% 阈值警告
    - Header 更新: x-mbx-used-weight-1m, x-mbx-order-count-1m

3. 交易对规则 (SymbolRulesData)
--------------------------------------------------------------------------------

从 exchangeInfo 解析:
  - symbol: 交易对名称
  - price_precision: 价格精度
  - quantity_precision: 数量精度
  - tick_size: 步长
  - min_qty: 最小数量
  - step_size: 步长数量
  - min_notional: 最小名义价值
  - max_notional: 最大名义价值 (默认 1,000,000)
  - leverage: 当前杠杆
  - max_leverage: 最大杠杆 (从 leverageBracket API 获取)
  - maker_fee: 做市商费率 (默认 0.0002)
  - taker_fee: 吃单费率 (默认 0.0005)

================================================================================
二、数据存储集成
================================================================================

1. SQLite 持久化 (e_risk_monitor/persistence/sqlite_persistence.rs)
--------------------------------------------------------------------------------

数据库: trading_events.db

表结构:
  - account_snapshots
      id, ts, account_id, total_equity, available, frozen_margin,
      unrealized_pnl, margin_ratio

  - exchange_positions
      id, ts, symbol, side, qty, avg_price, unrealized_pnl, margin_used

  - local_positions
      id, ts, symbol, strategy_id, direction, qty, avg_price,
      entry_ts, remark

  - channel_events
      id, ts, event, from_channel, to_channel, tr_ratio,
      ma5_in_20d_pos, pine_color, details

  - risk_events
      id, ts, event_type, symbol, order_id, reason, available_before,
      margin_ratio_before, action_taken, details

  - indicator_events
      id, ts, symbol, event, tr_ratio_5d_20d, tr_ratio_20d_60d,
      pos_norm_20, ma5_in_20d_pos, ma20_in_60d_pos, pine_color_20_50,
      pine_color_100_200, pine_color_12_26, channel_type, details

  - orders
      order_id (PK), symbol, side, qty, price, status, created_at, filled_at

  - sync_log
      id, sync_type, source, target, timestamp, details

索引:
  - idx_account_snapshots_ts
  - idx_exchange_positions_ts
  - idx_local_positions_ts
  - idx_channel_events_ts
  - idx_risk_events_ts
  - idx_indicator_events_ts
  - idx_orders_symbol
  - idx_orders_created_at
  - idx_sync_log_timestamp

2. 内存备份 (a_common/backup/memory_backup.rs)
--------------------------------------------------------------------------------

tmpfs 目录结构:
  根文件:
    - account.json
    - positions.json
    - trading_pairs.json
    - system_config.json

  目录:
    - channel/: minute.json, daily.json
    - depth/: <symbol>.json
    - trades/: <symbol>.csv
    - rules/: <symbol>.json
    - kline_1m_realtime/: <symbol>.json
    - kline_1m_history/: <symbol>.json
    - kline_1d_realtime/: <symbol>.json
    - kline_1d_history/: <symbol>.json
    - indicators_1m_realtime/: <symbol>.json
    - indicators_1m_history/: <symbol>.json
    - indicators_1d_realtime/: <symbol>.json
    - indicators_1d_history/: <symbol>.json
    - tasks/minute/: pool.json
    - tasks/daily/: pool.json
    - mutex/minute/: <symbol>.json
    - mutex/hour/: <symbol>.json

同步策略:
  - 定期同步: 30秒间隔
  - 从 tmpfs 同步到 disk_sync_dir
  - 故障恢复时从内存盘加载

3. CSV 导出
--------------------------------------------------------------------------------

IndicatorCsvWriter:
  - 文件: indicator_comparison.csv
  - 列: timestamp, symbol, tr_ratio_5d_20d, tr_ratio_20d_60d,
        pos_norm_20, ma5_in_20d_pos, ma20_in_60d_pos, pine_color_20_50,
        pine_color_100_200, pine_color_12_26, vel_percentile,
        acc_percentile, power, channel_type
  - 最大文件大小: 100MB (超过则创建新文件)

================================================================================
三、ExchangeGateway Trait
================================================================================

f_engine/src/order/gateway.rs 定义统一接口:

pub trait ExchangeGateway: Send + Sync {
    fn place_order(&self, req: OrderRequest) -> Result<OrderResult, EngineError>;
    fn get_account(&self) -> Result<ExchangeAccount, EngineError>;
    fn get_position(&self, symbol: &str) -> Result<Option<ExchangePosition>, EngineError>;
}

实现:
  - MockBinanceGateway: 模拟交易网关 (测试用)

================================================================================
四、错误类型
================================================================================

a_common/src/claint/error.rs:

EngineError:
  - RiskCheckFailed(String)
  - OrderExecutionFailed(String)
  - LockFailed(String)
  - InsufficientFund(String)
  - PositionLimitExceeded(String)
  - ModeSwitchFailed(String)
  - Network(String)
  - MemoryBackup(String)
  - SymbolNotFound(String)
  - Other(String)

MarketError:
  - WebSocketConnectionFailed(String)
  - WebSocketError(String)
  - SerializeError(String)
  - SubscribeFailed(String)
  - UnsubscribeFailed(String)
  - ParseError(String)
  - KLineError(String)
  - OrderBookError(String)
  - Timeout(String)
  - RedisError(String)
  - NetworkError(String)

================================================================================
五、配置与启动
================================================================================

1. 命令行参数 (clap)
--------------------------------------------------------------------------------

支持多交易对:
  --symbols=BTCUSDT,ETHUSDT,BNBUSDT

2. 环境变量
--------------------------------------------------------------------------------

RUSTC: 编译器路径
PLATFORM: Windows/Linux 自动检测

3. Platform 检测
--------------------------------------------------------------------------------

Platform::detect():
  - Windows: cfg(target_os = "windows")
  - Linux: 检查 /dev/shm 是否存在

================================================================================
