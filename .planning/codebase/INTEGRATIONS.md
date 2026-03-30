集成
===

Binance REST API
---
a_common::api::BinanceApiGateway

端点:
- fetch_symbol_rules
- fetch_account_info
- fetch_position_risk
- fetch_klines

Binance WebSocket
---
a_common::ws::BinanceWsConnector

数据流:
- @trade
- @kline_1m
- @depth

限流
---
REQUEST_WEIGHT 限制
ORDERS 限制
通过 RateLimiter 组件实现

WebSocket 重连
---
指数退避重连策略

外部依赖
---
无外部数据库（仅 SQLite，内置）

无外部认证提供商

无云服务
