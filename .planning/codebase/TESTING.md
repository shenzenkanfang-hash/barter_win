Testing Conventions - Barter-rs Trading System
==============================================

Author: Claude Code Analysis
Created: 2026-03-25
Status: Complete

================================================================================
1. TEST LOCATION
================================================================================

Unit tests: Inline in source files with #[cfg(test)] module

    // src/core/tests.rs or at bottom of source file
    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_my_function() { ... }
    }

Integration tests: Centralized in g_test crate

    crates/g_test/src/
    ├── b_data_source/   # b_data_source integration tests
    ├── strategy/        # Strategy layer tests
    └── lib.rs

Sandbox/Examples: h_sandbox crate for experimental testing

    crates/h_sandbox/examples/

================================================================================
2. TEST MODULE HEADER
================================================================================

Each test module starts with:

    #![forbid(unsafe_code)]

    //! ModuleName Tests
    //!
    //! Description of test coverage

    use relevant_modules;

================================================================================
3. FRAMEWORK AND DEPENDENCIES
================================================================================

Standard library testing:
- Use #[test] attribute
- Use #[cfg(test)] for conditional compilation
- Use assert!, assert_eq!, assert_ne! for assertions

Decimal testing with rust_decimal_macros:

    use rust_decimal_macros::dec;

    assert_eq!(value, dec!(100));
    assert_eq!(price, dec!(50000.12345));

Time testing with chrono:

    let ts = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();

================================================================================
4. MOCK GATEWAY PATTERN
================================================================================

Mock implementations implement the same trait as production:

    pub struct MockExchangeGateway {
        account: RwLock<ExchangeAccount>,
        positions: RwLock<HashMap<String, ExchangePosition>>,
        orders: RwLock<Vec<OrderResult>>,
        should_reject: RwLock<bool>,
        reject_reason: RwLock<Option<String>>,
    }

    impl ExchangeGateway for MockExchangeGateway {
        fn place_order(&self, req: OrderRequest) -> Result<OrderResult, EngineError> { ... }
        fn get_account(&self) -> Result<ExchangeAccount, EngineError> { ... }
        fn get_position(&self, symbol: &str) -> Result<Option<ExchangePosition>, EngineError> { ... }
    }

Helper methods on mock:

    impl MockExchangeGateway {
        pub fn new(initial_balance: Decimal) -> Self { ... }
        pub fn default_test() -> Self { Self::new(dec!(10000)) }
        pub fn set_reject(&self, reason: Option<String>) { ... }
        pub fn reset(&self) { ... }
    }

================================================================================
5. TEST STRATEGY PATTERN
================================================================================

Test strategies implement the Strategy trait:

    struct TestStrategy {
        id: String,
        name: String,
        symbols: Vec<String>,
        enabled: RwLock<bool>,
        state: StrategyState,
        signals_to_return: RwLock<Vec<TradingSignal>>,
    }

    impl Strategy for TestStrategy {
        fn id(&self) -> &str { &self.id }
        fn on_bar(&self, _bar: &StrategyKLine) -> Option<TradingSignal> {
            self.signals_to_return.read().first().cloned()
        }
    }

================================================================================
6. TEST DATA BUILDER
================================================================================

Builder pattern for test data construction:

    impl TestStrategy {
        fn new(id: &str, symbols: Vec<String>) -> Self { ... }
        fn set_signals(&self, signals: Vec<TradingSignal>) { ... }
        fn set_enabled(&self, enabled: bool) { ... }
    }

TradingSignal builder:

    let signal = TradingSignal::new(
        "BTCUSDT".to_string(),
        Direction::Long,
        dec!(0.1),
        "test1".to_string(),
    )
    .with_price(dec!(50000))
    .with_stop_loss(dec!(49000))
    .with_take_profit(dec!(52000))
    .with_signal_type(SignalType::Open)
    .with_priority(80);

================================================================================
7. TEST ORGANIZATION
================================================================================

Group tests by functionality with section comments:

    // ============================================================================
    // StrategyExecutor 基本功能测试
    // ============================================================================

    #[test]
    fn test_executor_register_and_count() { ... }

    // ============================================================================
    // SignalAggregator 测试
    // ============================================================================

    #[test]
    fn test_signal_aggregator_empty() { ... }

================================================================================
8. ASSERTION STYLE
================================================================================

Include descriptive messages:

    assert_eq!(executor.count(), 0, "初始策略数量应为 0");
    assert!(signals.is_empty(), "禁用策略不应返回信号");
    assert!(result.is_ok(), "应返回成功结果");

Test equality with decimal precision:

    assert_eq!(result[0].quantity, dec!(0.2), "应保留最大数量");

================================================================================
9. SERIALIZATION TESTING
================================================================================

Test JSON round-trip:

    #[test]
    fn test_kline_serialization() {
        let json = serde_json::to_string(&kline).unwrap();
        assert!(json.contains("BTCUSDT"));

        let restored: KLine = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.symbol, "BTCUSDT");
    }

================================================================================
10. INTEGRATION TEST STRUCTURE
================================================================================

Full integration test pattern:

    #[tokio::test]
    async fn test_trading_integration() {
        // Setup
        let gateway = MockExchangeGateway::default_test();
        let executor = StrategyExecutor::new();

        // Execute
        let result = execute_trading_cycle(...).await;

        // Verify
        assert!(result.is_ok());
    }

================================================================================
11. SANDBOX TESTING
================================================================================

Performance tests in h_sandbox:

    crates/h_sandbox/examples/full_loop_test.rs
    crates/h_sandbox/examples/perf_test.rs

Backtest infrastructure:

    crates/h_sandbox/src/backtest/
    ├── loader.rs    # Data loading
    ├── mod.rs
    └── strategy.rs  # Backtest strategy

================================================================================
12. COVERAGE TARGETS
================================================================================

Core business logic: 80%+ coverage
Error paths: Must be tested
Edge cases:
- Zero values
- Maximum limits
- Empty collections
- Concurrent access

================================================================================
13. TEST NAMING
================================================================================

Use descriptive test names:

    fn test_executor_register_and_count()
    fn test_signal_aggregator_same_direction_max_qty()
    fn test_trading_signal_builder_pattern()
    fn test_order_lifecycle_state_transitions()

Pattern: test_<unit>_<scenario>_<expected_result>

================================================================================
14. FIXTURES AND SETUP
================================================================================

Use setup methods:

    fn setup_test_environment() -> (MockGateway, Executor) {
        let gateway = MockExchangeGateway::default_test();
        let executor = StrategyExecutor::new();
        (gateway, executor)
    }

Shared test fixtures via common setup:

    #[cfg(test)]
    mod tests {
        use super::*;

        fn create_test_strategy(id: &str) -> Arc<dyn StrategyInstance> {
            Arc::new(TestStrategy::new(id, vec!["BTCUSDT".to_string()]))
        }
    }

================================================================================
15. ASYNC TESTING
================================================================================

Use tokio::test for async tests:

    #[tokio::test]
    async fn test_async_operation() {
        let result = my_async_function().await;
        assert!(result.is_ok());
    }

================================================================================
16. PROPERTY-BASED TESTING
================================================================================

Not currently used in this codebase, but recommended for future:
- Use proptest for fuzzing numeric calculations
- Test invariants across random inputs

================================================================================
17. BENCHMARK TESTS
================================================================================

Performance tests in h_sandbox/perf_test:

    crates/h_sandbox/src/perf_test/
    ├── engine_driver.rs
    ├── mod.rs
    ├── reporter.rs
    ├── tick_driver.rs
    └── tracker.rs

================================================================================
18. TESTING CONSTRAINTS
================================================================================

No network calls in unit tests (mock external services)
No database calls in unit tests (use in-memory alternatives)
Tests must be deterministic (no random values without seed)
Tests must clean up after themselves (reset mock state)

================================================================================
19. RUNNING TESTS
================================================================================

Run all tests:
    cargo test --all

Run tests for specific crate:
    cargo test -p g_test

Run with output:
    cargo test --all -- --nocapture

Run specific test:
    cargo test test_executor_register_and_count

================================================================================
20. CONTINUOUS INTEGRATION
================================================================================

Recommended CI checks:
- cargo test --all
- cargo check --all
- cargo clippy --all (linting)
- cargo fmt --check (formatting)

================================================================================
END OF TESTING CONVENTIONS
================================================================================
