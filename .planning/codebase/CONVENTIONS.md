# Code Conventions

## Rust Edition & Workspace

- **Edition**: Rust 2024
- **Workspace Structure**: Multi-crate workspace under `crates/`
- **`#![forbid(unsafe_code)]`**: Mandatory at the top of every `lib.rs`

## Derive Macro Order

```rust
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
```

## Error Handling Pattern

Use `thiserror` for clear error type hierarchies:

```rust
#[derive(Debug, Clone, Eq, PartialEq, Error)]
pub enum MyError {
    #[error("描述: {0}")]
    MyVariant(String),
}
```

- **Forbidden**: No `panic!()` in production code — return `Result` instead
- **Locked paths**: Only lock for account operations; tick/indicator paths are lock-free

## Module Structure

```
crates/
├── a_common/           # Infrastructure: API/WS gateways (no business type dependency)
├── b_data_source/       # Business data: DataFeeder, K-line synthesis, Tick
├── c_data_process/      # Data processing: indicators, signal generation
├── d_risk_monitor/      # Risk control: risk, position management
├── e_strategy/          # Strategy: daily/minute/Tick strategies
└── engine/              # Engine: risk, order execution, mode switching

engine/src/
├── core/                # Core engine
├── risk/                # Risk management
├── order/               # Order execution
├── position/            # Position management
├── persistence/         # SQLite persistence
├── channel/             # Channel management
└── shared/              # Shared utilities
```

## Naming Conventions

| Element | Convention | Example |
|---------|------------|---------|
| Types | PascalCase | `struct AccountPool`, `enum MarketError` |
| Functions | snake_case | `fn fetch_klines()`, `fn check_risk()` |
| Modules | snake_case | `mod market_data`, `mod order_gateway` |
| Constants | SCREAMING_SNAKE_CASE | `MAX_POSITION_SIZE` |
| Files | snake_case | `account_pool.rs`, `risk_rechecker.rs` |

## Synchronization

- **Primary**: `parking_lot::RwLock` (faster than std)
- **Financial values**: `rust_decimal::Decimal` (avoid float)
- **Time**: `chrono::DateTime<Utc>`

## Code Patterns

- **Lock-free hot paths**: Tick receiving, indicator updates, strategy judgment
- **Lock only for**: Order submission and capital updates
- **Pre-check all risk conditions** outside locks
- **Incremental calculation** for EMA, SMA, MACD — no full recalculation

## Three-Layer Indicator System

1. **TR (True Range)**: Volatility breakout detection
2. **Pine Color**: Trend signals (MACD + EMA10/20 + RSI)
3. **Price Position**: Cycle extreme detection
