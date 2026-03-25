================================================================================
STRUCTURE - Barter-rs Quantitative Trading System
================================================================================

PROJECT: barter-rs
PATH: D:\Rust项目\barter-rs-main
STATUS: Active Development (Phase 6: Integration)

================================================================================
1. DIRECTORY LAYOUT
================================================================================

Root Directory:
--------------------------------------------------------------------------------
D:\Rust项目\barter-rs-main/
|
|-- Cargo.toml              Workspace manifest
|-- Cargo.lock               Locked dependencies
|-- src/main.rs              Binary entry point
|-- CLAUDE.md                Project configuration
|
|-- crates/                  All crate modules (8 crates)
|   |-- a_common/            Infrastructure layer
|   |-- b_data_source/        Data source layer
|   |-- c_data_process/        Signal generation layer
|   |-- d_checktable/         Check layer
|   |-- e_risk_monitor/       Risk control layer
|   |-- f_engine/             Trading engine layer
|   |-- g_test/               Test layer
|   |-- h_sandbox/            Sandbox layer
|
|-- .planning/                Project planning
|   |-- codebase/             Codebase documentation
|   |-- PROJECT_COMPLIANCE_REPORT.md
|
|-- deploy/                   Deployment scripts
|-- docs/                     Documentation
|
|-- Bak_非必要指定不读取老版本/    Backup (do not use)
|-- target/                   Build output

================================================================================
2. CRATES STRUCTURE
================================================================================

2.1 crates/a_common - Infrastructure Layer
================================================================================
PATH: crates/a_common/

Directory:
    crates/a_common/
    |-- Cargo.toml
    |-- src/
        |-- lib.rs              Public API exports
        |-- api/                REST API gateway
        |   |-- binance_api.rs  BinanceApiGateway implementation
        |   |-- mod.rs
        |-- ws/                 WebSocket gateway
        |   |-- binance_ws.rs   BinanceWsConnector
        |   |-- websocket.rs    WebSocket base types
        |   |-- mod.rs
        |-- models/             Shared data models
        |   |-- dto.rs          Interface DTOs
        |   |-- market_data.rs  MarketKLine, MarketTick, etc.
        |   |-- types.rs        TradingAction, Side, OrderType, etc.
        |   |-- mod.rs
        |-- claint/             Error types
        |   |-- error.rs        EngineError, MarketError
        |   |-- mod.rs
        |-- config/             Configuration
        |   |-- platform.rs     Platform detection
        |   |-- volatility.rs    VolatilityConfig
        |   |-- mod.rs
        |-- backup/             Memory backup
        |   |-- memory_backup.rs MemoryBackup, account/position snapshots
        |   |-- mod.rs
        |-- volatility/         Volatility calculation
        |   |-- mod.rs          VolatilityCalc, VolatilityStats
        |-- logs/               Checkpoint logging
        |   |-- checkpoint.rs   CheckpointLogger implementations
        |   |-- mod.rs
        |-- util/               Utilities
        |   |-- telegram_notifier.rs
        |   |-- mod.rs
        |-- exchange/          Exchange types
            |-- mod.rs          ExchangeAccount, OrderResult

Key Files:
- lib.rs (68 lines): All public re-exports
- api/binance_api.rs: Binance REST API gateway with rate limiting
- ws/binance_ws.rs: Binance WebSocket connector
- models/market_data.rs: MarketKLine, MarketTick, VolatilityInfo, OrderBookSnapshot

Naming Conventions:
- Module files: snake_case.rs
- Public types: PascalCase
- Private helpers: snake_case

--------------------------------------------------------------------------------

2.2 crates/b_data_source - Data Source Layer
================================================================================
PATH: crates/b_data_source/

Directory:
    crates/b_data_source/
    |-- Cargo.toml
    |-- src/
        |-- lib.rs              Public API exports
        |-- api/                REST API interfaces
        |   |-- data_feeder.rs DataFeeder (unified interface)
        |   |-- account.rs     FuturesAccountData
        |   |-- position.rs    FuturesPositionData
        |   |-- trade_settings.rs TradeSettings, PositionMode
        |   |-- symbol_registry.rs SymbolRegistry
        |   |-- data_sync.rs   FuturesDataSyncer
        |   |-- mod.rs
        |-- ws/                 WebSocket interfaces
        |   |-- kline_1m/      1-minute K-line synthesis
        |   |   |-- kline.rs    Kline1mStream
        |   |   |-- kline_persistence.rs
        |   |   |-- ws.rs       WebSocket handler
        |   |   |-- mod.rs
        |   |-- kline_1d/      1-day K-line synthesis
        |   |   |-- ws.rs
        |   |   |-- mod.rs
        |   |-- order_books/    Order book aggregation
        |   |   |-- orderbook.rs OrderBook, DepthStream
        |   |   |-- ws.rs
        |   |   |-- mod.rs
        |   |-- volatility/    Volatility tracking
        |   |   |-- mod.rs     VolatilityManager
        |   |-- mod.rs
        |-- models/             Business data models
        |   |-- types.rs        KLine, Period, Tick
        |   |-- ws.rs           MarketStream, MockMarketStream
        |   |-- mod.rs
        |-- symbol_rules/       Trading pair rules
        |   |-- mod.rs          SymbolRuleService
        |-- trader_pool.rs      SymbolMeta, TraderPool
        |-- recovery.rs         CheckpointManager, RedisRecovery
        |-- replay_source.rs    Historical data replay

Key Files:
- lib.rs (54 lines): All public re-exports
- api/data_feeder.rs (135 lines): Unified data access interface
- ws/kline_1m/kline.rs: 1m K-line synthesis

Naming Conventions:
- DataFeeder: Central data access point
- KlineXxx: K-line related types
- VolatilityXxx: Volatility calculation types

--------------------------------------------------------------------------------

2.3 crates/c_data_process - Signal Generation Layer
================================================================================
PATH: crates/c_data_process/

Directory:
    crates/c_data_process/
    |-- Cargo.toml
    |-- src/
        |-- lib.rs              Public API exports
        |-- types.rs            Signal types, PineColor, PricePosition
        |-- pine_indicator_full.rs Pine v5 complete indicators
        |-- processor.rs        SignalProcessor
        |-- min/                Minute-level strategy
        |   |-- (input/output types)
        |-- day/                Day-level strategy
        |   |-- (input/output types)
        |-- strategy_state/     Strategy state management
            |-- (StrategyStateManager, etc.)

Key Files:
- lib.rs (24 lines): All public re-exports
- types.rs (297 lines): Signal, PineColor, MarketStatus, PricePosition
- pine_indicator_full.rs: EMA, RSI, PineColorDetector implementation

Naming Conventions:
- Signal types: Signal, TradingSignal
- Market status: MarketStatus (TREND, RANGE, PIN, INVALID)
- Pine color: PineColor (PureGreen, LightGreen, PureRed, LightRed, Purple, Neutral)

--------------------------------------------------------------------------------

2.4 crates/d_checktable - Check Layer
================================================================================
PATH: crates/d_checktable/

Directory:
    crates/d_checktable/
    |-- Cargo.toml
    |-- src/
        |-- lib.rs              Public API exports
        |-- check_table.rs      CheckTable, CheckEntry
        |-- types.rs           Check types
        |-- h_15m/              High-frequency 15m checks
        |   |-- check/
        |   |-- mod.rs
        |-- l_1d/               Low-frequency 1d checks
            |-- check/
            |-- mod.rs

Key Files:
- lib.rs (17 lines): All public re-exports
- check_table.rs (107 lines): CheckTable (FnvHashMap based), CheckEntry

Naming Conventions:
- CheckEntry: Records strategy judgment results
- CheckTable: HashMap<(symbol, strategy_id, period), CheckEntry>

--------------------------------------------------------------------------------

2.5 crates/e_risk_monitor - Risk Control Layer
================================================================================
PATH: crates/e_risk_monitor/

Directory:
    crates/e_risk_monitor/
    |-- Cargo.toml
    |-- src/
        |-- lib.rs              Public API exports
        |-- risk/                Risk checking modules
        |   |-- common/         RiskPreChecker, RiskReChecker
        |   |-- pin/            PinRiskLeverageGuard
        |   |-- trend/          TrendRiskLimitGuard
        |   |-- minute_risk.rs  Minute risk calculations
        |-- position/           Position management
        |   |-- position_manager.rs LocalPositionManager
        |   |-- position_exclusion.rs PositionExclusionChecker
        |-- persistence/        Data persistence
        |   |-- persistence.rs  PersistenceService
        |   |-- sqlite_persistence.rs SqliteEventRecorder
        |   |-- disaster_recovery.rs DisasterRecovery
        |   |-- startup_recovery.rs  StartupRecoveryManager
        |-- shared/              Shared components
            |-- account_pool.rs AccountPool
            |-- margin_config.rs MarginConfig
            |-- market_status.rs MarketStatusDetector
            |-- pnl_manager.rs  PnlManager

Key Files:
- lib.rs (20 lines): All public re-exports
- risk/common/: RiskPreChecker, RiskReChecker, OrderCheck, ThresholdConstants
- persistence/: PersistenceService, SqliteEventRecorder, DisasterRecovery

Naming Conventions:
- RiskXxx: Risk-related guards and checkers
- PositionXxx: Position management types
- PersistenceXxx: Storage-related types

--------------------------------------------------------------------------------

2.6 crates/f_engine - Trading Engine Layer
================================================================================
PATH: crates/f_engine/

Directory:
    crates/f_engine/
    |-- Cargo.toml
    |-- src/
        |-- lib.rs              Public API exports
        |-- types.rs            Core types (StrategyId, TradingDecision)
        |
        |-- core/               Core engine components
        |   |-- mod.rs          Core module exports
        |   |-- engine_v2.rs    TradingEngineV2 (main engine)
        |   |-- engine_state.rs EngineState, EngineStatus
        |   |-- state.rs        SymbolState, TradeLock
        |   |-- strategy_pool.rs StrategyPool
        |   |-- business_types.rs Business types (PositionSide, ChannelType)
        |   |-- triggers.rs     TriggerManager
        |   |-- execution.rs    TradingPipeline, OrderExecutor
        |   |-- fund_pool.rs    FundPoolManager
        |   |-- risk_manager.rs RiskManager
        |   |-- monitoring.rs   TimeoutMonitor
        |   |-- rollback.rs     RollbackManager
        |   |-- tests.rs
        |
        |-- order/              Order execution
        |   |-- mod.rs
        |   |-- order.rs        Order types
        |   |-- gateway.rs      ExchangeGateway trait
        |   |-- mock_binance_gateway.rs Mock implementation
        |
        |-- channel/            Trading modes
        |   |-- mod.rs
        |   |-- mode_switcher.rs ModeSwitcher
        |
        |-- strategy/           Strategy execution
        |   |-- mod.rs
        |   |-- executor.rs     StrategyExecutor
        |
        |-- interfaces/         Trait definitions
            |-- mod.rs          Interface exports
            |-- market_data.rs  MarketDataProvider trait
            |-- strategy.rs     StrategyExecutor, StrategyInstance traits
            |-- risk.rs         RiskChecker trait
            |-- execution.rs    ExchangeGateway trait
            |-- check_table.rs  CheckTableProvider trait
            |-- adapters.rs     Type adapters

Key Files:
- lib.rs (50 lines): All public re-exports
- core/engine_v2.rs (462 lines): Main TradingEngineV2 implementation
- interfaces/mod.rs (26 lines): Interface module organization
- interfaces/market_data.rs: MarketDataProvider trait
- interfaces/strategy.rs: StrategyExecutor trait
- interfaces/risk.rs: RiskChecker trait

Naming Conventions:
- Trait names: XxxTrait or XxxProvider, XxxExecutor
- Implementation: PascalCase (TradingEngineV2, EngineState)
- Internal modules: snake_case

--------------------------------------------------------------------------------

2.7 crates/g_test - Test Layer
================================================================================
PATH: crates/g_test/

Directory:
    crates/g_test/
    |-- Cargo.toml
    |-- src/
        |-- lib.rs
        |-- b_data_source/      b_data_source tests
        |-- strategy/            Strategy tests

--------------------------------------------------------------------------------

2.8 crates/h_sandbox - Sandbox Layer
================================================================================
PATH: crates/h_sandbox/

Directory:
    crates/h_sandbox/
    |-- Cargo.toml
    |-- src/
        |-- lib.rs              Public API exports
        |-- config.rs           ShadowConfig
        |-- simulator.rs        Account, OrderEngine, Position
        |-- gateway.rs          ShadowBinanceGateway
        |-- tick_generator.rs   TickGenerator, TickDriver
        |-- perf_test.rs        PerformanceTracker, EngineDriver
        |-- backtest.rs         BacktestStrategy, MaCrossStrategy

Key Files:
- lib.rs (19 lines): All public re-exports
- simulator.rs: Account, OrderEngine, Position, ShadowRiskChecker
- perf_test.rs: PerfTestConfig, PerformanceTracker

================================================================================
3. KEY FILE LOCATIONS
================================================================================

3.1 Main Entry Points
--------------------------------------------------------------------------------
Binary:         src/main.rs
Workspace:      Cargo.toml
Public crates:  crates/*/src/lib.rs

3.2 Core Engine Files
--------------------------------------------------------------------------------
TradingEngineV2:     crates/f_engine/src/core/engine_v2.rs
EngineState:          crates/f_engine/src/core/engine_state.rs
Execution Pipeline:   crates/f_engine/src/core/execution.rs
Interfaces:           crates/f_engine/src/interfaces/mod.rs

3.3 Data Flow Files
--------------------------------------------------------------------------------
DataFeeder:           crates/b_data_source/src/api/data_feeder.rs
Market Models:        crates/a_common/src/models/market_data.rs
Signal Types:         crates/c_data_process/src/types.rs
Check Table:          crates/d_checktable/src/check_table.rs

3.4 Risk Control Files
--------------------------------------------------------------------------------
Risk Pre-checker:     crates/e_risk_monitor/src/risk/common/mod.rs
Position Manager:      crates/e_risk_monitor/src/position/position_manager.rs
Persistence:          crates/e_risk_monitor/src/persistence/persistence.rs

3.5 Test Files
--------------------------------------------------------------------------------
Integration Tests:    crates/g_test/src/
Sandbox:              crates/h_sandbox/src/

================================================================================
4. NAMING CONVENTIONS
================================================================================

4.1 Module/File Naming
--------------------------------------------------------------------------------
- Rust source files: snake_case.rs
- Module directories: snake_case/
- Test modules: tests.rs or tests/ directory

4.2 Type Naming
--------------------------------------------------------------------------------
- Structs/Enums: PascalCase
- Struct fields: snake_case
- Enum variants: PascalCase or SCREAMING_SNAKE_CASE
- Type aliases: PascalCase

4.3 Trait Naming
--------------------------------------------------------------------------------
- Traits: PascalCase (e.g., MarketDataProvider, StrategyExecutor)
- Methods: snake_case
- Associated types: PascalCase

4.4 Constant Naming
--------------------------------------------------------------------------------
- Module-level constants: SCREAMING_SNAKE_CASE
- Static items: SCREAMING_SNAKE_CASE

4.5 Special Patterns
--------------------------------------------------------------------------------
- Error types: XxxError, XxxErrorKind
- Result types: XxxResult, or Result<T, XxxError>
- Builder patterns: XxxBuilder

================================================================================
5. PUBLIC API ORGANIZATION
================================================================================

5.1 Crate Public API Pattern
-----------------------------
Each crate exposes its public API in lib.rs with:

1. #![forbid(unsafe_code)]
2. pub mod submodules;
3. pub use x::{Type1, Type2};  // Re-exports
4. pub use y::{Trait1, Trait2}; // Trait re-exports

5.2 Cross-Crate Dependencies
-----------------------------
- f_engine depends on: a_common, b_data_source, c_data_process, e_risk_monitor
- e_risk_monitor depends on: a_common
- b_data_source depends on: a_common
- c_data_process depends on: a_common

5.3 Interface Re-exports
------------------------
Common types shared across crates are defined in a_common::models:
- MarketKLine, MarketTick, VolatilityInfo
- TradingAction, Side, OrderType, PositionSide
- SignalDirection, SignalType, TradingSignal

================================================================================
6. ARCHITECTURE CONSTRAINTS
================================================================================

6.1 Forbidden Patterns
----------------------
- NO unsafe code in any crate (#![forbid(unsafe_code)])
- NO panic!() in production code - use Result
- NO locks in high-frequency paths
- NO excessive clone() - prefer references

6.2 Required Patterns
--------------------
- All lib.rs: #![forbid(unsafe_code)]
- Derive macros: #[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
- Error types: use thiserror::Error
- Logging: use tracing (info!, warn!, error!)

6.3 Submodule Constraints (f_engine)
------------------------------------
New functionality in f_engine MUST be placed in appropriate submodules:
- core/    - Core engine components
- order/   - Order execution
- channel/ - Trading modes
- strategy/- Strategy execution
- interfaces/ - Trait definitions

================================================================================
7. LINE COUNTS (KEY FILES)
================================================================================

Core Files:
- crates/f_engine/src/core/engine_v2.rs:      ~462 lines
- crates/f_engine/src/core/execution.rs:     ~371 lines
- crates/f_engine/src/types.rs:              ~146 lines
- crates/f_engine/src/interfaces/mod.rs:      ~26 lines
- crates/f_engine/src/interfaces/strategy.rs: ~125 lines
- crates/f_engine/src/interfaces/risk.rs:     ~32 lines

Data Files:
- crates/b_data_source/src/api/data_feeder.rs: ~135 lines
- crates/a_common/src/models/market_data.rs:  ~89 lines
- crates/a_common/src/models/dto.rs:           ~239 lines

Indicator Files:
- crates/c_data_process/src/types.rs:         ~297 lines

Check Table:
- crates/d_checktable/src/check_table.rs:     ~107 lines

================================================================================
END OF STRUCTURE DOCUMENT
================================================================================
