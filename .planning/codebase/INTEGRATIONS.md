# External Integrations

## Binance Exchange

### REST API

- **Gateway**: `BinanceApiGateway` in `a_common/api/`
- **Endpoints Used**:
  - `/api/v3/account` — account info
  - `/api/v3/order` — place/query/cancel orders
  - `/api/v3/myTrades` — trade history
  - `/fapi/v1/leverageBracket` — margin/brackets
  - `/fapi/v1/account` — futures account
  - `/api/v3/exchangeInfo` — symbol trading rules

### WebSocket Streams

- **Trade Stream**: `BinanceTradeStream` in `a_common/ws/`
- **Kline Stream**: Via `BinanceWsConnector`
- **Format**: Combined streams `stream_name@aggTrade`, `stream_name@kline_1m`

### Symbol Rules

- **Fetcher**: `SymbolRulesFetcher` in `a_common/api/`
- **Storage**: SQLite `symbol_rules` table
- **Used by**: Risk management, position limits

## Data Flow

```
Binance REST API ──► BinanceApiGateway ──► DataFeeder ──► MarketStream
Binance WS ────────► BinanceWsConnector ──► KlinePersistence ──► PineIndicator
                                              │
                                              ▼
                                        SymbolRegistry
```

## No External Authentication

- **Auth Method**: API Key + Secret in gateway (no OAuth)
- **Key Storage**: Environment or config (not hardcoded)

## Platform Detection

- **Module**: `Platform::detect()` in `engine/src/shared/`
- **Auto-select**: Windows → E: drive paths, Linux → /dev/shm
