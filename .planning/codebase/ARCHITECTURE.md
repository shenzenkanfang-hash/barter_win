# Architecture

## Design Pattern

**Layered Architecture** with strict dependency direction:
- Infrastructure (`a_common`) has no business type dependency
- Business logic (`b_data_source`) depends on infrastructure
- Higher layers depend on lower layers

## Module Hierarchy

```
┌─────────────────────────────────────────────┐
│              engine (Execution)              │
├─────────────────────────────────────────────┤
│  e_strategy │ d_risk_monitor │ c_data_proc │
├─────────────────────────────────────────────┤
│            b_data_source                    │
├─────────────────────────────────────────────┤
│              a_common                       │
└─────────────────────────────────────────────┘
```

## Layer Responsibilities

### a_common (Infrastructure)
- **Purpose**: API/WS gateways, pure infrastructure
- **No business types**: Only generic market data types
- **Modules**: `api/`, `ws/`, `error.rs`

### b_data_source (Business Data)
- **Purpose**: DataFeeder, K-line synthesis, Tick generation
- **Depends on**: `a_common`
- **Key types**: `MarketStream`, `Tick`, `Kline`

### c_data_process (Processing)
- **Purpose**: Indicator calculation, signal generation
- **Indicators**: EMA, RSI, MACD, Pine color system

### d_risk_monitor (Risk Control)
- **Purpose**: Position limits, risk checks
- **Key files**: `check_table.rs`, `thresholds.rs`

### e_strategy (Strategy)
- **Purpose**: Daily/minute/Tick strategy execution
- **Types**: `DailyStrategy`, `MinuteStrategy`, `TickStrategy`

### engine (Execution)
- **Purpose**: Orchestrates all layers
- **Subdirectories**: `core/`, `risk/`, `order/`, `position/`, `persistence/`, `channel/`, `shared/`

## Key Abstractions

### Gateway Pattern
- `BinanceApiGateway` — REST operations
- `BinanceWsConnector` — WebSocket connection
- Mock variants in `b_data_mock`

### Repository Pattern
- `SymbolRegistry` — symbol metadata
- `AccountPool` — account state

### Pipeline Pattern
- `Pipeline` — tick → indicator → signal → order flow

## Data Flow

```
WebSocket Tick ─► DataFeeder ─► KlinePersistence ─► PineIndicator
                                    │
                                    ▼
                             SymbolRegistry
                                    │
                                    ▼
                          Engine (Risk + Order)
                                    │
                                    ▼
                            Binance Gateway
```
