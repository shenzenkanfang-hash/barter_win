Coding Conventions - Barter-rs Trading System
=============================================

Author: Claude Code Analysis
Created: 2026-03-25
Status: Complete

================================================================================
1. LIBRARY HEADER
================================================================================

All lib.rs files MUST start with:

    #![forbid(unsafe_code)]
    #![allow(dead_code)]

The #![forbid(unsafe_code)] is MANDATORY - no unsafe code permitted.

================================================================================
2. DERIVE MACRO ORDER
================================================================================

Standard derive order for all types:

    #[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]

Exceptions:
- When Eq is not needed: #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
- Simple types without Serialize: #[derive(Debug, Clone, Copy, PartialEq, Eq)]
- Default-only types: #[derive(Debug, Clone, Default, Serialize, Deserialize)]

================================================================================
3. ERROR HANDLING PATTERN
================================================================================

Using thiserror crate exclusively. Pattern:

    use thiserror::Error;

    #[derive(Debug, Clone, Eq, PartialEq, Error)]
    pub enum MyError {
        #[error("描述: {0}")]
        VariantName(String),

        #[error("复杂描述: {field}")]
        ComplexVariant { field: String },
    }

    pub type Result<T> = std::result::Result<T, MyError>;

From implementations for error conversion:

    impl From<rusqlite::Error> for MyError {
        fn from(e: rusqlite::Error) -> Self {
            MyError::Database(e.to_string())
        }
    }

    impl From<serde_json::Error> for MyError {
        fn from(e: serde_json::Error) -> Self {
            MyError::Serialization(e.to_string())
        }
    }

================================================================================
4. PROHIBITED PATTERNS
================================================================================

MUST NOT USE:
- panic!() - all errors must return Result
- unwrap() on non-test code - use ? or expect with context
- unsafe code
- excessive clone() - prefer references
- locks on hot paths (Tick接收、指标更新、策略判断)

================================================================================
5. NAMING CONVENTIONS
================================================================================

Modules:
- snake_case: lib.rs, my_module.rs, types.rs
- Module dirs: f_engine, b_data_source, c_data_process

Types:
- PascalCase: StrategyId, OrderRequest, TradingDecision
- Enum variants: PascalCase with descriptive names

Functions:
- snake_case: new(), calculate_ema(), check_risk()

Variables:
- snake_case: order_request, account_balance
- Prefixes for clarity: is_enabled, has_position, can_trade

Traits:
- Noun-based names: ExchangeGateway, StrategyExecutor, RiskChecker
- Not "I" prefix or "-er" suffix

Constants:
- SCREAMING_SNAKE_CASE: MAX_RETRY_COUNT, DEFAULT_TIMEOUT

================================================================================
6. TRAIT DEFINITIONS
================================================================================

All traits must include Send + Sync for thread safety:

    pub trait MyTrait: Send + Sync {
        fn method(&self, arg: Type) -> Result<Output>;
    }

Method signatures use &self (not &mut self) to enable concurrent access.

Builder pattern for optional parameters on types.

================================================================================
7. MODULE ORGANIZATION
================================================================================

Standard lib.rs structure:

    #![forbid(unsafe_code)]
    #![allow(dead_code)]

    //! ModuleName - Brief description
    //!
    //! Detailed description of purpose and architecture.

    pub mod submodule1;
    pub mod submodule2;

    // Re-exports
    pub use submodule1::{TypeA, TypeB};
    pub use submodule2::TraitC;

Comments use //! for module-level doc comments.

================================================================================
8. DECIMAL AND NUMERIC TYPES
================================================================================

Use rust_decimal::Decimal for all financial calculations:

    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;

    let price = dec!(50000);
    let quantity = dec!(0.1);

DO NOT use f32 or f64 for monetary values.

================================================================================
9. TIME HANDLING
================================================================================

Use chrono with DateTime<Utc>:

    use chrono::{DateTime, Utc};

    let timestamp: DateTime<Utc> = Utc::now();
    let ts = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();

================================================================================
10. SYNCHRONIZATION PRIMITIVES
================================================================================

Use parking_lot over std sync primitives:

    use parking_lot::RwLock;

    let shared_state = RwLock::new(initial_value);
    let read_guard = shared_state.read();
    let mut write_guard = shared_state.write();

parking_lot::RwLock is faster than std::sync::RwLock.

================================================================================
11. COLLECTIONS
================================================================================

Use FnvHashMap for O(1) lookups with small keys:

    use fnv::FnvHashMap;

For ordered iterations, use std::collections::HashMap.

================================================================================
12. SERIALIZATION
================================================================================

Use serde with JSON:

    #[derive(Serialize, Deserialize)]
    pub struct MyStruct { ... }

    let json = serde_json::to_string(&value).unwrap();
    let restored: MyType = serde_json::from_str(&json).unwrap();

================================================================================
13. LOGGING
================================================================================

Use tracing for structured logging:

    use tracing::{info, warn, error};

    info!("Order placed: {}", order_id);
    warn!("Risk check failed: {}", reason);
    error!("Network error: {}", e);

================================================================================
14. INTERFACE SEGREGATION
================================================================================

Define clear interfaces in separate trait files:

    // interfaces/strategy.rs
    pub trait StrategyExecutor: Send + Sync {
        fn register(&self, strategy: Arc<dyn StrategyInstance>);
        fn dispatch(&self, bar: &MarketKLine) -> Vec<TradingSignal>;
    }

Re-export types from a_common for shared types:

    pub use a_common::models::types::TradingAction;

================================================================================
15. TEST HELPERS PATTERN
================================================================================

Builder pattern for test data:

    impl TestStrategy {
        fn new(id: &str, symbols: Vec<String>) -> Self { ... }
        fn set_signals(&self, signals: Vec<TradingSignal>) { ... }
        fn set_enabled(&self, enabled: bool) { ... }
    }

Mock gateways implement the same trait as real implementations:

    impl ExchangeGateway for MockExchangeGateway {
        fn place_order(&self, req: OrderRequest) -> Result<OrderResult, EngineError> { ... }
    }

================================================================================
16. TYPE RE-EXPORTS
================================================================================

Centralize type definitions in authoritative locations:

    // a_common for shared types
    pub use a_common::models::types::{Side, OrderType, TradingAction};

    // f_engine re-exports for business types
    pub use crate::types::OrderRequest;

================================================================================
17. DOCUMENTATION
================================================================================

Module-level documentation:

    //! ModuleName - Brief description
    //!
    //! Detailed description of purpose, architecture, and usage.

Type documentation:

    /// K线数据
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct KLine { ... }

================================================================================
18. FILE LENGTH AND ORGANIZATION
================================================================================

Target <500 lines per file. Split large modules into submodules.

Each submodule in its own file under parent module directory.

================================================================================
19. CONFIGURATION PATTERNS
================================================================================

Use const for compile-time constants:

    const MAX_RETRY_COUNT: u8 = 3;

Use static with lazy initialization for runtime config:

    use std::sync::OnceLock;
    static CONFIG: OnceLock<MyConfig> = OnceLock::new();

================================================================================
20. ARCHITECTURE CONSTRAINTS
================================================================================

Hot paths (no locks):
- Tick接收
- 指标更新
- 策略判断

Locks only for:
- 下单
- 资金更新

Lock外的预检所有风控条件 (pre_check without lock).

================================================================================
END OF CONVENTIONS
================================================================================
