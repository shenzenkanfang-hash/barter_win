# Testing

## Test Locations

| Location | Pattern | Purpose |
|----------|---------|---------|
| `crates/*/tests/dt_*.rs` | `dt_` prefix | Integration tests, checktable tests |
| `crates/b_data_mock/tests/test_*.rs` | `test_` prefix | b_data_mock unit tests |
| `crates/*/src/*.rs` | inline `mod tests { }` | Unit tests alongside source |

## Test File Naming

- **Integration tests**: `dt_NNN_name_test.rs` (e.g., `dt_001_checktable_test.rs`)
- **Unit tests in b_data_mock**: `test_name.rs` (e.g., `test_mock_gateway.rs`)
- **Coverage tests**: `test_bm_p0_coverage.rs`

## Test Macros

```rust
// Synchronous tests
#[test]
fn test_my_function() {
    assert_eq!(result, expected);
}

// Async tests
#[tokio::test]
async fn test_async_operation() {
    let result = async_function().await;
    assert!(result.is_ok());
}
```

## Test Utilities

| Utility | Crate | Purpose |
|---------|-------|---------|
| Decimal literals | `rust_decimal_macros::dec!` | Create Decimal values |
| Temporary files | `tempfile` | I/O tests, persistence tests |
| Log capture | `tracing-subscriber` | Capture and verify logs |
| Mock implementations | `b_data_mock` | MockApiGateway, MockAccount |

## MockBinanceGateway Pattern

Located in `crates/b_data_mock/src/api/mock_gateway.rs`:

```rust
use b_data_mock::{MockApiGateway, MockConfig};
use rust_decimal_macros::dec;

#[test]
fn test_gateway_place_order() {
    let config = MockConfig::default();
    let gateway = MockApiGateway::new(dec!(10000.0), config);

    gateway.update_price("BTCUSDT", dec!(50000.0));

    let result = gateway.place_order("BTCUSDT", Side::Buy, dec!(0.1), None);
    assert!(result.is_ok());
}
```

## b_data_mock Test Structure

```
crates/b_data_mock/
├── src/
│   ├── api/
│   │   ├── mock_gateway.rs      # MockApiGateway
│   │   ├── mock_account.rs      # Mock Account
│   │   └── mock_config.rs       # MockConfig
│   └── ws/
│       └── kline_generator.rs   # KlineStreamGenerator
└── tests/
    ├── test_mock_gateway.rs     # Gateway tests
    ├── test_store.rs            # Store tests
    ├── test_models.rs           # Model tests
    └── test_bm_p0_coverage.rs   # P0 coverage tests
```

## d_checktable Test Structure

```
crates/d_checktable/
└── tests/
    ├── dt_001_checktable_test.rs
    ├── dt_002_003_trader_executor_test.rs
    ├── dt_004_quantity_calculator_test.rs
    ├── dt_006_007_signal_status_test.rs
    └── dt_011_check_chain_context_test.rs
```

## Test Execution

```bash
# All tests
cargo test --all

# Specific crate
cargo test -p b_data_mock
cargo test -p d_checktable

# Integration tests only
cargo test -p g_test

# With output
cargo test --all -- --nocapture

# Specific test
cargo test test_checktable_new
```

## Coverage Priority

| Priority | Module | Focus |
|----------|--------|-------|
| P0 | b_data_mock | MockApiGateway, MockAccount, KlineGenerator |
| P1 | d_checktable | CheckTable, Signal validation, Quantity calculation |
| P2 | c_data_process | Indicator calculations, PineColor |
| P3 | e_risk_monitor | Risk checks, position limits |

## Inline Test Pattern

```rust
// In source file (e.g., crates/d_checktable/src/check_table.rs)

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_checktable_new() {
        let table = CheckTable::new();
        assert_eq!(table.current_round_id(), 0);
    }
}
```

## Test Data Helpers

```rust
fn create_test_entry(symbol: &str, strategy_id: &str, period: &str) -> CheckEntry {
    CheckEntry {
        symbol: symbol.to_string(),
        strategy_id: strategy_id.to_string(),
        period: period.to_string(),
        ema_signal: Signal::LongExit,
        rsi_value: dec!(50),
        pine_color: PineColor::Neutral,
        price_position: dec!(50),
        final_signal: Signal::LongExit,
        target_price: dec!(50000),
        quantity: dec!(0.01),
        risk_flag: false,
        timestamp: Utc::now(),
        round_id: 1,
        is_high_freq: true,
    }
}
```
