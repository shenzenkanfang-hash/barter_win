# Milestones

## v0.1: Foundation

**Goal**: Project scaffold and core infrastructure

- Workspace structure
- Error type definitions
- Core data structures (Order, Position, Fund)
- Logging setup

**Status**: Not started

---

## v0.2: Market Data Layer

**Goal**: WebSocket connection and K-line synthesis

- Exchange WebSocket connector
- K-line incremental synthesis
- Market data trait abstraction

**Status**: Not started

---

## v0.3: Indicator Layer

**Goal**: Core indicators with O(1) incremental calculation

- EMA incremental calculation
- Pine color (MACD + EMA + RSI)
- TR and price position

**Status**: Not started

---

## v0.4: Strategy Layer

**Goal**: Strategy trait and three strategy types

- Strategy trait definition
- Daily strategy
- Minute strategy
- Tick strategy

**Status**: Not started

---

## v0.5: Engine Layer

**Goal**: Core engine with risk check and order execution

- Engine core
- Risk pre-check (lock-free)
- Order execution with global lock
- Position management

**Status**: Not started
