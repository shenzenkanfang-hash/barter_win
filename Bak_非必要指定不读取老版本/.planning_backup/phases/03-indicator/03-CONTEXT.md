# Phase 03: Indicator Layer - Context

**Phase:** 3
**Goal:** Core indicators with O(1) incremental calculation

## Phase Boundary

Indicator layer provides incremental calculation for trading indicators. All indicators update in O(1) time.

## Three Core Indicators

Per `docs/indicator-logic.md`:

### 1. TR (True Range)
- Purpose: Volatility + breakout filtering
- Structure: MonotonicQueue for HHV/LLV, anchor price
- Calculation: max(high-low, |high-anchor|, |low-anchor|)

### 2. Pine Color
- Purpose: Trend direction signal
- Components: EMA(12,26), EMA(10,20), RSI(14)
- Colors: PureGreen, LightGreen, PureRed, LightRed, Purple

### 3. Price Position
- Purpose: Cycle extreme detection
- Formula: (close - low) / (high - low)
- Tick-driven for precision

## EMA Incremental Formula

```
EMA_new = price * k + EMA_old * (1 - k)
where k = 2 / (period + 1)
```

## RSI Incremental

```
RSI = 100 - (100 / (1 + RS))
RS = avg_gain / avg_loss
```

## References

- `docs/indicator-logic.md`
- `docs/2026-03-20-trading-system-rust-design.md` (section 六)

---
*Phase: 03-indicator*
