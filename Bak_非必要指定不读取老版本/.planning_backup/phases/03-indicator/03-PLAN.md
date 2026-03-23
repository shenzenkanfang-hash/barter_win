# Plan: Phase 03 - Indicator Layer

**Phase:** 3
**Goal:** Core indicators with O(1) incremental calculation
**Wave:** 1 of 2

## Wave 1: EMA and RSI

### Task 3.1: EMA Indicator

**read_first:** `docs/indicator-logic.md`

**acceptance_criteria:**
- [ ] `EMA` struct with period, value, k coefficient
- [ ] `calculate(&mut self, price: Decimal) -> Decimal` O(1) update
- [ ] Uses rust_decimal::Decimal

**action:**
```rust
// crates/indicator/src/ema.rs
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

#[derive(Debug, Clone)]
pub struct EMA {
    pub period: u32,
    pub value: Decimal,
    k: Decimal,
}

impl EMA {
    pub fn new(period: u32) -> Self {
        let k = dec!(2) / (Decimal::from(period) + dec!(1));
        Self { period, value: Decimal::ZERO, k }
    }

    pub fn calculate(&mut self, price: Decimal) -> Decimal {
        if self.value.is_zero() {
            self.value = price;
        } else {
            self.value = price * self.k + self.value * (dec!(1) - self.k);
        }
        self.value
    }
}
```

---

### Task 3.2: RSI Indicator

**read_first:** `docs/indicator-logic.md`

**acceptance_criteria:**
- [ ] `RSI` struct with period, avg_gain, avg_loss
- [ ] `calculate(&mut self, price: Decimal, prev_price: Decimal) -> Decimal`
- [ ] Returns RSI value (0-100)

**action:**
```rust
// crates/indicator/src/rsi.rs
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

#[derive(Debug, Clone)]
pub struct RSI {
    pub period: u32,
    avg_gain: Decimal,
    avg_loss: Decimal,
    last_price: Decimal,
}

impl RSI {
    pub fn new(period: u32) -> Self {
        Self {
            period,
            avg_gain: Decimal::ZERO,
            avg_loss: Decimal::ZERO,
            last_price: Decimal::ZERO,
        }
    }

    pub fn calculate(&mut self, price: Decimal) -> Decimal {
        if self.last_price.is_zero() {
            self.last_price = price;
            return Decimal::ZERO;
        }

        let change = price - self.last_price;
        self.last_price = price;

        let gain = if change > Decimal::ZERO { change } else { -change };
        let loss = if change < Decimal::ZERO { -change } else { Decimal::ZERO };

        // Initial average
        if self.avg_loss.is_zero() {
            self.avg_gain = gain;
            self.avg_loss = loss;
        } else {
            self.avg_gain = (self.avg_gain * Decimal::from(self.period - 1) + gain) / Decimal::from(self.period);
            self.avg_loss = (self.avg_loss * Decimal::from(self.period - 1) + loss) / Decimal::from(self.period);
        }

        if self.avg_loss.is_zero() {
            return dec!(100);
        }

        let rs = self.avg_gain / self.avg_loss;
        dec!(100) - (dec!(100) / (dec!(1) + rs))
    }
}
```

---

## Wave 2: Pine Color and Price Position

### Task 3.3: Pine Color

**read_first:** `docs/indicator-logic.md`

**acceptance_criteria:**
- [ ] `PineColor` enum: PureGreen, LightGreen, PureRed, LightRed, Purple
- [ ] `detect(macd, signal, hist, rsi) -> PineColor`
- [ ] MACD conditions per design doc

**action:**
```rust
// crates/indicator/src/pine_color.rs
use rust_decimal::Decimal;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PineColor {
    PureGreen,   // 强势多头
    LightGreen,  // 弱势多头
    PureRed,     // 强势空头
    LightRed,    // 弱势空头
    Purple,      // 极值区域
}

pub struct PineColorDetector;

impl PineColorDetector {
    pub fn detect(macd: Decimal, signal: Decimal, rsi: Decimal) -> PineColor {
        // Purple: RSI extreme
        if rsi >= dec!(70) || rsi <= dec!(30) {
            return PineColor::Purple;
        }

        // Based on MACD and Signal relationship
        if macd >= signal && macd >= Decimal::ZERO {
            PineColor::PureGreen
        } else if macd <= signal && macd >= Decimal::ZERO {
            PineColor::LightGreen
        } else if macd <= signal && macd <= Decimal::ZERO {
            PineColor::PureRed
        } else {
            PineColor::LightRed
        }
    }
}
```

---

### Task 3.4: Price Position

**read_first:** `docs/indicator-logic.md`

**acceptance_criteria:**
- [ ] `PricePosition` struct with period window
- [ ] `calculate(close, high, low) -> Decimal` (0-1 range)
- [ ] Tick-driven

**action:**
```rust
// crates/indicator/src/price_position.rs
use rust_decimal::Decimal;

#[derive(Debug, Clone)]
pub struct PricePosition {
    period: usize,
}

impl PricePosition {
    pub fn new(period: usize) -> Self {
        Self { period }
    }

    pub fn calculate(&self, close: Decimal, high: Decimal, low: Decimal) -> Decimal {
        if high == low {
            return Decimal::ZERO;
        }
        (close - low) / (high - low)
    }
}
```

---

### Task 3.5: Indicator Module Setup

**acceptance_criteria:**
- [ ] `crates/indicator/src/lib.rs` exports all indicators
- [ ] `Cargo.toml` with dependencies

**action:**
```rust
// crates/indicator/src/lib.rs
#![forbid(unsafe_code)]

pub mod ema;
pub mod rsi;
pub mod pine_color;
pub mod price_position;
pub mod error;

pub use ema::EMA;
pub use rsi::RSI;
pub use pine_color::{PineColor, PineColorDetector};
pub use price_position::PricePosition;
pub use error::IndicatorError;
```

```toml
# crates/indicator/Cargo.toml
[package]
name = "indicator"
version = "0.1.0"
edition = "2024"

[dependencies]
rust_decimal = { workspace = true }
serde = { workspace = true }
```

---
*Plan: Phase 03 - Indicator Layer*
