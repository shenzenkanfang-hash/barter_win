RUST QUANTITATIVE TRADING SYSTEM - TESTING GUIDE

================================================================================
TEST CRATE STATUS
================================================================================

The g_test crate is currently DISABLED.

Reason: The crate has 544 compilation errors that need to be resolved.
The crate is temporarily excluded from the build until the errors are fixed.

Do not attempt to build or test g_test at this time.

================================================================================
TEST LOCATIONS
================================================================================

Unit Tests:
- Place #[test] functions in the same file as the code they test
- Convention: at the bottom of crate/src/module.rs files
- Example: crate/src/indicators/src/lib.rs has #[cfg(test)] module at bottom

Integration Tests:
- Location: crates/*/tests/ directory
- Example: crates/b_data_source/tests/ for data source integration tests
- Example: crates/f_engine/tests/ for execution engine tests

Test modules are compiled separately and only run with cargo test.

================================================================================
TEST UTILITIES
================================================================================

ReplaySource - Historical Data Playback
Location: b_data_mock/replay_source.rs

Purpose: Plays back historical market data from CSV files for testing.

Usage pattern:
1. Load CSV file with historical ticks/klines
2. Create ReplaySource with config (speed, start_time, etc.)
3. Call next() to get next historical data point
4. Simulates real-time delivery of historical data

Example:
let replay = ReplaySource::from_csv("test_data/btc_usdt_1m.csv");
replay.set_speed(1.0);  // 1x playback speed
while let Some(tick) = replay.next() {
    // process tick
}

MockApiGateway - Sandbox Testing
Location: b_data_mock/api/mock_gateway.rs

Purpose: Mocks exchange API responses for sandbox testing without real connections.

Supports:
- Mock order placement and cancellation
- Mock market data subscription
- Mock account balance queries
- Simulated network latency and failures

Example:
let gateway = MockApiGateway::new();
gateway.mock_order_response(OrderType::Limit, true);  // Always succeed
gateway.mock_fill_response(vec![Fill::partial(100, 50.0)]);

MockAccount - Account Simulation
Location: Typically in b_data_mock or test utils

Purpose: Simulates account state for testing without real exchange connection.

MockConfig - Configuration Simulation
Purpose: Provides test configurations that override production defaults.

Use MockConfig when you need to:
- Set fake API keys
- Configure test-specific timeouts
- Override exchange endpoints to localhost

================================================================================
PLATFORM GUARDS
================================================================================

Some tests are platform-specific. Use conditional compilation:

Windows-specific tests:
#[cfg(windows)]
#[test]
fn test_windows_path_handling() {
    // Windows path handling test
}

Linux-specific tests:
#[cfg(target_os = "linux")]
#[test]
fn test_linux_socket_handling() {
    // Linux socket test
}

Common guards available:
#[cfg(windows)]
#[cfg(target_os = "linux")]
#[cfg(target_os = "macos")]
#[cfg(all(windows, feature = "test"))]
#[cfg(unix)]

================================================================================
TEST PATTERNS
================================================================================

MarketDataStoreImpl Test Helper:

A common pattern for creating test instances with temporary directories:

impl MarketDataStoreImpl {
    /// Creates a new instance for testing with a temporary directory.
    /// Automatically cleans up the temp directory when dropped.
    pub fn new_test() -> (Self, TempDir) {
        let temp_dir = TempDir::new("market_data_test").unwrap();
        let path = temp_dir.path().to_path_buf();
        let store = Self::new(path);
        (store, temp_dir)
    }
}

Usage:
#[test]
fn test_store_and_retrieve() {
    let (store, _temp_dir) = MarketDataStoreImpl::new_test();
    store.insert(price_data).unwrap();
    let retrieved = store.get(&symbol).unwrap();
    assert_eq!(retrieved.price, expected_price);
}

================================================================================
TEST FRAMEWORK
================================================================================

No formal test framework is used beyond cargo test.

Standard Rust testing:
- #[test] for test functions
- #[cfg(test)] for test modules
- #[should_panic] for expected panics
- #[ignore] for tests that should not run in normal cargo test

No external test frameworks like rstest, proptest, etc. are currently used.
If you need property-based testing, discuss with the team first.

Run all tests:
cargo test

Run tests for specific crate:
cargo test -p b_data_source

Run tests with output:
RUST_BACKTRACE=1 cargo test -- --nocapture

================================================================================
PIPELINE STORE FOR OBSERVABILITY TESTING
================================================================================

PipelineStore provides observability testing capabilities.

Location: Likely in c_data_process or a similar pipeline-related crate.

Features:
- trace_id tracking through the pipeline
- version tracking for components
- timing/metrics collection

Purpose: Verify that observability infrastructure correctly propagates
context through the data processing pipeline.

Example usage:
let store = PipelineStore::new_test();
let trace_id = store.insert_trace("tick_processing".to_string());
// Process through pipeline
let completed = store.get_trace(trace_id).unwrap();
assert!(completed.stages.len() > 0);

trace_id: Unique identifier for a single pipeline execution, useful for
correlating logs and metrics across components.

version tracking: Records which version of each component processed the data,
useful for debugging version mismatches.

================================================================================
RUNNING TESTS
================================================================================

Basic test run:
cargo test

Run with output capture disabled (see println):
cargo test -- --nocapture

Run specific test:
cargo test test_order_execution

Run tests in release mode (may catch different bugs):
cargo test --release

Run with all features:
cargo test --all-features

Check test coverage (requires tarpaulin):
cargo tarpaulin --verbose

================================================================================
MANUAL TESTING
================================================================================

For manual testing of the trading system:

1. Use b_data_mock to generate simulated market data
2. Connect to a sandbox exchange if available
3. Use paper trading mode to verify behavior without real funds

Manual testing checklist:
- [ ] System starts without panics
- [ ] Market data flows through pipeline
- [ ] Orders can be created and tracked
- [ ] Positions update correctly on fills
- [ ] Risk limits are enforced
- [ ] Logs contain trace_id for debugging

================================================================================
TEST DATA
================================================================================

Test data files are typically stored in:
- crates/*/test_data/ directories
- Or at the repository root in test_data/

CSV format for market data:
timestamp,symbol,open,high,low,close,volume
2024-01-01T00:00:00Z,BTCUSDT,50000.0,50100.0,49900.0,50050.0,100.5

Ensure test data:
- Has realistic price ranges
- Contains no NaN or infinity values
- Uses proper DateTime<Utc> timestamps
- Has no gaps that would trigger sequence warnings
