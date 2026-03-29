# Technical Concerns

## High Priority

### 1. Lock Contention Risk
- **Issue**: Multiple tick streams may contend for `parking_lot::RwLock`
- **Mitigation**: Hot paths (tick/indicator/strategy) are lock-free
- **Locked only**: Account operations, order submission

### 2. Decimal Precision in Aggregation
- **Issue**: Accumulating decimal errors over many calculations
- **Mitigation**: Use `rust_decimal` throughout; avoid float conversions

### 3. WebSocket Reconnection
- **Issue**: No explicit reconnection logic in current WS connector
- **Impact**: Stream interruption on network issues
- **Status**: Needs verification

## Medium Priority

### 4. Kline Update Race Condition
- **Issue**: Incremental K-line updates may race with persistence
- **Location**: `KlinePersistence` in `b_data_source`
- **Status**: Needs review

### 5. Symbol Rules Cache
- **Issue**: Rules fetched from API on startup, no refresh
- **Impact**: Stale trading rules if Binance updates
- **Status**: Periodic refresh needed

### 6. Memory Backup Integrity
- **Issue**: In-memory backup on `E:/shm/` is volatile
- **Impact**: Data loss on crash/power failure
- **Mitigation**: SQLite persistence as backup

## Known Issues

### Windows Path Handling
- **Issue**: Backslash vs forward slash in paths
- **Status**: Platform detection added; verify all paths

### Test Coverage
- **Gap**: `f_engine` has limited unit tests
- **Focus area**: `core/`, `risk/` modules

## Security

- **API Keys**: Stored in environment/config, not hardcoded
- **No SQL injection**: Using parameterized queries via rusqlite
- **No unsafe code**: `#![forbid(unsafe_code)]` enforced

## Performance Considerations

- **Tick processing**: O(1) incremental updates
- **Indicator recalculation**: Avoided via incremental EMA
- **Memory**: RAM disk for high-frequency backup
- **Disk I/O**: Async SQLite writes, non-blocking
