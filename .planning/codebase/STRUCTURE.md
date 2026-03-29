# Directory Structure

## Workspace Root

```
barter-rs-main/
├── Cargo.toml           # Workspace manifest
├── Cargo.lock
├── src/
│   └── main.rs          # Entry point, tracing init
├── crates/
│   ├── a_common/        # Infrastructure (API/WS gateways)
│   ├── b_data_source/   # Business data layer
│   ├── b_data_mock/     # Mock implementations for testing
│   ├── c_data_process/  # Data processing (indicators)
│   ├── d_checktable/    # Check table tests
│   ├── d_risk_monitor/  # Risk monitoring
│   ├── e_strategy/      # Strategy implementations
│   ├── f_engine/        # Engine (see below)
│   └── g_test/          # Integration tests
├── .planning/           # GSD planning docs
├── docs/                # Design documents
└── tests/               # Integration tests
```

## Engine Subdirectory

```
f_engine/src/
├── core/               # engine.rs, pipeline.rs, pipeline_form.rs
├── risk/               # risk.rs, risk_rechecker.rs, order_check.rs, thresholds.rs, minute_risk.rs
├── order/              # order.rs, gateway.rs, mock_binance_gateway.rs
├── position/            # position_manager.rs, position_exclusion.rs
├── persistence/         # sqlite_persistence.rs, memory_backup.rs, disaster_recovery.rs, persistence.rs
├── channel/            # channel.rs, mode.rs
└── shared/             # account_pool.rs, check_table.rs, pnl_manager.rs, symbol_rules.rs
```

## Key Files

| File | Purpose |
|------|---------|
| `src/main.rs` | Entry point, tracing initialization |
| `crates/a_common/api/binance_api_gateway.rs` | REST API client |
| `crates/a_common/ws/binance_ws_connector.rs` | WebSocket client |
| `crates/b_data_source/data_feeder.rs` | Main data feed orchestrator |
| `crates/c_data_process/pine_indicator_full.rs` | Full Pine v5 indicators |
| `crates/f_engine/core/engine.rs` | Core trading engine |
| `crates/f_engine/persistence/sqlite_persistence.rs` | SQLite persistence |

## Naming Conventions

- **Crates**: `snake_case` (e.g., `b_data_source`)
- **Modules**: `snake_case`
- **Files**: `snake_case.rs`
- **Types**: `PascalCase`
- **Functions**: `snake_case()`
- **Constants**: `SCREAMING_SNAKE_CASE`
