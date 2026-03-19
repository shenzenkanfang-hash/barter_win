# Trading System Rust

## What This Is

High-performance quantitative trading system rebuilt in Rust, inspired by Barter-rs architecture. Supports multi-period strategies (daily + minute + tick) with dynamic mode switching based on market volatility.

Core goals:
- Multi-period strategy parallel execution
- Dynamic high/low frequency mode switching
- Hybrid position mode: shared fund pool, strategy-private positions
- Incremental indicator calculation for tick-level performance

## Core Value

**Reliable tick-level order execution** — Every design decision prioritizes the ability to place orders accurately at high frequency without data races or lock contention.

## Requirements

### Validated

(None yet — initial build phase)

### Active

- [ ] Project scaffold: workspace structure, error types, core data structures
- [ ] Market data layer: WebSocket connector, K-line synthesis
- [ ] Indicator layer: EMA, Pine color, TR/Price position
- [ ] Strategy layer: Daily/Minute/Tick strategy traits and implementations
- [ ] Engine layer: Core engine, risk pre-check, order execution

### Out of Scope

- Backtesting framework — Focus on live trading first
- Multiple exchange support — Single exchange to start
- Machine learning integration — Manual strategy only for v1

## Context

- **Source**: Migrating from Go quantitative trading system
- **Architecture reference**: Barter-rs (ztNozdormu/barter-rs)
- **Key design**: Three-tier indicators (TR/Pine/Price position) per brainstorming 2026-03-20

## Constraints

- **Tech stack**: Rust stable, Tokio async runtime
- **Lock-free hot path**: Tick processing must never block on locks
- **Financial precision**: Use rust_decimal for all calculations

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Three-tier indicators (TR/Pine/Price position) | Simplified from Barter-rs complex indicators | ✓ Good |
| No-lock tick processing | High-frequency performance requirement | ✓ Good |
| Strategy-private positions | Avoid contention between strategies | ✓ Good |
| Lock-free pre-check, lock-order execution | Minimize lock hold time | ✓ Good |

---
*Last updated: 2026-03-20 after initial GSD setup*
