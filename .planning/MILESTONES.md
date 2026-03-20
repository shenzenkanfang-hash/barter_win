# Milestones

## v0.1: Foundation

**Goal**: Project scaffold and core infrastructure

- Workspace structure
- Error type definitions
- Core data structures (Order, Position, Fund)
- Logging setup

**Status**: COMPLETE

---

## v0.2: Market Data Layer

**Goal**: WebSocket connection and K-line synthesis

- Exchange WebSocket connector (trait)
- K-line incremental synthesis
- Market data trait abstraction
- MockMarketStream for testing

**Status**: COMPLETE

---

## v0.3: Indicator Layer

**Goal**: Core indicators with O(1) incremental calculation

- EMA incremental calculation
- Pine color (MACD + EMA + RSI)
- TR and price position
- RSI relative strength index

**Status**: COMPLETE

---

## v0.4: Strategy Layer

**Goal**: Strategy trait and three strategy types

- Strategy trait definition
- Signal, TradingMode, OrderRequest types
- Order side abstraction

**Status**: COMPLETE

---

## v0.5: Engine Layer

**Goal**: Core engine with risk check and order execution

- Engine core (TradingEngine)
- Risk pre-check (lock-free)
- Order execution with global lock
- Position management (types conversion)
- ModeSwitcher for volatility detection

**Status**: COMPLETE

---

## v0.6: Integration

**Goal**: Full trading flow integration

- main.rs entry point
- Component wiring
- Mock data flow
- End-to-end compilation

**Status**: COMPLETE (代码实现完成,待编译验证)
