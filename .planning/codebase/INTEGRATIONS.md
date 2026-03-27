================================================================================
INTEGRATIONS.md - External APIs and Integrations
================================================================================

Binance API Integration
================================================================================

The system integrates with Binance APIs for market data and trading.

REST API Gateway
--------------------------------------------------------------------------------
Location: crates/a_common/src/api/binance_api.rs

Base URLs:
  - Production Market:  https://fapi.binance.com
  - Production Account: https://fapi.binance.com
  - Testnet Market:     https://testnet.binancefuture.com
  - Testnet Account:    https://testnet.binancefuture.com

Key Endpoints Used:
  - GET /fapi/v1/exchangeInfo     - Exchange trading rules and symbol info
  - GET /fapi/v2/account          - Account information
  - GET /fapi/v2/positionRisk     - Position risk information
  - GET /fapi/v1/leverageBracket   - Leverage brackets

Rate Limiting:
  - Implements dual-rate limiting: REQUEST_WEIGHT and ORDERS per minute
  - Default limits: 2400 REQUEST_WEIGHT/min, 1200 ORDERS/min
  - Header-based usage tracking: x-mbx-used-weight-1m, x-mbx-order-count-1m
  - 80% threshold warning system

RateLimiter Struct (binance_api.rs lines 53-68):
  - request_weight_limit: Mutex<u32>
  - orders_limit: Mutex<u32>
  - Priority queues: High (orders), Medium (account), Low (market data)

SymbolRulesFetcher Trait:
  - Type alias for BinanceApiGateway
  - Fetches trading rules: price_precision, qty_precision, min_qty, max_qty, etc.

WebSocket Integration
--------------------------------------------------------------------------------
Location: crates/a_common/src/ws/binance_ws.rs

Connection URLs:
  - Production: wss://stream.binancefuture.com/ws/
  - Testnet: wss://stream.binancefuture.com/ws/

Stream Types:
  - @trade      - Real-time trade stream
  - @kline_Xm   - K-line/candlestick stream (1m, 5m, etc.)
  - @depth      - Order book depth stream

BinanceWsConnector Struct:
  - new()        - Single symbol trade stream
  - new_multi()  - Multi-stream connection for batch subscriptions
  - connect()    - Async WebSocket connection

Message Types (with Binance field names):
  - BinanceTradeMsg:    e=eventType, E=eventTime, s=symbol, t=tradeId, p=price, q=qty, T=tradeTime, m=isBuyerMaker
  - BinanceKlineMsg:    e=eventType, E=eventTime, s=symbol, k=KlineData
  - BinanceDepthMsg:    e=eventType, E=eventTime, s=symbol, U=firstUpdateId, u=finalUpdateId, b=bids, a=asks

KlineData Fields: t=klineStartTime, T=klineCloseTime, s=symbol, i=interval,
                  f=firstTradeId, L=lastTradeId, o=open, c=close, h=high, l=low,
                  v=volume, n=numTrades, x=isClosed

Redis Integration
================================================================================

Purpose: High-performance memory backup for trading state

Location: crates/b_data_source/Cargo.toml (redis 0.27)

Usage Patterns:
  - Connection manager for async operations
  - Used in: b_data_source (recovery.rs, symbol_registry.rs)

Features Used:
  - tokio-comp - Tokio runtime integration
  - connection-manager - Connection pooling

SQLite Integration
================================================================================

Purpose: Persistent event storage and disaster recovery

Locations:
  - crates/e_risk_monitor/src/persistence/sqlite_persistence.rs
  - crates/c_data_process/src/strategy_state/db.rs

Version: rusqlite 0.32 (bundled)

Tables Created:
  - account_snapshots       - Account equity snapshots
  - exchange_positions      - Exchange position records
  - local_positions         - Local strategy positions
  - channel_events         - Channel mode switch events
  - risk_events            - Risk rejection/liquidation events
  - indicator_events       - Indicator significant changes

Key Structs (sqlite_persistence.rs):
  - AccountSnapshotRecord
  - ExchangePositionRecord
  - LocalPositionRecord
  - ChannelEventRecord
  - RiskEventRecord
  - IndicatorEventRecord

Parquet Integration (Sandbox Only)
================================================================================

Purpose: Historical market data replay for backtesting

Location: crates/mock 组件/Cargo.toml (parquet 56)

Features: snap compression, default-features disabled

Usage: crates/mock 组件/src/backtest/loader.rs
  - Load historical tick/kline data from Parquet files
  - Replay market conditions for strategy testing

Telegram Notification
================================================================================

Location: crates/a_common/src/util/telegram_notifier.rs

Purpose: Alert and notification delivery

Implementation: HTTP-based bot API calls via reqwest

Platform Detection
================================================================================

Automatic platform-aware path selection:

Location: crates/a_common/src/backup/mod.rs

Windows:
  - Primary: E:/shm/backup/ (high-speed memory disk)
  - Fallback: E:/backup/

Linux:
  - Primary: /dev/shm/backup/
  - Fallback: data/backup/

Database Paths:
  - Windows: E:/backup/trading_events.db
  - Linux: data/trading_events.db

Mock Implementations (for testing)
================================================================================

MockBinanceGateway:
  - Location: crates/f_engine/src/order/mock_binance_gateway.rs
  - Simulates: Account, positions, orders, margin, risk checks
  - Used in: Sandbox testing, strategy backtesting

================================================================================
End of INTEGRATIONS.md
================================================================================
