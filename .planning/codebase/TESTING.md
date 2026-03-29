# Testing

## Test Locations

| Location | Purpose |
|----------|---------|
| `tests/` | Integration tests with `dt_` prefix naming |
| `crates/b_data_source/tests/` | Mock tests with `test_` prefix |
| `crates/*/src.rs` | Inline `mod tests { }` within source |

## Test Categories

### Unit Tests
```rust
#[test]
fn test_my_function() {
    // ...
}
```

### Async Tests
```rust
#[tokio::test]
async fn test_async_operation() {
    // ...
}
```

### Integration Tests
- **Location**: `g_test` crate for cross-crate integration testing
- **Purpose**: Test interactions between multiple modules

## Mock Pattern

The `b_data_mock` crate mirrors `b_data_source` for testing:
- `MockBinanceGateway` — account, position, order, margin, risk mocks
- `SignalSynthesisLayer` — channel exit logic mock

## Test Utilities

| Utility | Purpose |
|---------|---------|
| `rust_decimal_macros::dec!` | Decimal literal creation |
| `tempfile` | Temporary files for I/O tests |
| `tracing-subscriber` | Log capture in tests |

## Coverage Focus

- **P0**: `b_data_mock` comprehensive tests
- **P1**: `d_checktable` detailed DT tests
- **Integration**: `g_test` crate for end-to-end scenarios

## Test Execution

```bash
# All tests
cargo test --all

# Specific crate
cargo test -p b_data_mock

# With output
cargo test --all -- --nocapture
```
