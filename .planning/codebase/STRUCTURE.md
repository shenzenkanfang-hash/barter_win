================================================================================
STRUCTURE.md - Directory Structure and File Locations
================================================================================

Author: Software Architect
Created: 2026-03-26
GSD-Phase: documentation
Status: complete
================================================================================

1. Root Level Structure
================================================================================

D:\Rust项目\barter-rs-main\
  |
  +-- Cargo.toml              : Workspace manifest
  +-- Cargo.lock              : Dependency lock file
  +-- CLAUDE.md               : Project instructions
  +-- .git/                   : Git repository
  |
  +-- crates/                 : All crate modules
  |     a_common/             : Infrastructure layer
  |     x_data/               : Business data abstraction
  |     b_data_source/        : Data layer
  |     c_data_process/       : Signal generation layer
  |     d_checktable/        : Check layer
  |     e_risk_monitor/       : Risk compliance layer
  |     f_engine/             : Engine runtime layer
  |     g_test/              : Test layer
  |     +-- h_sandbox/        : Sandbox layer
  |
  +-- .planning/              : Project planning
  |     +-- codebase/         : This directory
  |           +-- ARCHITECTURE.md
  |           +-- STRUCTURE.md
  |
  +-- Bak_非必要指定不读取老版本/ : Old version backup (do not read)

================================================================================
2. Crates Directory Structure
================================================================================

2.1 a_common/ - Infrastructure Layer
--------------------------------------------------------------------------------
Path: D:\Rust项目\barter-rs-main\crates\a_common\

Purpose: Pure infrastructure (API/WS gateways, config, no business types)

Structure:
a_common/
  +-- src/
  |     +-- lib.rs                 : Crate entry (re-exports all modules)
  |     |
  |     +-- api/                   : REST API gateway
  |     |     +-- mod.rs
  |     |     +-- binance_api.rs   : BinanceApiGateway, RateLimiter
  |     |     +-- symbol_rules.rs   : SymbolRulesFetcher
  |     |     +-- kline_fetcher.rs  : ApiKlineFetcher
  |     |     +-- rate_limiter.rs   : RateLimiter, RateLimit
  |     |
  |     +-- ws/                    : WebSocket gateway
  |     |     +-- mod.rs
  |     |     +-- websocket.rs      : BinanceWsConnector
  |     |     +-- trade_stream.rs   : BinanceTradeStream
  |     |     +-- combined_stream.rs: BinanceCombinedStream
  |     |
  |     +-- config/                : Configuration
  |     |     +-- mod.rs
  |     |     +-- platform.rs       : Platform detection, Paths
  |     |
  |     +-- models/                 : Data models
  |     |     +-- mod.rs
  |     |     +-- types.rs          : OrderStatus, etc.
  |     |     +-- dto.rs            : DTO types
  |     |     +-- market_data.rs    : Market data types
  |     |
  |     +-- logs/                   : Checkpoint logging
  |     |     +-- mod.rs
  |     |     +-- checkpoint.rs     : CheckpointLogger, Stage, StageResult
  |     |
  |     +-- backup/                 : Memory backup
  |     |     +-- mod.rs            : MemoryBackup, paths, constants
  |     |
  |     +-- exchange/               : Exchange types
  |     |     +-- mod.rs            : ExchangeAccount, PositionDirection
  |     |
  |     +-- volatility/             : Volatility calculation
  |     |     +-- mod.rs            : VolatilityCalc, VolatilityStats
  |     |
  |     +-- claint/                 : Error types
  |     |     +-- mod.rs            : MarketError, EngineError, AppError
  |     |
  |     +-- util/                   : Utilities
  |     |     +-- mod.rs
  |     |     +-- telegram_notifier.rs
  |     |
  |     +-- a_int_test/            : Internal tests (private)
  |
  +-- Cargo.toml

Key Files:
  - src/lib.rs : 69 lines, re-exports all infrastructure types


2.2 x_data/ - Business Data Abstraction Layer
--------------------------------------------------------------------------------
Path: D:\Rust项目\barter-rs-main\crates\x_data\

Purpose: Unified business data types (eliminates cross-module duplicates)

Structure:
x_data/
  +-- src/
  |     +-- lib.rs                 : Crate entry
  |     |
  |     +-- position/              : Position types
  |     |     +-- mod.rs           : LocalPosition, PositionSide, PositionSnapshot
  |     |
  |     +-- account/               : Account types
  |     |     +-- mod.rs           : FundPool, FundPoolManager, AccountSnapshot
  |     |
  |     +-- market/                : Market types
  |     |     +-- mod.rs           : Tick, KLine, OrderBook, SymbolVolatility
  |     |
  |     +-- trading/               : Trading types
  |     |     +-- mod.rs
  |     |     +-- signal.rs        : StrategySignal, TradeCommand, StrategyId
  |     |
  |     +-- state/                 : State management traits
  |     |     +-- mod.rs           : StateViewer, StateManager, UnifiedStateView
  |     |
  |     +-- error.rs               : Error types
  |
  +-- Cargo.toml

Key Files:
  - src/lib.rs : 31 lines, defines business type re-exports
  - src/trading/signal.rs : StrategySignal, TradeCommand traits


2.3 b_data_source/ - Data Layer
--------------------------------------------------------------------------------
Path: D:\Rust项目\barter-rs-main\crates\b_data_source\

Purpose: Market data processing, K-line synthesis, order books

Structure:
b_data_source/
  +-- src/
  |     +-- lib.rs                 : Crate entry
  |     |
  |     +-- ws/                    : WebSocket data interface
  |     |     +-- mod.rs           : VolatilityManager, SymbolVolatility
  |     |     +-- kline_1m/        : 1-minute K-line
  |     |     |     +-- mod.rs
  |     |     |     +-- kline_persistence.rs
  |     |     |     +-- kline_synthesizer.rs
  |     |     +-- kline_1d/        : 1-day K-line
  |     |     |     +-- mod.rs
  |     |     |     +-- ws.rs
  |     |     +-- order_books/     : Order book depth
  |     |     |     +-- mod.rs
  |     |     |     +-- orderbook.rs
  |     |     |     +-- ws.rs
  |     |
  |     +-- api/                   : REST API data interface
  |     |     +-- mod.rs
  |     |     +-- account.rs       : FuturesAccount, FuturesAccountData
  |     |     +-- position.rs      : FuturesPosition, FuturesPositionData
  |     |     +-- data_feeder.rs   : DataFeeder (unified interface)
  |     |     +-- data_sync.rs     : FuturesDataSyncer
  |     |     +-- symbol_registry.rs: SymbolRegistry
  |     |     +-- trade_settings.rs: TradeSettings, PositionMode
  |     |
  |     +-- models/                : Business models
  |     |     +-- mod.rs
  |     |     +-- types.rs         : MarketStream, MockMarketStream
  |     |     +-- ws.rs           : KLine, Period, Tick
  |     |
  |     +-- recovery/              : Checkpoint recovery
  |     |     +-- mod.rs
  |     |     +-- checkpoint_manager.rs
  |     |
  |     +-- trader_pool.rs         : TraderPool, SymbolMeta, TradingStatus
  |     +-- replay_source.rs       : ReplaySource, KLineSource
  |     +-- symbol_rules.rs        : SymbolRuleService, ParsedSymbolRules
  |
  +-- examples/
  |     +-- test_trade_settings.rs
  |
  +-- Cargo.toml

Key Files:
  - src/lib.rs : 54 lines
  - src/ws/kline_1m/kline_synthesizer.rs : K-line synthesis logic


2.4 c_data_process/ - Signal Generation Layer
--------------------------------------------------------------------------------
Path: D:\Rust项目\barter-rs-main\crates\c_data_process\

Purpose: Indicator calculation, signal generation, strategy state

Structure:
c_data_process/
  +-- src/
  |     +-- lib.rs
  |     |
  |     +-- pine_indicator_full.rs : Pine v5 indicators (EMA, RSI, PineColor)
  |     |
  |     +-- min/                   : Minute-level strategy
  |     |     +-- mod.rs
  |     |     +-- trend.rs
  |     |
  |     +-- day/                   : Day-level strategy
  |     |     +-- mod.rs
  |     |     +-- trend.rs
  |     |
  |     +-- processor.rs           : SignalProcessor
  |     |
  |     +-- strategy_state/         : Strategy state management
  |           +-- mod.rs
  |           +-- state.rs          : StrategyStateManager
  |           +-- db.rs            : StrategyStateDb
  |           +-- error.rs
  |
  +-- Cargo.toml

Key Files:
  - src/lib.rs : 24 lines
  - src/pine_indicator_full.rs : Full Pine v5 indicator implementation


2.5 d_checktable/ - Check Layer
--------------------------------------------------------------------------------
Path: D:\Rust项目\barter-rs-main\crates\d_checktable\

Purpose: Periodic strategy checks (async concurrent)

Structure:
d_checktable/
  +-- src/
  |     +-- lib.rs
  |     |
  |     +-- check_table.rs         : CheckTable, CheckEntry
  |     +-- types.rs               : CheckChainContext, CheckSignal
  |     |
  |     +-- h_15m/                 : High-frequency 15-minute checks
  |     |     +-- mod.rs
  |     |     +-- min_quantity_calculator.rs
  |     |
  |     +-- l_1d/                  : Low-frequency 1-day checks
  |           +-- mod.rs
  |           +-- day_quantity_calculator.rs
  |
  +-- Cargo.toml

Key Files:
  - src/lib.rs : 18 lines


2.6 e_risk_monitor/ - Risk Compliance Layer
--------------------------------------------------------------------------------
Path: D:\Rust项目\barter-rs-main\crates\e_risk_monitor\

Purpose: Risk control, position management, persistence

Structure:
e_risk_monitor/
  +-- src/
  |     +-- lib.rs
  |     |
  |     +-- risk/                   : Risk management
  |     |     +-- mod.rs
  |     |     +-- common/           : Common risk
  |     |     |     +-- mod.rs
  |     |     |     +-- risk.rs     : RiskPreChecker
  |     |     |     +-- risk_rechecker.rs
  |     |     |     +-- order_check.rs
  |     |     |     +-- thresholds.rs
  |     |     +-- pin/              : Pin risk
  |     |     |     +-- mod.rs
  |     |     |     +-- pin_risk_limit.rs
  |     |     +-- trend/            : Trend risk
  |     |     |     +-- mod.rs
  |     |     |     +-- trend_risk_limit.rs
  |     |     +-- minute_risk.rs    : Minute-level risk calculation
  |     |
  |     +-- position/               : Position management
  |     |     +-- mod.rs
  |     |     +-- position_manager.rs : LocalPosition, LocalPositionManager
  |     |     +-- position_exclusion.rs : PositionExclusionChecker
  |     |
  |     +-- persistence/            : Data persistence
  |     |     +-- mod.rs
  |     |     +-- persistence.rs     : PersistenceService
  |     |     +-- sqlite_persistence.rs : SqliteEventRecorder
  |     |     +-- disaster_recovery.rs : DisasterRecovery
  |     |     +-- startup_recovery.rs : StartupRecoveryManager
  |     |
  |     +-- shared/                 : Shared components
  |           +-- mod.rs
  |           +-- account_pool.rs   : AccountPool, CircuitBreakerState
  |           +-- margin_config.rs  : MarginPoolConfig
  |           +-- pnl_manager.rs    : PnlManager
  |           +-- round_guard.rs    : RoundGuard
  |           +-- market_status.rs  : MarketStatus, MarketStatusDetector
  |
  +-- Cargo.toml

Key Files:
  - src/lib.rs : 23 lines


2.7 f_engine/ - Engine Runtime Layer
--------------------------------------------------------------------------------
Path: D:\Rust项目\barter-rs-main\crates\f_engine\

Purpose: Core trading engine, execution coordination

Structure:
f_engine/
  +-- src/
  |     +-- lib.rs                 : Crate entry (59 lines)
  |     |
  |     +-- core/                  : Core engine components
  |     |     +-- mod.rs           : 55 lines, core exports
  |     |     +-- engine_v2.rs     : TradingEngineV2 (main engine)
  |     |     +-- engine_state.rs  : EngineState, EngineStatus
  |     |     +-- state.rs         : SymbolState, TradeLock
  |     |     +-- strategy_pool.rs : StrategyPool
  |     |     +-- execution.rs     : TradingPipeline
  |     |     +-- triggers.rs     : TriggerManager
  |     |     +-- fund_pool.rs     : FundPoolManager
  |     |     +-- risk_manager.rs  : RiskManager
  |     |     +-- monitoring.rs     : TimeoutMonitor
  |     |     +-- rollback.rs      : RollbackManager
  |     |     +-- business_types.rs : PositionSide, VolatilityTier, etc.
  |     |     +-- tests.rs         : Unit tests
  |     |
  |     +-- interfaces/             : Trait definitions (ONLY traits, no impl)
  |     |     +-- mod.rs           : 26 lines
  |     |     +-- market_data.rs   : MarketDataProvider, MarketKLine
  |     |     +-- strategy.rs      : StrategyExecutor, TradingSignal
  |     |     +-- risk.rs          : RiskChecker, RiskLevel
  |     |     +-- execution.rs     : ExchangeGateway trait
  |     |     +-- check_table.rs   : CheckTableProvider
  |     |     +-- adapters.rs      : Adapter implementations
  |     |
  |     +-- order/                  : Order execution
  |     |     +-- mod.rs
  |     |     +-- gateway.rs        : ExchangeGateway trait
  |     |     +-- order.rs          : OrderExecutor
  |     |     +-- mock_binance_gateway.rs : Mock implementation
  |     |
  |     +-- channel/               : Channel mode switching
  |     |     +-- mod.rs
  |     |     +-- mode_switcher.rs  : ChannelType, mode transitions
  |     |
  |     +-- strategy/              : Strategy components
  |     |     +-- mod.rs
  |     |     +-- executor.rs      : StrategyExecutor
  |     |
  |     +-- types.rs               : Shared types (OrderRequest, Side)
  |
  +-- Cargo.toml

Key Files:
  - src/lib.rs : 59 lines
  - src/core/mod.rs : 55 lines, lists all core submodules
  - src/core/engine_v2.rs : Main TradingEngineV2 implementation
  - src/interfaces/mod.rs : 26 lines, all trait exports

f_engine/src Subdirectory Constraint (MANDATORY):
--------------------------------------------------------------------------------
NEW files MUST be placed in appropriate subdirectories:
  - core/       : Engine core logic (engine, state, execution)
  - order/      : Order execution (gateway, order executor)
  - channel/    : Channel/mode switching
  - strategy/   : Strategy components
  - interfaces/ : Cross-module trait definitions ONLY
  - types.rs    : Shared types across submodules


2.8 g_test/ - Test Layer
--------------------------------------------------------------------------------
Path: D:\Rust项目\barter-rs-main\crates\g_test\

Purpose: Centralized functional tests

Structure:
g_test/
  +-- src/
  |     +-- lib.rs                 : 13 lines
  |     |
  |     +-- b_data_source/         : b_data_source tests
  |     |     +-- mod.rs
  |     |     +-- api/
  |     |     |     +-- mod.rs
  |     |     |     +-- account.rs
  |     |     |     +-- symbol_registry.rs
  |     |     +-- ws/
  |     |     |     +-- mod.rs
  |     |     |     +-- kline.rs
  |     |     |     +-- orderbook.rs
  |     |     +-- models/
  |     |     |     +-- mod.rs
  |     |     |     +-- models.rs
  |     |     +-- recovery.rs
  |     |     +-- trader_pool_test.rs
  |     |
  |     +-- strategy/              : Strategy tests
  |           +-- mod.rs
  |           +-- mock_gateway.rs
  |           +-- strategy_executor_test.rs
  |
  +-- Cargo.toml


2.9 h_sandbox/ - Sandbox Layer
--------------------------------------------------------------------------------
Path: D:\Rust项目\barter-rs-main\crates\h_sandbox\

Purpose: Experimental code, testing new features

Structure:
h_sandbox/
  +-- src/
  |     +-- lib.rs                 : 26 lines
  |     |
  |     +-- config.rs              : ShadowConfig
  |     +-- simulator.rs           : Account, OrderEngine, Position
  |     +-- gateway.rs             : ShadowBinanceGateway
  |     +-- tick_generator.rs      : TickGenerator, KLineInput
  |     +-- perf_test.rs           : Performance testing
  |     +-- backtest.rs            : BacktestStrategy, MaCrossStrategy
  |     +-- historical_replay.rs    : ReplayController, MemoryInjector
  |
  +-- Cargo.toml

================================================================================
3. Naming Conventions
================================================================================

3.1 Crate Naming
--------------------------------------------------------------------------------
  - a_common     : Infrastructure layer (a_ prefix for lowest layer)
  - x_data       : Business abstraction (x_ for cross-cutting)
  - b_data_source : Data layer
  - c_data_process : Signal generation
  - d_checktable  : Check layer
  - e_risk_monitor : Risk layer
  - f_engine      : Engine layer
  - g_test        : Test layer
  - h_sandbox     : Sandbox layer

3.2 Module Naming
--------------------------------------------------------------------------------
  - snake_case for modules and files
  - Example: position_manager.rs, risk_rechecker.rs

3.3 Trait Naming
--------------------------------------------------------------------------------
  - PascalCase traits with _Provider, _Executor, _Checker suffix
  - Examples: MarketDataProvider, StrategyExecutor, RiskChecker

3.4 Type Naming
--------------------------------------------------------------------------------
  - PascalCase for types, enums, structs
  - snake_case for fields and functions
  - Example: struct LocalPositionManager, field position_side

================================================================================
4. Key File Locations
================================================================================

Error Types:
  - a_common/src/claint/mod.rs         : MarketError, EngineError, AppError
  - x_data/src/error.rs                : x_data errors
  - c_data_process/src/strategy_state/error.rs

State Management:
  - x_data/src/state/mod.rs            : StateViewer, StateManager traits
  - f_engine/src/core/engine_state.rs  : EngineState implementation

Trading Engine:
  - f_engine/src/core/engine_v2.rs     : TradingEngineV2 (main entry)
  - f_engine/src/interfaces/mod.rs    : All trait definitions

Persistence:
  - e_risk_monitor/src/persistence/persistence.rs
  - e_risk_monitor/src/persistence/sqlite_persistence.rs
  - e_risk_monitor/src/persistence/disaster_recovery.rs

Data Models:
  - a_common/src/models/types.rs       : OrderStatus, basic types
  - a_common/src/models/market_data.rs : Market data types
  - x_data/src/trading/signal.rs      : StrategySignal, TradeCommand

Indicators:
  - c_data_process/src/pine_indicator_full.rs : Pine v5 indicators

================================================================================
5. Line Counts Summary
================================================================================

Key Files:
  - a_common/src/lib.rs           : 69 lines
  - x_data/src/lib.rs             : 31 lines
  - b_data_source/src/lib.rs      : 54 lines
  - c_data_process/src/lib.rs    : 24 lines
  - d_checktable/src/lib.rs      : 18 lines
  - e_risk_monitor/src/lib.rs    : 23 lines
  - f_engine/src/lib.rs          : 59 lines
  - f_engine/src/core/mod.rs     : 55 lines
  - f_engine/src/interfaces/mod.rs: 26 lines
  - g_test/src/lib.rs            : 13 lines
  - h_sandbox/src/lib.rs         : 26 lines

Total Rust files in crates/: 200+ files

================================================================================
6. Import Path Patterns
================================================================================

Pattern: crate::<module>::<submodule>

Examples:
  - use crate::core::engine_v2::TradingEngineV2;
  - use crate::interfaces::{StrategyExecutor, RiskChecker};
  - use crate::order::OrderExecutor;
  - use a_common::MarketError;
  - use x_data::position::LocalPosition;

Note: f_engine internal imports use crate:: submodules
      External crate imports use full crate path

================================================================================
End of STRUCTURE.md
================================================================================
