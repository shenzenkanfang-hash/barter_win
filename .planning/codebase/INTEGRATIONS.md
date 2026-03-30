INTEGRATIONS
===

Binance REST API
---
a_common::api::BinanceApiGateway

Endpoints:
- fetch_symbol_rules
- fetch_account_info
- fetch_position_risk
- fetch_klines

Binance WebSocket
---
a_common::ws::BinanceWsConnector

Streams:
- @trade
- @kline_1m
- @depth

Rate Limiting
---
REQUEST_WEIGHT limit
ORDERS limit
Via RateLimiter component

WebSocket Reconnection
---
Exponential backoff reconnection strategy

External Dependencies
---
No external database (SQLite only, bundled)

No external auth providers

No cloud services
