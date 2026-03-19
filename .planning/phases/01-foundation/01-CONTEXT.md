# Phase 01: Foundation - Context

**Gathered:** 2026-03-20
**Status:** Ready for planning

## Phase Boundary

Establish project scaffold and core infrastructure that all subsequent phases depend on.

## Decisions

### Workspace Structure
- Workspace-based Rust project
- Crates: market/, indicator/, strategy/, engine/, account/
- Main binary entry point

### Dependencies
- parking_lot: synchronization primitives
- rust_decimal: financial precision
- thiserror: error derivation
- tracing: structured logging
- chrono: time handling

### Error Handling
- All errors use thiserror::Error
- No panic!() for error handling

### Code Standards
- #![forbid(unsafe_code)]
- Clippy strict warnings
- rustfmt formatting

## Specific Ideas

### Workspace
```
trading-system/
├── Cargo.toml           # workspace root
├── src/main.rs         # binary
└── crates/
    ├── market/
    ├── indicator/
    ├── strategy/
    ├── engine/
    └── account/
```

### Error Type
```rust
#[derive(Debug, Error)]
pub enum TradingError {
    #[error("market error: {0}")]
    Market(String),
    #[error("order error: {0}")]
    Order(String),
    // ...
}
```

### Core Structs
- Order: order_id, symbol, side, order_type, price, qty, status
- Position: symbol, side, qty, entry_price
- FundPool: total_equity, available, positions_value

---
*Phase: 01-foundation*
