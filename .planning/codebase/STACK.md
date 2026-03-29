# Technology Stack

## Core Language

- **Language**: Rust (Edition 2024)
- **Workspace**: Multi-crate under `crates/`

## Runtime & Async

- **Runtime**: Tokio — async I/O, multi-threaded task scheduling
- **Sync Primitives**: `parking_lot::RwLock` (faster than std)

## Financial Calculations

- **Decimal**: `rust_decimal` — avoids floating-point precision issues
- **Decimal Macros**: `rust_decimal_macros::dec!` for literal creation

## Time Handling

- **Library**: `chrono` with `DateTime<Utc>`

## Error Handling

- **Library**: `thiserror` — structured error types with `#[derive(Error)]`

## Logging & Tracing

- **Library**: `tracing` — structured logging
- **Subscriber**: `tracing-subscriber` for test output capture

## Serialization

- **Library**: `serde` with `Serialize`/`Deserialize` derives

## Database

- **SQLite**: `rusqlite 0.32` (bundled)
- **Used for**: Trading event persistence, symbol rules storage

## External Data Sources

- **Binance**: Primary exchange
  - REST API: Market data, account info, order management
  - WebSocket: Real-time trade streams, Kline streams

## Storage Paths

| Platform | Path |
|----------|------|
| Windows (RAM Disk) | `E:/shm/backup/` |
| Windows (HDD) | `E:/backup/trading_events.db` |
| Linux (RAM Disk) | `/dev/shm/backup/` |
| Linux (HDD) | `data/trading_events.db` |

## Key Dependencies

```toml
tokio = { version = "1", features = ["full"] }
parking_lot = "0.12"
rust_decimal = "1.33"
chrono = { version = "0.4", features = ["serde"] }
thiserror = "1.0"
tracing = "0.1"
serde = { version = "1.0", features = ["derive"] }
rusqlite = { version = "0.32", features = ["bundled"] }
```
