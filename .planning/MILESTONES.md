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

---

## v0.7: Pipeline Architecture

**Goal**: Species-level pipeline parallel architecture

- Check table (CheckTable)
- Round guard (one-round encoding)
- PipelineForm (full flow form)
- SymbolRules (trading pair rules)
- VolatilityChannel (slow/fast channel)
- Position mutex check

**Status**: COMPLETE

---

## v0.8: Risk Control Enhancement

**Goal**: Three-layer risk architecture

- AccountPool: Account margin pool with circuit breaker
- StrategyPool: Strategy margin pool with rebalancing
- OrderCheck: Order risk checker with Lua script
- PnlManager: Profit/loss management
- RiskReChecker: Lock-in risk re-check

**Status**: COMPLETE

---

## v0.9: Strategy Enhancement

**Goal**: Strategy state machine

- TrendStrategy: Trend strategy state machine
- PinStrategy: Martin/pin strategy state machine
- ZScore indicator framework
- TRRatio indicator framework
- MarketStatusDetector: Market status detection

**Status**: COMPLETE

---

## v0.10: Persistence & Indicators

**Goal**: Persistence service and advanced indicators

- PersistenceService: Trade record, position snapshot
- AccountPool: Account margin pool with circuit breaker
- StrategyPool: Strategy fund pool with rebalancing
- VelocityPercentile: Velocity percentile indicator
- PriceDeviation: Price deviation indicator
- Momentum: Momentum indicator
- BigCycleCalculator: Daily cycle indicators (TR Ratio, position, PineColor)

**Status**: COMPLETE

---

## v1.0: Integration & Testing (Next)

**Goal**: Full system integration and compilation verification

- Compile and verify all modules
- Adjust indicator calculations based on Python code
- Integration testing with mock data

**Status**: PENDING
