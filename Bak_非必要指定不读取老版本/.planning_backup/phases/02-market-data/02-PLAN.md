# Plan: Phase 02 - Market Data Layer

**Phase:** 2
**Goal:** WebSocket connection and K-line synthesis
**Wave:** 1 of 2

## Wave 1: Core Types

### Task 2.1: Market Data Types

**read_first:** `docs/2026-03-20-trading-system-rust-design.md` (section 十, module structure)

**acceptance_criteria:**
- [ ] `Tick` struct with symbol, price, qty, timestamp
- [ ] `KLine` struct with OHLCV + period + timestamp
- [ ] `Period` enum: Minute(u8), Day
- [ ] All use rust_decimal::Decimal

**action:**
```rust
// crates/market/src/types.rs
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Period {
    Minute(u8),  // 1, 5, 15, 60, etc
    Day,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tick {
    pub symbol: String,
    pub price: Decimal,
    pub qty: Decimal,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KLine {
    pub symbol: String,
    pub period: Period,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
    pub timestamp: DateTime<Utc>,
}
```

---

### Task 2.2: K-line Synthesizer

**read_first:** `docs/2026-03-20-trading-system-rust-design.md` (section 十四/14.1)

**acceptance_criteria:**
- [ ] `KLineSynthesizer` struct
- [ ] `update(&mut self, tick: &Tick) -> Option<KLine>` method
- [ ] Incremental O(1) update
- [ ] Returns completed K-line when period closes

**action:**
```rust
// crates/market/src/kline.rs
use crate::types::{KLine, Period, Tick};
use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use std::collections::VecDeque;

pub struct KLineSynthesizer {
    pub symbol: String,
    pub period: Period,
    current: Option<KLine>,
}

impl KLineSynthesizer {
    pub fn new(symbol: String, period: Period) -> Self {
        Self { symbol, period, current: None }
    }

    pub fn update(&mut self, tick: &Tick) -> Option<KLine> {
        let kline_timestamp = self.period_start(tick.timestamp);

        match &mut self.current {
            Some(kline) if kline.timestamp == kline_timestamp => {
                // Update current K-line (O(1))
                kline.high = kline.high.max(tick.price);
                kline.low = kline.low.min(tick.price);
                kline.close = tick.price;
                kline.volume += tick.qty;
                None
            }
            Some(kline) => {
                // Period changed, return completed K-line and start new one
                let completed = kline.clone();
                self.current = Some(self.new_kline(tick, kline_timestamp));
                Some(completed)
            }
            None => {
                // First tick
                self.current = Some(self.new_kline(tick, kline_timestamp));
                None
            }
        }
    }

    fn period_start(&self, timestamp: DateTime<Utc>) -> DateTime<Utc> {
        match self.period {
            Period::Minute(m) => {
                let minutes = (timestamp.timestamp() / 60 / m as i64) * 60 * m as i64;
                DateTime::from_timestamp(minutes, 0).unwrap()
            }
            Period::Day => {
                let days = timestamp.date_naive().and_hms_opt(0, 0, 0).unwrap();
                DateTime::<Utc>::from_naive_utc_and_offset(days, Utc)
            }
        }
    }

    fn new_kline(&self, tick: &Tick, timestamp: DateTime<Utc>) -> KLine {
        KLine {
            symbol: self.symbol.clone(),
            period: self.period,
            open: tick.price,
            high: tick.price,
            low: tick.price,
            close: tick.price,
            volume: tick.qty,
            timestamp,
        }
    }
}
```

---

## Wave 2: WebSocket Framework

### Task 2.3: WebSocket Traits

**read_first:** `docs/2026-03-20-trading-system-rust-design.md` (section 四/1)

**acceptance_criteria:**
- [ ] `MarketConnector` trait with `subscribe(symbol)`, `unsubscribe(symbol)`
- [ ] `MarketStream` trait with `async fn next_tick() -> Option<Tick>`
- [ ] Uses Tokio async runtime

**action:**
```rust
// crates/market/src/websocket.rs
use crate::types::Tick;
use async_trait::async_trait;

#[async_trait]
pub trait MarketConnector: Send + Sync {
    async fn subscribe(&mut self, symbol: &str) -> Result<(), TradingError>;
    async fn unsubscribe(&mut self, symbol: &str) -> Result<(), TradingError>;
}

#[async_trait]
pub trait MarketStream: Send + Sync {
    async fn next_tick(&mut self) -> Option<Tick>;
}
```

### Task 2.4: Market Module Setup

**acceptance_criteria:**
- [ ] `crates/market/src/lib.rs` exports types, kline, websocket modules
- [ ] Update `crates/market/Cargo.toml` with dependencies

**action:**
Update `crates/market/src/lib.rs`:
```rust
#![forbid(unsafe_code)]

pub mod types;
pub mod kline;
pub mod websocket;
pub mod error;

pub use types::{Period, Tick, KLine};
pub use kline::KLineSynthesizer;
pub use websocket::{MarketConnector, MarketStream};
pub use error::MarketError;
```

Update `crates/market/Cargo.toml`:
```toml
[package]
name = "market"
version = "0.1.0"
edition = "2024"

[dependencies]
rust_decimal = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
chrono = { workspace = true }
tokio = { workspace = true }
async-trait = "0.1"
```

---
*Plan: Phase 02 - Market Data Layer*
