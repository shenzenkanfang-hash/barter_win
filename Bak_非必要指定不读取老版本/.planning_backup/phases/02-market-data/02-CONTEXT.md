# Phase 02: Market Data Layer - Context

**Phase:** 2
**Goal:** WebSocket connection and K-line synthesis

## Phase Boundary

Market data layer receives ticks from exchange via WebSocket, synthesizes K-lines incrementally.

## Architecture

```
WebSocket → Tick → K-line Synthesis (1m, 1d) → OrderBook
                ↓
         Channel → Strategy
```

## Key Components

### Tick
```rust
pub struct Tick {
    pub symbol: String,
    pub price: Decimal,
    pub qty: Decimal,
    pub timestamp: DateTime<Utc>,
}
```

### K-line
```rust
pub struct KLine {
    pub symbol: String,
    pub period: Period,        // 1m, 1d
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
    pub timestamp: DateTime<Utc>,
}
```

### Incremental Update
- O(1) update for current K-line
- Only current K-line changes, historical immutable
- VecDeque for fixed window

## OrderBook (Later Phase)

- BTreeMap for sorted bids/asks
- O(log N) update
- Price levels with quantity

## References

- `docs/2026-03-20-trading-system-rust-design.md` (section 十四/14.1)
- `docs/indicator-logic.md` (TR calculation)

---
*Phase: 02-market-data*
