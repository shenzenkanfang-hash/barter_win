# Code Conventions

## Safety

- **Mandatory**: `#![forbid(unsafe_code)]` at the top of every `lib.rs`
- **Forbidden**: No `panic!()` in production code — all errors return `Result`

## Derive Macro Order

```rust
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
```

Error enums use `Error` instead of `Serialize, Deserialize`:

```rust
#[derive(Debug, Clone, Eq, PartialEq, Error)]
pub enum MyError {
    #[error("描述: {0}")]
    MyVariant(String),
}
```

## Error Handling

Use `thiserror` for clear error type hierarchies:

```rust
use thiserror::Error;

#[derive(Debug, Clone, Eq, PartialEq, Error)]
pub enum MarketError {
    #[error("WebSocket连接失败: {0}")]
    WebSocketConnectionFailed(String),

    #[error("序列化错误: {0}")]
    SerializeError(String),
}
```

## Synchronization

- **Primary**: `parking_lot::RwLock` (faster than std::sync::RwLock)
- **Lock-free hot paths**: Tick receiving, indicator updates, strategy judgment
- **Lock only for**: Order submission and capital updates
- **Pre-check all risk conditions** outside locks

## Financial Values

- **Mandatory**: `rust_decimal::Decimal` for all financial calculations
- **Macro**: Use `rust_decimal_macros::dec!` for literal decimals
- **Forbidden**: No floating-point types (`f32`, `f64`) for financial values

```rust
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

let price: Decimal = dec!(50000.0);
let quantity = dec!(0.01);
```

## Naming Conventions

| Element | Convention | Example |
|---------|------------|---------|
| Types | PascalCase | `struct AccountPool`, `enum Signal` |
| Functions | snake_case | `fn fetch_klines()`, `fn check_risk()` |
| Modules | snake_case | `mod market_data`, `mod order_gateway` |
| Constants | SCREAMING_SNAKE_CASE | `MAX_POSITION_SIZE` |
| Files | snake_case | `account_pool.rs`, `risk_rechecker.rs` |

## Incremental Calculation

All indicators must use incremental calculation, not full recalculation:

- **EMA**: Update in place using `ema = price * k + ema * (1 - k)` where `k = 2/(period + 1)`
- **SMA**: Maintain running sum with add/remove
- **MACD**: Update EMA12, EMA26, then compute DIF/DEA
- **RSI**: Maintain average gain/loss with smoothing
- **K-line**: Update current candle incrementally, create new only on close

## Three-Layer Indicator System

1. **TR (True Range)**: Volatility breakout detection
2. **Pine Color**: Trend signals (MACD + EMA10/20 + RSI)
3. **Price Position**: Cycle extreme detection

## Module Structure

```
crates/
├── a_common/           # Infrastructure: API/WS gateways, config, errors
├── b_data_mock/        # Mock implementations for testing
├── b_data_source/      # Business data: DataFeeder, K-line synthesis
├── c_data_process/     # Data processing: indicators, signal generation
├── d_checktable/       # Check table: signal validation, quantity calculation
├── e_risk_monitor/     # Risk control: position management, risk checks
├── f_engine/            # Engine: orchestration, mode switching
└── g_test/             # Integration tests

engine/src/ (legacy, now in f_engine)
├── core/                # Core engine
├── risk/                # Risk management
├── order/               # Order execution
├── position/            # Position management
├── persistence/         # SQLite persistence
├── channel/             # Channel management
└── shared/              # Shared utilities
```

## Layer Dependencies

```
a_common (no dependencies)
    |
    v
b_data_source (depends on a_common)
    |
    v
c_data_process (depends on b_data_source)
    |
    v
d_checktable (depends on c_data_process)
    |
    v
e_risk_monitor (depends on d_checktable)
```
