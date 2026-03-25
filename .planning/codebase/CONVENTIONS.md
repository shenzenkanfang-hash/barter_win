================================================================================
CODE CONVENTIONS - Rust Barter-rs Trading System
================================================================================

================================================================================
1. LIB.RS HEADER - FORBIDDEN UNSAFE CODE
================================================================================

All lib.rs files MUST start with:

    #![forbid(unsafe_code)]
    #![allow(dead_code)]

Example (from a_common/src/lib.rs):

    #![forbid(unsafe_code)]
    #![allow(dead_code)]

    //! a_common - 基础设施层
    //!
    //! 提供 API/WS 网关、配置、通用错误、数据模型等基础设施组件。

Rationale: This project explicitly forbids unsafe code in library crates to ensure memory safety in the trading engine.


================================================================================
2. DERIVE MACRO ORDERING
================================================================================

Standard order for derive macros on structs and enums:

    #[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]

For enums with Error trait:

    #[derive(Debug, Clone, Eq, PartialEq, Error)]

For simple types without Serialize/Deserialize:

    #[derive(Debug, Clone, Default)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]

For types needing Hash:

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]

Order priority: Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, Error

Examples from codebase:

    // d_checktable/src/types.rs
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    #[derive(Debug, Clone)]

    // c_data_process/src/types.rs
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[derive(Debug, Clone, Default, Serialize, Deserialize)]

    // Error types
    #[derive(Debug, Clone, Eq, PartialEq, Error)]           // thiserror
    #[derive(Debug, Clone, Error)]                            // thiserror (no Eq/PartialEq needed)


================================================================================
3. ERROR HANDLING - THISERROR PATTERN
================================================================================

All error types use thiserror::Error for clear error hierarchy.

Pattern A - Standard Error Enum:

    use thiserror::Error;

    #[derive(Debug, Clone, Eq, PartialEq, Error)]
    pub enum MarketError {
        #[error("WebSocket连接失败: {0}")]
        WebSocketConnectionFailed(String),

        #[error("WebSocket错误: {0}")]
        WebSocketError(String),

        #[error("序列化错误: {0}")]
        SerializeError(String),

        #[error("订阅失败: {0}")]
        SubscribeFailed(String),
    }

Pattern B - Unified AppError with From implementations:

    #[derive(Debug, Clone, Error)]
    pub enum AppError {
        #[error("[Engine] 风控检查失败: {0}")]
        RiskCheckFailed(String),

        #[error("[Market] WebSocket连接失败: {0}")]
        WebSocketConnectionFailed(String),

        #[error("[Data] 序列化错误: {0}")]
        SerializeError(String),
    }

    impl From<EngineError> for AppError {
        fn from(e: EngineError) -> Self {
            match e {
                EngineError::RiskCheckFailed(msg) => AppError::RiskCheckFailed(msg),
                // ... other variants
            }
        }
    }

Key conventions:
- Error messages in Chinese with category prefix: [Engine], [Market], [Data], [Infra]
- Use String for error context, not &'static str
- Implement From<> for error conversion between layers
- Error variants grouped by module with comments


================================================================================
4. FORBIDDEN PATTERNS
================================================================================

4.1 NO panic!()

    PROHIBITED:
        panic!("something went wrong");
        panic!("position limit exceeded");

    REQUIRED:
        Return Result<T, SomeError> and handle errors properly.

    Rationale: Panic would crash the trading engine, losing state and money.

4.2 NO unsafe CODE

    PROHIBITED:
        unsafe { ... }
        #![allow(unsafe_code)]  // in lib.rs

    REQUIRED:
        #![forbid(unsafe_code)]

    Rationale: Trading systems require memory safety guarantees.

4.3 NO EXCESSIVE clone()

    PROHIBITED (in hot paths):
        let data = big_struct.clone();

    PREFERRED:
        Use references (&T) or Arc/Rc for shared ownership.
        Only clone when necessary (e.g., returning owned data).

4.4 NO MUTEX IN HOT PATHS

    PROHIBITED (in Tick receive, indicator update, strategy judgment):
        let guard = mutex.lock().unwrap();

    REQUIRED:
        Use parking_lot::RwLock for read-heavy workloads.
        Use atomic types for simple counters.

    Rationale: Lock contention destroys performance in high-frequency trading.


================================================================================
5. NAMING CONVENTIONS
================================================================================

5.1 MODULES
    - lowercase_with_underscores: api, ws, risk_monitor, position_manager
    - No pluralization: data_source not data_sources

5.2 TYPES/STRUCTS/ENUMS
    - PascalCase: TradingEngine, OrderRequest, PositionSide
    - Exception: acronyms stay uppercase: OKX, BTCUSDT, KLine

5.3 FUNCTIONS/METHODS
    - snake_case: calculate_position(), validate_order()
    - Builder pattern methods: with_price(), with_stop_loss()

5.4 CONSTANTS
    - SCREAMING_SNAKE_CASE: MAX_POSITION_SIZE, DEFAULT_TIMEOUT_MS
    - Or UPPER_SNAKE for config: KLINE_1M_REALTIME_DIR

5.5 TRAIT NAMES
    - PascalCase: Strategy, ExchangeGateway, MarketDataProvider
    - Behavior nouns: Read, Write, Clone

5.6 ERROR VARIANTS
    - PascalCase: InsufficientFunds, PositionLimitExceeded
    - Verb past tense: ConnectionFailed, OrderRejected
    - Category prefix in AppError: [Engine], [Market]


================================================================================
6. DOCUMENTATION
================================================================================

Every module should have a doc comment:

    //! Module Name - Purpose
    //!
    //! Additional description of what this module does and its responsibilities.

Example:

    //! a_common - 基础设施层
    //!
    //! 提供 API/WS 网关、配置、通用错误、数据模型等基础设施组件。


================================================================================
7. MODULE STRUCTURE
================================================================================

f_engine/src/ subdirectory structure (enforced):

    f_engine/src/
    ├── core/               # 核心引擎 (engine.rs, pipeline.rs, pipeline_form.rs)
    ├── risk/               # 风控 (risk.rs, risk_rechecker.rs, order_check.rs, thresholds.rs, minute_risk.rs)
    ├── order/              # 订单 (order.rs, gateway.rs, mock_binance_gateway.rs)
    ├── position/           # 持仓 (position_manager.rs, position_exclusion.rs)
    ├── persistence/        # 持久化 (sqlite_persistence.rs, memory_backup.rs, disaster_recovery.rs, persistence.rs)
    ├── channel/            # 通道 (channel.rs, mode.rs)
    └── shared/             # 共享 (account_pool.rs, check_table.rs, pnl_manager.rs, symbol_rules.rs, etc.)

Key rule: No new files outside submodules. All new functionality goes into existing submodules.


================================================================================
8. IMPORT CONVENTIONS
================================================================================

Use crate:: for internal imports:

    use crate::core::engine_v2::{TradingEngineV2, TradingEngineConfig};
    use crate::types::TradingAction;

Use external crate names as imported:

    use rust_decimal_macros::dec;
    use chrono::{DateTime, Utc};
    use parking_lot::RwLock;
    use thiserror::Error;

Group imports by external crate:

    use rust_decimal_macros::dec;
    use rust_decimal::Decimal;

    use chrono::{DateTime, Utc};

    use parking_lot::RwLock;

    use serde::{Serialize, Deserialize};


================================================================================
9. TEST HELPER CONVENTIONS
================================================================================

Test helper functions marked with #[allow(dead_code)]:

    #[allow(dead_code)]
    fn create_sample_klines() -> Vec<KLine> {
        vec![
            KLine {
                symbol: "BTCUSDT".to_string(),
                // ...
            },
        ]
    }

Decimal creation uses rust_decimal_macros:

    dec!(50000)
    dec!(0.1)
    dec!(3.5)


================================================================================
10. TRAIT BOUND ORDERING
================================================================================

When multiple trait bounds are needed:

    impl<T: Clone + Send + Sync + 'static> SomeTrait for T

Order: Clone, Send, Sync, 'static, then any custom trait bounds.

================================================================================
END OF CONVENTIONS
================================================================================
