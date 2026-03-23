# Plan: Phase 01 - Foundation

**Phase:** 1
**Goal:** Project scaffold and core infrastructure
**Wave:** 1 of 2

## Wave 1: Project Scaffold

### Task 1.1: Create Workspace Structure

**read_first:** `docs/2026-03-20-trading-system-rust-design.md` (module structure)

**acceptance_criteria:**
- [ ] `Cargo.toml` exists at root with workspace definition
- [ ] `crates/` directory with: market/, indicator/, strategy/, engine/, account/
- [ ] Each crate has `src/lib.rs`
- [ ] `src/main.rs` exists

**action:**
```
trading-system/
├── Cargo.toml           # workspace with members = ["crates/*"]
├── src/main.rs
└── crates/
    ├── market/lib.rs    # pub mod market;
    ├── indicator/lib.rs
    ├── strategy/lib.rs
    ├── engine/lib.rs
    └── account/lib.rs
```

---

### Task 1.2: Configure Dependencies

**read_first:** `docs/2026-03-20-trading-system-rust-design.md` (dependencies)

**acceptance_criteria:**
- [ ] Root `Cargo.toml` has [workspace.dependencies]
- [ ] Dependencies: parking_lot, rust_decimal, thiserror, tracing, chrono, tokio

**action:**
```toml
[workspace.dependencies]
parking_lot = "0.12"
rust_decimal = "1.36"
thiserror = "2.0"
tracing = "0.1"
chrono = "0.4"
tokio = { version = "1", features = ["full"] }

[workspace]
members = ["crates/*"]
```

---

### Task 1.3: Configure Code Standards

**read_first:** `docs/2026-03-20-trading-system-rust-design.md` (conventions)

**acceptance_criteria:**
- [ ] `rustfmt.toml` exists with edition = "2024"
- [ ] `#![forbid(unsafe_code)]` in each lib.rs

**action:**
Create `rustfmt.toml`:
```toml
edition = "2024"
imports_granularity = "crate"
```

---

## Wave 2: Core Infrastructure

### Task 2.1: Define Error Types

**read_first:** `docs/2026-03-20-trading-system-rust-design.md` (error handling)

**acceptance_criteria:**
- [ ] `crates/account/src/error.rs` defines TradingError
- [ ] Derives Debug, Error
- [ ] Variants: Market, Order, Position, Fund, Config

**action:**
```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TradingError {
    #[error("market error: {0}")]
    Market(String),

    #[error("order error: {0}")]
    Order(String),

    #[error("position error: {0}")]
    Position(String),

    #[error("fund error: {0}")]
    Fund(String),

    #[error("config error: {0}")]
    Config(String),
}
```

---

### Task 2.2: Define Core Data Structures

**read_first:** `docs/2026-03-20-trading-system-rust-design.md` (data structures)

**acceptance_criteria:**
- [ ] `Order` struct with all fields using Decimal
- [ ] `Position` struct
- [ ] `FundPool` struct
- [ ] `Side` enum: Buy, Sell
- [ ] `OrderType` enum: Market, Limit
- [ ] `OrderStatus` enum: Pending, Filled, Cancelled

**action:**
```rust
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side { Buy, Sell }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderType { Market, Limit }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderStatus { Pending, Filled, Cancelled }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub order_id: u64,
    pub symbol: String,
    pub side: Side,
    pub order_type: OrderType,
    pub price: Decimal,
    pub qty: Decimal,
    pub status: OrderStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub symbol: String,
    pub side: Side,
    pub qty: Decimal,
    pub entry_price: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FundPool {
    pub total_equity: Decimal,
    pub available: Decimal,
    pub positions_value: Decimal,
}
```

---

### Task 2.3: Setup Logging

**read_first:** `docs/architecture-reference.md` (logging)

**acceptance_criteria:**
- [ ] tracing configured in main.rs
- [ ] JSON output format
- [ ] Info log on startup

**action:**
```rust
use tracing_subscriber::fmt::format::FmtSpan;

fn main() {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_span_events(FmtSpan::CLOSE)
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .json()
        .init();

    tracing::info!("Trading system started");
}
```

---

## Verification

**must_haves:**
1. `cargo build --workspace` succeeds
2. All crates build independently
3. No unsafe code
4. Error types follow thiserror pattern
5. Order/Position/Fund use Decimal

---
*Plan: 2026-03-20*
