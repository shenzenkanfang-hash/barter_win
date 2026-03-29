================================================================================
STRUCTURE.md - Barter-Rs Directory Layout
================================================================================
Author: Claude Code
Created: 2026-03-29
Status: Complete
================================================================================

1. ROOT DIRECTORY
================================================================================

barter-rs-main/
├── Cargo.toml              # Workspace manifest
├── Cargo.lock
├── CLAUDE.md               # Project instructions
├── rustfmt.toml
├── .gitignore
│
├── crates/                 # All crates (workspace members)
│   ├── a_common/           # Infrastructure layer (no business types)
│   ├── x_data/             # Business data types
│   ├── b_data_source/      # Real market data (WebSocket + REST)
│   ├── b_data_mock/        # Mock data (sandbox/testing)
│   ├── c_data_process/     # Indicators and signal processing
│   ├── d_checktable/       # Strategy check tables
│   ├── e_risk_monitor/     # Risk management
│   ├── f_engine/           # Trading engine core
│   └── g_test/             # Integration tests
│
├── src/                    # Binary crate (main entry)
│   └── main.rs
│
├── .planning/              # Project planning docs
│   ├── PROJECT.md
│   ├── ROADMAP.md
│   ├── milestones/
│   └── codebase/           # This directory
│
├── docs/                   # Design documents
├── data/                   # Runtime data (Linux)
├── deploy/                 # Deployment configs
├── sandbox/                # Sandbox/playground
└── target/                 # Cargo build output


================================================================================
2. CRATES DIRECTORY
================================================================================

crates/
│
├── a_common/               # ===============================================
│   Cargo.toml
│   src/
│   ├── lib.rs              # Module root (forbid unsafe_code)
│   │
│   ├── api/                # REST API gateway
│   │   ├── mod.rs
│   │   ├── binance_api.rs  # BinanceApiGateway, RateLimiter
│   │   └── kline_fetcher.rs # Historical K-line fetching
│   │
│   ├── ws/                 # WebSocket gateway
│   │   ├── mod.rs
│   │   ├── binance_ws.rs   # BinanceWsConnector, BinanceTradeStream
│   │   └── websocket.rs     # WebSocket utilities
│   │
│   ├── config/             # Platform and path configuration
│   │   ├── mod.rs
│   │   ├── platform.rs     # Platform detection (Windows/Linux)
│   │   ├── paths.rs        # Path constants
│   │   └── volatility.rs   # Volatility config
│   │
│   ├── models/             # Data models
│   │   ├── mod.rs
│   │   ├── types.rs        # OrderStatus, etc.
│   │   ├── market_data.rs  # Market data types
│   │   └── dto.rs          # Data transfer objects
│   │
│   ├── backup/             # Memory backup system
│   │   ├── mod.rs
│   │   └── memory_backup.rs # MemoryBackup, AccountSnapshot
│   │
│   ├── exchange/           # Exchange gateway types
│   │   ├── mod.rs
│   │   └── (exchange types)
│   │
│   ├── volatility/         # Volatility calculation
│   │   ├── mod.rs
│   │   └── (volatility types)
│   │
│   ├── claint/             # Error types
│   │   ├── mod.rs
│   │   └── error.rs        # MarketError, EngineError, AppError
│   │
│   ├── logs/               # Checkpoint logging
│   │   ├── mod.rs
│   │   └── checkpoint.rs    # CheckpointLogger
│   │
│   └── util/               # Utilities
│       ├── mod.rs
│       ├── sanitize.rs     # String sanitization
│       └── telegram_notifier.rs
│
│
├── x_data/                 # ===============================================
│   Cargo.toml
│   src/
│   ├── lib.rs              # Module root
│   │
│   ├── position/           # Position types
│   │   ├── mod.rs
│   │   ├── snapshot.rs
│   │   └── types.rs
│   │
│   ├── account/            # Account types
│   │   ├── mod.rs
│   │   ├── pool.rs         # FundPoolManager
│   │   └── types.rs
│   │
│   ├── market/             # Market data types
│   │   ├── mod.rs
│   │   ├── kline.rs
│   │   ├── tick.rs
│   │   ├── orderbook.rs
│   │   └── volatility.rs
│   │
│   ├── trading/            # Trading types
│   │   ├── mod.rs
│   │   ├── signal.rs       # StrategySignal, TradeCommand
│   │   ├── order.rs
│   │   ├── futures.rs
│   │   └── rules.rs
│   │
│   ├── state/              # State management traits
│   │   ├── mod.rs
│   │   └── traits.rs       # StateViewer, StateManager
│   │
│   └── error.rs
│
│
├── b_data_source/          # ===============================================
│   Cargo.toml
│   src/
│   ├── lib.rs              # Module root + DataFeeder re-exports
│   │
│   ├── ws/                 # WebSocket data interfaces
│   │   ├── mod.rs
│   │   ├── kline_1m/       # 1-minute K-line processing
│   │   │   ├── mod.rs
│   │   │   ├── ws.rs        # Kline1mStream
│   │   │   ├── kline.rs    # K-line synthesis
│   │   │   └── kline_persistence.rs
│   │   ├── kline_1d/       # 1-day K-line processing
│   │   │   ├── mod.rs
│   │   │   └── ws.rs
│   │   ├── order_books/     # Order book aggregation
│   │   │   ├── mod.rs
│   │   │   ├── ws.rs
│   │   │   └── orderbook.rs
│   │   └── volatility/     # Volatility detection
│   │       └── mod.rs
│   │
│   ├── api/                # REST API data interfaces
│   │   ├── mod.rs
│   │   ├── data_feeder.rs  # DataFeeder (unified WS+REST)
│   │   ├── account.rs      # Account data
│   │   ├── position.rs     # Position data
│   │   ├── symbol_registry.rs # SymbolRegistry
│   │   ├── trade_settings.rs # TradeSettings
│   │   ├── data_sync.rs    # Data synchronization
│   │   └── symbol_rules.rs # Symbol rules service
│   │
│   ├── store/              # Market data storage
│   │   ├── mod.rs
│   │   ├── store_trait.rs  # MarketDataStore trait
│   │   ├── store_impl.rs   # MarketDataStoreImpl
│   │   ├── memory_store.rs
│   │   ├── history_store.rs
│   │   └── volatility.rs
│   │
│   ├── history/            # Historical data management
│   │   ├── mod.rs
│   │   ├── manager.rs
│   │   ├── provider.rs
│   │   ├── api.rs
│   │   └── types.rs
│   │
│   ├── engine/             # Clock and engine timing
│   │   ├── mod.rs
│   │   ├── clock.rs
│   │   ├── processor.rs
│   │   ├── auditor.rs
│   │   └── run.rs
│   │
│   ├── trader_pool.rs      # Trading pair pool
│   ├── replay_source.rs    # Historical data replay
│   ├── recovery.rs         # Checkpoint recovery
│   ├── models/
│   │   ├── mod.rs
│   │   ├── types.rs
│   │   └── ws.rs
│   │
│   └── examples/
│       ├── mock_ws_handshake.rs
│       └── test_trade_settings.rs
│
│
├── b_data_mock/            # ===============================================
│   # Mirror of b_data_source with mock implementations
│   Cargo.toml
│   src/
│   ├── lib.rs
│   ├── api/                # Mock API (MockApiGateway)
│   ├── ws/                 # Mock WebSocket (simulated data)
│   ├── store/              # In-memory store
│   ├── history/            # Mock history
│   ├── models/
│   ├── symbol_rules/
│   ├── trader_pool.rs
│   ├── replay_source.rs
│   ├── recovery.rs
│   │
│   └── tests/              # Unit tests
│
│
├── c_data_process/         # ===============================================
│   Cargo.toml
│   src/
│   ├── lib.rs              # PineColorDetector, SignalProcessor
│   │
│   ├── pine_indicator_full.rs # Full Pine v5 indicator implementation
│   │                          # EMA, RSI, MACD, colors, etc.
│   │
│   ├── processor.rs        # SignalProcessor (manages calculators)
│   │
│   ├── min/                # Minute-level indicators
│   │   ├── mod.rs
│   │   └── trend.rs        # Indicator1m, Indicator1mOutput
│   │
│   ├── day/                # Day-level indicators
│   │   ├── mod.rs
│   │   └── trend.rs        # BigCycleCalculator
│   │
│   ├── strategy_state/     # Persistent strategy state
│   │   ├── mod.rs
│   │   ├── state.rs
│   │   ├── db.rs           # SQLite persistence
│   │   └── error.rs
│   │
│   └── types.rs
│
│
├── d_checktable/           # ===============================================
│   Cargo.toml
│   src/
│   ├── lib.rs              # CheckTable, CheckEntry, CheckChainContext
│   │
│   ├── check_table.rs      # Core check table logic
│   ├── types.rs
│   │
│   ├── h_15m/              # High-frequency 15-minute checks
│   │   ├── mod.rs
│   │   ├── signal.rs
│   │   ├── status.rs
│   │   ├── quantity_calculator.rs
│   │   ├── executor.rs
│   │   ├── trader.rs
│   │   └── repository.rs
│   │
│   ├── l_1d/               # Low-frequency 1-day checks
│   │   ├── mod.rs
│   │   ├── signal.rs
│   │   ├── status.rs
│   │   └── quantity_calculator.rs
│   │
│   ├── h_volatility_trader/ # Volatility-based trading
│   │   ├── mod.rs
│   │   ├── volatility_ranker.rs
│   │   └── simple_executor.rs
│   │
│   ├── examples/
│   │   └── h_15m_test.rs
│   │
│   └── tests/
│       ├── dt_001_checktable_test.rs
│       ├── dt_002_003_trader_executor_test.rs
│       ├── dt_004_quantity_calculator_test.rs
│       ├── dt_006_007_signal_status_test.rs
│       └── dt_011_check_chain_context_test.rs
│
│
├── e_risk_monitor/         # ===============================================
│   Cargo.toml
│   src/
│   ├── lib.rs              # Re-exports all risk types
│   │
│   ├── risk/               # Risk checking
│   │   ├── mod.rs
│   │   ├── common/         # Common risk checks
│   │   │   ├── mod.rs
│   │   │   ├── risk.rs     # RiskPreChecker
│   │   │   ├── risk_rechecker.rs
│   │   │   ├── order_check.rs
│   │   │   └── thresholds.rs
│   │   ├── pin/            # PIN risk limit
│   │   │   ├── mod.rs
│   │   │   └── pin_risk_limit.rs
│   │   ├── trend/          # Trend risk limit
│   │   │   ├── mod.rs
│   │   │   └── trend_risk_limit.rs
│   │   └── minute_risk.rs  # Minute-level risk
│   │
│   ├── position/           # Position management
│   │   ├── mod.rs
│   │   ├── position_manager.rs # LocalPositionManager
│   │   └── position_exclusion.rs
│   │
│   ├── persistence/        # Persistence and recovery
│   │   ├── mod.rs
│   │   ├── persistence.rs   # PersistenceService
│   │   ├── sqlite_persistence.rs # SqliteEventRecorder
│   │   ├── disaster_recovery.rs # DisasterRecovery
│   │   └── startup_recovery.rs # StartupRecoveryManager
│   │
│   └── shared/            # Shared utilities
│       ├── mod.rs
│       ├── account_pool.rs # AccountPool with circuit breaker
│       ├── margin_config.rs
│       ├── market_status.rs # MarketStatusDetector
│       ├── pnl_manager.rs
│       └── round_guard.rs
│
│
├── f_engine/               # ===============================================
│   Cargo.toml
│   src/
│   ├── lib.rs              # EventEngine, TraderManager exports
│   │
│   ├── core/               # Core engine
│   │   ├── mod.rs
│   │   ├── engine.rs       # EventDrivenEngine
│   │   └── strategy_loop.rs
│   │
│   ├── event/              # Event-driven architecture
│   │   ├── mod.rs
│   │   ├── event_engine.rs # EventEngine (main tick processor)
│   │   ├── event_bus.rs    # EventBus, EventBusHandle
│   │   └── tests.rs
│   │
│   ├── interfaces/         # Trait definitions
│   │   ├── mod.rs
│   │   └── risk.rs        # RiskChecker trait
│   │
│   ├── strategy/          # Strategy management
│   │   ├── mod.rs
│   │   └── trader_manager.rs
│   │
│   └── types.rs           # OrderRequest, TradingDecision, Side, etc.
│
│
└── g_test/                 # ===============================================
    Cargo.toml
    src/
    ├── lib.rs
    │
    ├── b_data_source/      # b_data_source tests
    │   ├── mod.rs
    │   ├── api/
    │   ├── models/
    │   ├── ws/
    │   ├── recovery.rs
    │   └── replay_source_test.rs
    │
    └── strategy/           # Strategy integration tests
        ├── mod.rs
        ├── strategy_executor_test.rs
        ├── trading_integration_test.rs
        └── mock_gateway.rs


================================================================================
3. KEY FILES TABLE
================================================================================

File                                    Layer           Purpose
---------------------------------------- --------------- --------------------------
a_common/src/api/binance_api.rs         a_common        REST API gateway
a_common/src/ws/binance_ws.rs           a_common        WebSocket gateway
a_common/src/backup/memory_backup.rs    a_common        Memory backup system
a_common/src/claint/error.rs            a_common        Error types
a_common/src/config/platform.rs         a_common        Platform detection

x_data/src/market/kline.rs             x_data          KLine type
x_data/src/trading/signal.rs            x_data          StrategySignal
x_data/src/state/traits.rs             x_data          StateManager trait

b_data_source/src/api/data_feeder.rs   b_data_source   Unified data interface
b_data_source/src/ws/kline_1m/ws.rs    b_data_source   1m K-line stream
b_data_source/src/store/store_impl.rs   b_data_source   MarketDataStore impl
b_data_source/src/replay_source.rs      b_data_source   Historical replay

c_data_process/src/pine_indicator_full.rs c_data_process Full Pine v5 indicators
c_data_process/src/processor.rs         c_data_process  Signal processor
c_data_process/src/min/trend.rs         c_data_process  1m indicator calculator
c_data_process/src/day/trend.rs         c_data_process  1d indicator calculator

d_checktable/src/check_table.rs         d_checktable    Check table core
d_checktable/src/h_15m/executor.rs     d_checktable    15m strategy executor

e_risk_monitor/src/risk/common/risk.rs e_risk_monitor  RiskPreChecker
e_risk_monitor/src/position/position_manager.rs e_risk_monitor Position
e_risk_monitor/src/persistence/sqlite_persistence.rs e_risk_monitor SQLite
e_risk_monitor/src/persistence/disaster_recovery.rs e_risk_monitor Recovery

f_engine/src/event/event_engine.rs      f_engine        Main tick processor
f_engine/src/types.rs                  f_engine        Core types (OrderRequest, etc.)


================================================================================
4. f_engine/src/ SUBSTRUCTURE (Detailed)
================================================================================

f_engine/src/
├── lib.rs              # Public exports
│                       # EventEngine, EventBus, TraderManager
│                       # OrderRequest, TradingDecision, Side
│
├── types.rs            # Core trading types
│                       # StrategyId, TradingAction, OrderType
│
├── core/               # Basic engine (deprecated)
│   ├── mod.rs
│   ├── engine.rs       # EventDrivenEngine
│   └── strategy_loop.rs
│
├── event/              # Event-driven engine (recommended)
│   ├── mod.rs
│   ├── event_engine.rs # EventEngine - tick processing pipeline
│   │                   # on_tick -> update_store -> calc_indicators
│   │                   #       -> strategy.decide -> risk_check -> place_order
│   ├── event_bus.rs    # EventBus, EventBusHandle, DEFAULT_CHANNEL_BUFFER
│   └── tests.rs
│
├── interfaces/         # Trait definitions
│   ├── mod.rs
│   └── risk.rs         # RiskChecker trait (for external risk systems)
│
└── strategy/           # Strategy management (deprecated)
    ├── mod.rs
    └── trader_manager.rs


================================================================================
5. FEATURE FLAG CONFIGURATION
================================================================================

b_data_source/b_data_mock switching in Cargo.toml:

[features]
default = ["b_data_source"]
mock = ["b_data_mock"]
b_data_source = []
b_data_mock = []

Usage:
  cargo run              # Uses b_data_source (real market data)
  cargo run --features mock  # Uses b_data_mock (simulated data)


================================================================================
6. TEST STRUCTURE
================================================================================

Unit Tests:
  - Inline #[cfg(test)] modules in each .rs file
  - Example: c_data_process/src/processor.rs has tests at bottom

Integration Tests:
  - crates/g_test/src/ - black-box integration tests
  - crates/d_checktable/tests/ - DT_xxx test files
  - crates/b_data_mock/tests/ - mock data tests

Test Commands:
  cargo test --all              # Run all tests
  cargo test -p c_data_process # Test specific crate
  cargo test --lib             # Library tests only


================================================================================
END OF STRUCTURE.md
================================================================================
