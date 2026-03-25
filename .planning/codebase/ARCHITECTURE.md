================================================================================
ARCHITECTURE.md - System Architecture Documentation
================================================================================

Author: Software Architect
Created: 2026-03-26
GSD-Phase: documentation
Status: complete
================================================================================

1. Overview
================================================================================

The barter-rs trading system is a high-performance quantitative trading platform
built in Rust, implementing a 6-layer architecture that processes market data
through indicators and risk checks to execution.

Architecture Layers (Bottom to Top):
--------------------------------------------------------------------------------
a_common (Infrastructure) --> b_data_source (Data) --> c_data_process (Signal)
        --> d_checktable (Check) --> e_risk_monitor (Risk) --> f_engine (Engine)

================================================================================
2. Layer-by-Layer Architecture
================================================================================

2.1 a_common - Infrastructure Layer
--------------------------------------------------------------------------------
Purpose: Pure infrastructure components with no business logic dependencies

Modules:
  - api/          : Binance REST API gateway, rate limiter, symbol rules fetcher
  - ws/           : WebSocket connector, trade stream, combined stream
  - config/       : Platform detection (Windows/Linux), path management
  - logs/         : Checkpoint logging (CompositeCheckpointLogger)
  - models/       : DTO types, market data types, order types
  - exchange/     : Exchange gateway types (ExchangeAccount, ExchangePosition)
  - volatility/   : Volatility calculation (VolatilityCalc, VolatilityStats)
  - backup/       : Memory backup system for high-speed data persistence
  - claint/       : Error types (MarketError, EngineError, AppError)

Key Design:
  - No business types (Position, Fund, OrderRequest) - only infrastructure
  - All re-exports go through x_data for business types
  - Binance gateway pattern: pure message passing, no business logic


2.2 x_data - Business Data Abstraction Layer
--------------------------------------------------------------------------------
Purpose: Unified business data types, eliminating cross-module duplicate definitions

Modules:
  - position/     : PositionSide, LocalPosition, PositionSnapshot
  - account/      : FundPool, FundPoolManager, AccountSnapshot
  - market/       : Tick, KLine, OrderBook, SymbolVolatility
  - trading/      : SymbolRulesData, OrderResult, OrderRecord
  - state/        : StateViewer, StateManager, UnifiedStateView traits

Architecture Position:
  - a_common (pure infrastructure) <- x_data (business data) <- business layer

Key Design:
  - All business types defined once in x_data
  - State management traits for uniform state access
  - Eliminates circular dependencies between business crates


2.3 b_data_source - Data Layer
--------------------------------------------------------------------------------
Purpose: Market data processing - data subscription, K-line synthesis, order books

Modules:
  - ws/           : WebSocket data interface
    - kline_1m/   : 1-minute K-line synthesis and persistence
    - kline_1d/   : 1-day K-line stream
    - order_books/: Order book depth streaming
    - volatility/ : Volatility manager per symbol
  - api/          : REST API data interface
    - account/     : Futures account data
    - position/    : Futures position data
    - data_feeder/ : DataFeeder (unified data interface)
    - data_sync/  : Futures data synchronization
    - symbol_registry/: SymbolRegistry
    - trade_settings/: TradeSettings, PositionMode
  - models/       : MarketStream, MockMarketStream, KLine, Period, Tick
  - recovery/     : CheckpointManager, RedisRecovery
  - trader_pool/  : SymbolMeta, TradingStatus, TraderPool
  - replay_source/ : KLineSource, ReplaySource for historical replay

Key Design:
  - Data subscription abstraction via MarketStream trait
  - K-line synthesis with configurable periods
  - Volatility tracking per symbol


2.4 c_data_process - Signal Generation Layer
--------------------------------------------------------------------------------
Purpose: Indicator calculation, signal generation, strategy state management

Modules:
  - pine_indicator_full/ : Pine v5 indicators (EMA, RSI, PineColor)
  - min/         : Minute-level strategy processing
  - day/         : Day-level strategy processing
  - processor/   : SignalProcessor
  - strategy_state/: StrategyStateManager, StrategyStateDb

Key Types:
  - PineColorDetector (V5) : Trend detection using MACD + EMA10/20 + RSI
  - EMA, RSI : Standard technical indicators (incremental calculation)
  - SignalProcessor : Converts indicators to trading signals

Architecture:
  - Indicator System: TR (True Range) -> Pine Color -> Price Position
  - Incremental O(1) calculation for all indicators
  - K-line incremental update for current bar


2.5 d_checktable - Check Layer
--------------------------------------------------------------------------------
Purpose: Periodic strategy checks organized by frequency (async concurrent)

Modules:
  - h_15m/       : High-frequency 15-minute strategy checks
  - l_1d/        : Low-frequency 1-day strategy checks
  - check_table/ : CheckTable, CheckEntry
  - types/       : CheckChainContext, CheckSignal, CheckChainResult

Key Design:
  - CheckTable aggregates checks per period
  - Async concurrent execution
  - Scheduled by engine layer (f_engine)


2.6 e_risk_monitor - Risk Compliance Layer
--------------------------------------------------------------------------------
Purpose: Exchange hard rules, risk control, position management

Modules:
  - risk/         : Risk management
    - common/     : RiskPreChecker, RiskReChecker, OrderCheck, Thresholds
    - pin/        : PinRiskLeverageGuard, PinVolatilityLevel
    - trend/      : TrendRiskLimitGuard, TrendSymbolLimit, TrendGlobalLimit
    - minute_risk/: calculate_hour_open_notional, calculate_minute_open_notional
  - position/     : LocalPositionManager, PositionExclusionChecker
  - persistence/  : PersistenceService, SqliteEventRecorder, DisasterRecovery
  - shared/       : AccountPool, MarginConfig, PnlManager, RoundGuard

Key Design:
  - Hybrid position mode:
    - Fund pool protected by RwLock (low frequency)
    - Strategy positions calculated independently (lock-free)
  - Two-level risk checking:
    - Pre-check (outside lock)
    - Fine-tuning (inside lock)


2.7 f_engine - Engine Runtime Layer
--------------------------------------------------------------------------------
Purpose: Core execution engine, coordinating all layers

Submodules (f_engine/src/):
  - core/         : Core engine components
    - engine_v2/   : TradingEngineV2 (main engine, V1.4 implementation)
    - engine_state/: EngineState, EngineStatus, CircuitBreaker
    - strategy_pool/: StrategyPool, StrategyAllocation
    - state/       : SymbolState, SymbolMetrics, TradeLock
    - execution/   : TradingPipeline, OrderExecutor trait
    - triggers/    : TriggerManager (parallel triggers)
    - fund_pool/   : FundPoolManager
    - risk_manager/: RiskManager, RiskConfig
    - monitoring/  : TimeoutMonitor
    - rollback/    : RollbackManager
  - interfaces/   : Cross-module interaction interfaces (traits only)
    - market_data/ : MarketDataProvider, MarketKLine, MarketTick
    - strategy/    : StrategyExecutor, StrategyInstance, TradingSignal
    - risk/        : RiskChecker, RiskLevel, PositionInfo
    - execution/   : ExchangeGateway
    - check_table/ : CheckTableProvider
  - order/        : Order execution
    - gateway.rs   : ExchangeGateway trait
    - order.rs     : OrderExecutor
    - mock_binance_gateway.rs : Mock implementation
  - channel/      : Channel mode switching
    - mode_switcher.rs : ChannelType, mode transitions
  - types.rs      : Shared types (OrderRequest, StrategyId, Side, OrderType)

Execution Flow (V1.4):
--------------------------------------------------------------------------------
1. Parallel triggers (minute-level / day-level)
2. StrategyQuery + Strategy execution (2s timeout)
3. Risk pre-check (outside lock)
4. Symbol-level lock acquisition (1s timeout)
5. Risk fine-tuning (inside lock)
6. Freeze funds + Place order
7. Trade confirmation + State alignment
8. Confirm funds / Rollback

Key Design:
  - High frequency path lock-free (tick, indicators, strategy)
  - Lock only for order placement and fund updates
  - All risk checks pre-validated outside lock


2.8 g_test - Test Layer
--------------------------------------------------------------------------------
Purpose: Centralized functional tests for all crates

Modules:
  - b_data_source/ : b_data_source related tests
    - api/         : API tests
    - ws/          : WebSocket tests
    - models/      : Model tests
    - recovery/    : Recovery tests
    - trader_pool_test.rs
  - strategy/      : Strategy layer black-box tests
    - mock_gateway.rs
    - strategy_executor_test.rs


2.9 h_sandbox - Sandbox Layer
--------------------------------------------------------------------------------
Purpose: Experimental code, testing new features

Modules:
  - config/       : ShadowConfig
  - simulator/    : Account, OrderEngine, Position, ShadowRiskChecker
  - gateway/      : ShadowBinanceGateway
  - tick_generator/: TickGenerator, SimulatedTick, KLineInput
  - perf_test/   : Performance testing
  - backtest/    : BacktestStrategy, MaCrossStrategy
  - historical_replay/: StreamTickGenerator, MemoryInjector, ReplayController

================================================================================
3. Data Flow
================================================================================

Market Data Ingestion:
--------------------------------------------------------------------------------
Binance WS/API --> a_common (raw messages) --> b_data_source (business models)
    --> c_data_process (indicators) --> Trading Signals

Order Execution:
--------------------------------------------------------------------------------
TradingEngineV2 (f_engine)
    --> d_checktable (check) --> e_risk_monitor (risk) --> Exchange Gateway
    --> b_data_source (state update) --> e_risk_monitor (position update)

State Management:
--------------------------------------------------------------------------------
x_data::StateViewer + StateManager traits
    --> UnifiedStateView for system snapshot
    --> e_risk_monitor (position persistence)

================================================================================
4. Key Design Patterns
================================================================================

4.1 Trait-Based Interfaces (f_engine/src/interfaces/)
--------------------------------------------------------------------------------
All cross-module communication through trait interfaces:
  - MarketDataProvider: Market data access
  - StrategyExecutor: Strategy execution
  - RiskChecker: Risk validation
  - ExchangeGateway: Order placement
  - CheckTableProvider: Check table access

4.2 Incremental Calculation (O(1))
--------------------------------------------------------------------------------
- EMA, SMA, MACD, RSI all use incremental algorithms
- K-line updates only modify current bar
- No full recalculation on new data

4.3 Hybrid Position Mode
--------------------------------------------------------------------------------
- Fund pool: RwLock protected (low frequency)
- Strategy positions: Independent calculation (lock-free)
- Prevents lock contention in high-frequency path

4.4 Two-Level Risk Checking
--------------------------------------------------------------------------------
- Level 1: Pre-check outside lock (fast rejection)
- Level 2: Fine-tuning inside lock (precise validation)

4.5 Checkpoint Logging
--------------------------------------------------------------------------------
- CompositeCheckpointLogger: Multiple loggers combined
- ConsoleCheckpointLogger: Development debugging
- TracingCheckpointLogger: Production tracing
- Stage-based progress tracking

================================================================================
5. Technical Stack
================================================================================

| Component      | Technology          | Purpose                    |
|----------------|---------------------|----------------------------|
| Runtime        | Tokio               | Async IO, multi-thread     |
| State          | FnvHashMap          | O(1) lookup                |
| Sync           | parking_lot         | Efficient RwLock           |
| Decimal        | rust_decimal        | Financial precision        |
| Time           | chrono              | DateTime<Utc>              |
| Error          | thiserror           | Error type hierarchy       |
| Logging        | tracing             | Structured logging         |
| Serialization  | serde               | Serialize/Deserialize      |
| Database       | rusqlite 0.32       | Event persistence          |

================================================================================
6. Directory Structure Summary
================================================================================

crates/
  a_common/      - Infrastructure layer (API/WS, no business types)
  x_data/        - Business data abstraction (position, account, market, trading)
  b_data_source/ - Data layer (DataFeeder, K-line, order book)
  c_data_process/ - Signal layer (indicators, signals)
  d_checktable/  - Check layer (periodic checks)
  e_risk_monitor/ - Risk layer (risk control, position)
  f_engine/      - Engine layer (core execution)
  g_test/        - Test layer (integration tests)
  h_sandbox/     - Sandbox (experimental)

f_engine/src/
  core/          - Engine core (engine_v2, state, triggers, execution)
  interfaces/    - Trait definitions for cross-module communication
  order/         - Order execution (gateway, order executor)
  channel/       - Channel mode switching
  types.rs       - Shared types
  lib.rs         - Library entry point

================================================================================
End of ARCHITECTURE.md
================================================================================
