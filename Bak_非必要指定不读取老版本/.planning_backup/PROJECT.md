# Trading System Rust

## What This Is

High-performance quantitative trading system in Rust. Supports multi-period strategies (daily + minute + tick) with dynamic mode switching based on market volatility.

## Core Value

**Reliable tick-level order execution** — Every design decision prioritizes the ability to place orders accurately at high frequency without data races or lock contention.

## Requirements

### Validated

- ✓ v0.1: Foundation — workspace, error types, core data structures, logging — v1.0
- ✓ v0.2: Market data — WebSocket, K-line synthesis — v1.0
- ✓ v0.3: Indicator layer — EMA, Pine color, TR/Price position — v1.0
- ✓ v0.4: Strategy layer — Strategy trait, 3 strategy types — v1.0
- ✓ v0.5: Engine layer — Core engine, risk check, order execution — v1.0
- ✓ v0.6-v0.10: Enhancement phases — v1.0

### Active

- [ ] v1.1: MockBinanceGateway + Signal Synthesis Layer
  - MockBinanceGateway: 模拟币安账户/持仓/订单/保证金，风控与币安一致
  - Signal Synthesis Layer: tr_ratio<1退出 + 日线趋势平仓
  - Complete test coverage
  - Indicator comparison (Rust vs Python)
  - Phase: `.planning/phases/08-v1.1-mock-binance/`

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
*Last updated: 2026-03-20 after v1.0 milestone*
