# Technical Concerns

## Fatal Issues

### F-001: g_test Module Compilation Failure
- **Issue**: 544 compilation errors - incorrect API paths and types
- **Root Cause**:
  - Tries to import `f_engine::order::gateway::ExchangeGateway` (path doesn't exist)
  - Uses non-existent types: Strategy, StrategyKLine, StrategyState, TradingSignal
  - Missing imports for Arc, Utc, SignalType, Mode, SymbolState
- **Impact**: Global test framework completely unusable
- **Status**: Unfixed
- **Resolution**: Requires full refactor to match new module structure

## Serious Issues

### S-001: Clock Time Precision Test Failure
- **Location**: `crates/b_data_source/src/engine/clock.rs`
- **Issue**: `test_historical_clock_update` fails - time precision assertion
- **Status**: Unfixed

### S-002: f_engine Test Database Path Error
- **Location**: `crates/f_engine/src/strategy/trader_manager.rs`
- **Issue**: Test uses hardcoded database path, fails in test environment
- **Status**: Unfixed

### S-003: mock_ws_handshake Example Compilation Failure
- **Location**: `crates/b_data_source/examples/mock_ws_handshake.rs`
- **Issue**: Import path errors
- **Status**: Unfixed

## Technical Debt

### N-001: collapsible_if Warnings (~70)
- **Issue**: Nested if statements that could be combined with &&
- **Priority**: Low

### N-002: await_holding_lock Warnings (~20)
- **Issue**: Potential deadlock - async operations while holding lock
- **Locations**:
  - `crates/a_common/src/api/binance_api.rs` (rate_limiter.lock().acquire().await)
  - `crates/b_data_source/src/recovery.rs` (redis.lock().await)
- **Priority**: Medium
- **Resolution**: Consider tokio::sync::Mutex for async context

### N-003: dead_code Warnings
- **Locations**:
  - `crates/c_data_process/src/types.rs:71`
  - `crates/c_data_process/src/pine_indicator_full.rs`
  - `crates/x_data/src/account/pool.rs:12`
  - `crates/a_common/src/ws/binance_ws.rs:24,261`
- **Priority**: Low

### N-004: Deprecated API Usage (11 warnings)
- **Locations**:
  - `crates/c_data_process/src/processor.rs:566` - start_loop()
  - `crates/b_data_source/src/api/data_feeder.rs` - old channel APIs
- **Priority**: Medium

## Architecture Concerns

### 1. Lock Contention Risk
- **Issue**: Multiple tick streams share parking_lot::RwLock
- **Mitigation**: Hot paths lock-free, lock only for account/order operations

### 2. Kline Update Race Condition
- **Location**: `KlinePersistence` in `b_data_source`
- **Issue**: Incremental K-line updates may race with persistence
- **Status**: Needs review

### 3. Symbol Rules Cache
- **Issue**: Rules fetched from API on startup, no refresh mechanism
- **Impact**: Stale trading rules if Binance updates
- **Resolution**: Add background refresh task

### 4. Memory Backup Integrity
- **Issue**: In-memory backup on E:/shm/ is volatile
- **Mitigation**: SQLite persistence as backup (dual-layer)

### 5. WebSocket Reconnection
- **Issue**: No explicit reconnection logic
- **Impact**: Stream interruption on network issues
- **Resolution**: Add exponential backoff reconnection

## Security

- **API Keys**: Stored in environment/config, not hardcoded
- **No SQL injection**: Parameterized queries via rusqlite
- **No unsafe code**: `#![forbid(unsafe_code)]` enforced on all lib.rs

## Performance Considerations

- **Tick processing**: O(1) incremental updates
- **Indicator recalculation**: Avoided via incremental EMA
- **Memory**: RAM disk for high-frequency backup
- **Disk I/O**: Async SQLite writes, non-blocking

## Test Coverage Gaps

| Module    | Interfaces | Tested | Coverage |
|-----------|------------|--------|----------|
| a_common  | 15+       | 15+    | 100%     |
| b_data_source | 20+   | 15+    | 75%      |
| b_data_mock | 15+      | 2      | 13%      |
| c_data_process | 10+  | 10+    | 100%     |
| d_checktable | 8+      | 8      | 100%     |
| e_risk_monitor | 15+ | 15+    | 100%     |
| f_engine  | 10+       | 0      | 0%       |
| g_test    | -         | 0      | 0%       |
