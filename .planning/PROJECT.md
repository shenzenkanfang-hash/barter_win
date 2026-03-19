# Trading System Rust

## What This Is

High-performance quantitative trading system in Rust. Supports multi-period strategies (daily + minute + tick) with dynamic mode switching based on market volatility.

## Core Value

**Reliable tick-level order execution** — Every design decision prioritizes the ability to place orders accurately at high frequency without data races or lock contention.

## Requirements

### Active

- [ ] v0.1: Foundation — workspace, error types, core data structures, logging
- [ ] v0.2: Market data — WebSocket, K-line synthesis
- [ ] v0.3: Indicator layer — EMA, Pine color, TR/Price position
- [ ] v0.4: Strategy layer — Strategy trait, 3 strategy types
- [ ] v0.5: Engine layer — Core engine, risk check, order execution

### Out of Scope

- Backtesting — Focus on live trading first
- Multiple exchanges — Single exchange to start
- Machine learning — Manual strategy only for v1

## Constraints

- **Tech stack**: Rust stable, Tokio async
- **Lock-free hot path**: Tick processing must never block on locks
- **Financial precision**: Use rust_decimal for all calculations

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Three-tier indicators | Simplified from complex alternatives | ✓ Good |
| No-lock tick processing | High-frequency requirement | ✓ Good |
| Strategy-private positions | Avoid contention | ✓ Good |

---
*Last updated: 2026-03-20*
