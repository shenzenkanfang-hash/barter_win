================================================================================
TESTING PATTERNS - Rust Barter-rs Trading System
================================================================================

================================================================================
1. TEST FRAMEWORK
================================================================================

Primary test framework: Rust built-in test harness with #[cfg(test)]

Dependencies:
    [dev-dependencies]
    rust_decimal_macros = "1.x"    # For dec!() macro in tests
    tokio = { version = "1.x", features = ["full"] }  # For async tests
    tempfile = "3.x"               # For temporary files

Test file location: Inline with source files or in dedicated test crate (g_test)


================================================================================
2. TEST STRUCTURE AND LOCATIONS
================================================================================

2.1 INLINE TESTS (#[cfg(test)] modules)

Tests are placed in a separate mod at the bottom of source files:

    // f_engine/src/core/tests.rs
    #![cfg(test)]

    #[cfg(test)]
    mod business_types_tests {
        use super::*;

        #[test]
        fn test_position_side() {
            assert!(PositionSide::LONG.is_long());
            assert!(!PositionSide::LONG.is_short());
        }
    }

    #[cfg(test)]
    mod triggers_tests {
        #[test]
        fn test_trigger_config_default() {
            let config = TriggerConfig::default();
            assert_eq!(config.minute_volatility_threshold, dec!(13));
        }
    }

Files with inline tests (examples):
    - crates/f_engine/src/core/tests.rs (738 lines, 25+ tests)
    - crates/b_data_source/src/symbol_rules/mod.rs
    - crates/c_data_process/src/strategy_state/mod.rs
    - crates/e_risk_monitor/src/risk/common/thresholds.rs

2.2 DEDICATED TEST CRATE (g_test)

Dedicated test crate: crates/g_test/src/

    g_test/src/
    ├── lib.rs                     # Test module root
    ├── b_data_source/
    │   ├── mod.rs
    │   ├── replay_source_test.rs  # ReplaySource black box tests
    │   ├── trader_pool_test.rs
    │   ├── api/
    │   │   ├── mod.rs
    │   │   ├── account.rs
    │   │   └── symbol_registry.rs
    │   ├── models/
    │   │   ├── mod.rs
    │   │   └── models.rs
    │   ├── ws/
    │   │   ├── mod.rs
    │   │   ├── kline.rs
    │   │   └── orderbook.rs
    │   └── recovery.rs
    └── strategy/
        ├── mod.rs
        ├── mock_gateway.rs        # Mock implementations
        ├── strategy_executor_test.rs
        └── trading_integration_test.rs


================================================================================
3. TEST NAMING CONVENTIONS
================================================================================

3.1 TEST FUNCTION PATTERNS

Standard pattern: test_<unit>_<scenario>_<expected>

    #[test]
    fn test_position_side() { }

    #[test]
    fn test_volatility_tier() { }

    #[test]
    fn test_strategy_response_execute() { }

    #[test]
    fn test_order_lifecycle() { }

    #[test]
    fn test_risk_check_result() { }

    #[test]
    fn test_replay_source_next_kline_exhausted() { }

3.2 TEST MODULE PATTERNS

Group related tests in mod blocks:

    #[cfg(test)]
    mod business_types_tests { }

    #[cfg(test)]
    mod triggers_tests { }

    #[cfg(test)]
    mod execution_tests { }

    #[cfg(test)]
    mod fund_pool_tests { }

    #[cfg(test)]
    mod risk_manager_tests { }

    #[cfg(test)]
    mod monitoring_tests { }

    #[cfg(test)]
    mod rollback_tests { }

    #[cfg(test)]
    mod trade_lock_tests { }

3.3 INTEGRATION TEST PATTERNS

Black box test files: test_<component>_<type>

    test_replay_source_from_data()
    test_replay_source_next_kline()
    test_replay_source_from_csv()
    test_executor_register_and_count()
    test_signal_aggregator_priority_order()


================================================================================
4. ASYNC TESTS
================================================================================

Use #[tokio::test] for async tests:

    #[tokio::test]
    async fn test_replay_source_from_csv() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "symbol,period,open,high,low,close,volume,timestamp").unwrap();
        writeln!(temp_file, "BTCUSDT,1m,50000,50500,49500,50200,100,2024-01-01T00:00:00Z").unwrap();

        let path = temp_file.path();
        let replay = ReplaySource::from_csv(path).await.unwrap();
        assert_eq!(replay.len(), 3);
    }

    #[tokio::test]
    async fn test_replay_source_from_csv_multi_line() {
        // ...
    }


================================================================================
5. MOCKING PATTERNS
================================================================================

5.1 STRUCT MOCK IMPLEMENTATION

Create test structs implementing traits:

    #[allow(dead_code)]
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
        fn name(&self) -> &str { &self.name }
        fn symbols(&self) -> Vec<String> { self.symbols.clone() }
        fn is_enabled(&self) -> bool { *self.enabled.read() }
        fn on_bar(&self, _bar: &StrategyKLine) -> Option<TradingSignal> {
            self.signals_to_return.read().first().cloned()
        }
        fn state(&self) -> &StrategyState { &self.state }
    }

5.2 MOCK GATEWAY

From g_test/src/strategy/mock_gateway.rs:

    pub struct MockExchangeGateway {
        pub orders: Arc<Mutex<Vec<PlacedOrder>>>,
        pub position: Arc<Mutex<PositionSnapshot>>,
    }

5.3 HELPER FUNCTIONS

Mark test helpers with #[allow(dead_code)]:

    #[allow(dead_code)]
    fn create_sample_klines() -> Vec<KLine> {
        vec![
            KLine {
                symbol: "BTCUSDT".to_string(),
                period: Period::Minute(1),
                open: dec!(50000),
                // ...
            },
        ]
    }

    #[allow(dead_code)]
    fn create_multi_symbol_klines() -> Vec<KLine> {
        // ...
    }


================================================================================
6. ASSERTION PATTERNS
================================================================================

Standard assertions:

    assert!(PositionSide::LONG.is_long());
    assert!(!PositionSide::LONG.is_short());

    assert_eq!(pool.available(), dec!(10000));
    assert_eq!(replay.len(), 5);

    assert!(result.is_ok());
    assert!(result.is_err());

    assert!(signal.is_valid());
    assert!(!invalid_signal.is_valid());

With failure messages (Chinese):

    assert_eq!(executor.count(), 1, "初始策略数量应为 0");
    assert_eq!(signals.len(), 1, "应返回 1 个信号");
    assert!(signals.is_empty(), "不应返回任何信号");


================================================================================
7. TEST DATA CREATION
================================================================================

7.1 DECIMAL VALUES

Use rust_decimal_macros:

    dec!(50000)
    dec!(0.1)
    dec!(3.5)
    dec!(0.05)
    dec!(0.02)

7.2 KLINE CREATION

    KLine {
        symbol: "BTCUSDT".to_string(),
        period: Period::Minute(1),
        open: dec!(50000),
        high: dec!(50500),
        low: dec!(49500),
        close: dec!(50200),
        volume: dec!(100),
        timestamp: Utc::now(),
    }

7.3 TRADING SIGNAL CREATION

    let signal = TradingSignal::new(
        "BTCUSDT".to_string(),
        Direction::Long,
        dec!(0.1),
        "test_strategy".to_string(),
    )
    .with_price(dec!(50000))
    .with_stop_loss(dec!(49000))
    .with_take_profit(dec!(52000))
    .with_signal_type(SignalType::Open)
    .with_priority(80);

7.4 STRATEGY KLINE

    let bar = StrategyKLine {
        symbol: "BTCUSDT".to_string(),
        period: "1m".to_string(),
        open: dec!(50000),
        high: dec!(51000),
        low: dec!(49000),
        close: dec!(50500),
        volume: dec!(100),
        timestamp: Utc::now(),
    };


================================================================================
8. CONCURRENCY TESTS
================================================================================

Use std::thread for concurrent testing:

    #[test]
    fn test_rollback_manager_concurrent() {
        use std::sync::Arc;

        let fund_pool = Arc::new(FundPoolManager::new(dec!(100000), dec!(200000)));
        let manager = Arc::new(RollbackManager::new(fund_pool.clone()));

        fund_pool.freeze(ChannelType::HighSpeed, dec!(10000));

        let mut handles = vec![];
        for i in 0..10 {
            let m = manager.clone();
            let fp = fund_pool.clone();
            let handle = std::thread::spawn(move || {
                let result = m.rollback_order(ChannelType::HighSpeed, dec!(1000));
                assert!(result.success, "并发回滚应该成功");
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(fund_pool.available(ChannelType::HighSpeed), dec!(100000));
    }


================================================================================
9. COVERAGE APPROACH
================================================================================

9.1 UNIT TEST COVERAGE (inline tests)

Cover:
    - Business types: enums, structs, methods
    - State transitions
    - Configuration defaults
    - Edge cases (empty, boundary values)
    - Error conditions

9.2 INTEGRATION TEST COVERAGE (g_test crate)

Cover:
    - Cross-module communication
    - Trait implementations
    - Full workflows (register -> dispatch -> signal)
    - CSV parsing and file I/O
    - Concurrent operations

9.3 TEST CATEGORIES

From codebase analysis:

    business_types_tests: PositionSide, VolatilityTier, RiskState, ChannelType,
                         StrategyQuery, StrategyResponse, RiskCheckResult,
                         OrderLifecycle, OrderInfo, FundPool, PriceControlOutput

    triggers_tests:      TriggerConfig, MinuteTrigger, DailyTrigger, TriggerManager

    execution_tests:     ExecutionConfig, OrderExecutor, TradingPipeline, StateSyncer

    fund_pool_tests:     FundPoolManager, freeze, confirm_usage, rollback

    risk_manager_tests:  RiskConfig, RiskManager pre_check/lock_check

    monitoring_tests:    TimeoutMonitor, HealthChecker, TimeoutSeverity

    rollback_tests:      RollbackManager, OrderRollbackHelper, concurrent rollback

    trade_lock_tests:    TradeLock try_lock, is_stale, position


================================================================================
10. TEST EXECUTION
================================================================================

Run all tests:
    cargo test --all

Run tests for specific crate:
    cargo test -p f_engine
    cargo test -p g_test

Run with output:
    cargo test -- --nocapture

Run specific test:
    cargo test test_position_side
    cargo test test_executor_register

Check coverage (requires tarpaulin):
    cargo tarpaulin --all-crates


================================================================================
11. TEST FILE DOCUMENTATION
================================================================================

Test files should have module documentation:

    //! ReplaySource 黑盒测试
    //!
    //! 测试历史数据回放功能

    //! StrategyExecutor 黑盒测试
    //!
    //! 测试策略调度器的完整功能

Test module documentation:

    // ============================================================================
    // TradeLock 测试
    // ============================================================================

    #[cfg(test)]
    mod trade_lock_tests {
        // ...
    }


================================================================================
END OF TESTING PATTERNS
================================================================================
