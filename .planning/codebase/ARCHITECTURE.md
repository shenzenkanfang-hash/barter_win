================================================================================
ARCHITECTURE - Barter-rs Quantitative Trading System
================================================================================

PROJECT: barter-rs
PATH: D:\Rust项目\barter-rs-main
STATUS: Active Development (Phase 6: Integration)

================================================================================
1. ARCHITECTURAL PATTERN
================================================================================

1.1 Overall Pattern: Layered Hexagonal Architecture
----------------------------------------------------

The system follows a 6-layer vertical slicing architecture with hexagonal
elements at the engine layer. Each layer has clear responsibilities and
communication pathways.

    Upper Layers (Policy)          Lower Layers (Mechanism)
    ======================          =========================
    g_test (Tests)                 a_common (Infrastructure)
    h_sandbox (Experiments)        b_data_source (Data Gateway)
    f_engine (Runtime)              c_data_process (Signal Gen)
    e_risk_monitor (Compliance)    d_checktable (Checks)
    ...

1.2 Key Architectural Principles
----------------------------------------------------

[1] HIGH-FREQUENCY PATHS ARE LOCK-FREE
    - Tick reception, indicator updates, strategy judgment: NO locks
    - Locks only for: order execution, fund updates
    - Pre-checks all risk conditions outside lock scope

[2] INCREMENTAL CALCULATION O(1)
    - EMA, SMA, MACD: Incremental computation
    - K-line: Incremental update current bar

[3] THREE-LAYER INDICATOR SYSTEM
    - TR (True Range): Volatility breakout judgment
    - Pine Color: Trend signals (MACD + EMA10/20 + RSI)
    - Price Position: Cycle extreme judgment

[4] HYBRID POSITION MODE
    - Fund Pool: RwLock protected (low frequency)
    - Strategy Positions: Independent calculation (lock-free)

================================================================================
2. LAYER ARCHITECTURE (6-LAYER + 2)
================================================================================

Layer Dependencies (Top to Bottom):
    g_test --> h_sandbox --> f_engine --> e_risk_monitor --> d_checktable
                                                          --> c_data_process
                                                          --> b_data_source
                                                          --> a_common

--------------------------------------------------------------------------------

2.1 a_common - Infrastructure Layer
================================================================================
PURPOSE: API/WS gateways, error types, configuration, shared models

KEY SUBMODULES:
- api/          BinanceApiGateway, RateLimiter, SymbolRulesFetcher (REST)
- ws/           BinanceTradeStream, BinanceCombinedStream, BinanceWsConnector
- models/       MarketKLine, MarketTick, VolatilityInfo, OrderBookSnapshot
- claint/       EngineError, MarketError (thiserror-based)
- config/       Platform detection, Paths (E:/shm on Windows, /dev/shm on Linux)
- volatility/   VolatilityCalc, VolatilityStats, VolatilityRank
- backup/       MemoryBackup, memory_backup_dir (E:/shm/backup)
- logs/         CheckpointLogger, CompositeCheckpointLogger

KEY PATTERNS:
- Gateway Pattern: BinanceApiGateway for REST, BinanceWsConnector for WS
- Rate Limiting: Token bucket algorithm in BinanceApiGateway
- Platform Abstraction: Platform::detect() for cross-platform paths

INTERFACES (from lib.rs exports):
- BinanceApiGateway, RateLimiter, SymbolRulesFetcher
- BinanceTradeStream, BinanceWsConnector
- VolatilityCalc, VolatilityStats
- EngineError, MarketError

--------------------------------------------------------------------------------

2.2 b_data_source - Data Source Layer
================================================================================
PURPOSE: Data feeding, K-line synthesis, order book, volatility detection

KEY SUBMODULES:
- api/          DataFeeder (unified interface), SymbolRegistry, TradeSettings
- ws/           kline_1m, kline_1d, order_books, volatility
- symbol_rules/ SymbolRuleService, ParsedSymbolRules
- trader_pool/  SymbolMeta, TradingStatus, TraderPool
- replay_source/KLineSource, ReplaySource (historical data replay)

KEY PATTERNS:
- DataFeeder: Unified data access interface (all queries must go through here)
- K-line Synthesis: Real-time 1m/15m/1d bar construction from ticks
- Symbol Registry: Trading pair management

INTERFACES (from lib.rs exports):
- DataFeeder, SymbolRegistry
- KLine, Period, Tick, MarketStream
- VolatilityManager, SymbolVolatility

--------------------------------------------------------------------------------

2.3 c_data_process - Signal Generation Layer
================================================================================
PURPOSE: Indicator calculation, signal generation, strategy state

KEY SUBMODULES:
- pine_indicator_full/  PineColorDetector (Pine v5), EMA, RSI, colors
- min/         Minute-level strategy input/output
- day/         Day-level strategy input/output
- processor/   SignalProcessor
- strategy_state/ StrategyStateManager, StrategyStateDb, PositionState

KEY PATTERNS:
- Pine Color Detector: MACD + EMA10/20 + RSI based trend signals
- Incremental Indicators: EMA, RSI computed incrementally
- Signal Types: LongEntry, ShortEntry, LongHedge, ShortHedge, LongExit, ShortExit

INTERFACES (from lib.rs exports):
- PineColorDetectorV5, EMA, RSI
- SignalProcessor
- StrategyStateManager, PositionState

--------------------------------------------------------------------------------

2.4 d_checktable - Check Layer
================================================================================
PURPOSE: Strategy check tables by period (async concurrent execution)

KEY SUBMODULES:
- check_table/  CheckTable, CheckEntry (FnvHashMap storage)
- h_15m/        High-frequency 15-minute strategy checks
- l_1d/         Low-frequency 1-day strategy checks

KEY PATTERNS:
- CheckEntry: Records strategy judgment results (symbol, strategy_id, period)
- Async Concurrent: Checks execute concurrently, engine layer coordinates
- Period-based: Different strategies for different timeframes

INTERFACES (from lib.rs exports):
- CheckTable, CheckEntry

--------------------------------------------------------------------------------

2.5 e_risk_monitor - Risk Control Layer
================================================================================
PURPOSE: Risk control, position management, persistence

KEY SUBMODULES:
- risk/         common, pin, trend, minute_risk
- position/     LocalPositionManager, PositionExclusionChecker
- persistence/  PersistenceService, SqliteEventRecorder, DisasterRecovery
- shared/       AccountPool, MarketStatusDetector, PnlManager

KEY PATTERNS:
- Two-Level Risk: Pre-check (outside lock) + Fine-check (inside lock)
- Position Exclusion: Prevents conflicting positions
- Disaster Recovery: Memory disk backup + SQLite persistence

INTERFACES (from lib.rs exports):
- RiskPreChecker, RiskReChecker
- LocalPositionManager, PositionExclusionChecker
- PersistenceService, SqliteEventRecorder

--------------------------------------------------------------------------------

2.6 f_engine - Trading Engine Runtime
================================================================================
PURPOSE: Core execution, order management, mode switching

KEY SUBMODULES (7-subdirectory structure):
- core/         TradingEngineV2, EngineState, StrategyPool, State
- order/        OrderExecutor, ExchangeGateway, MockBinanceGateway
- channel/      ModeSwitcher
- strategy/     StrategyExecutor
- interfaces/   Trait definitions for all cross-module communication
- types.rs      Core type definitions

TRADING ENGINE V2 EXECUTION FLOW (V1.4):
    1. Parallel trigger checks (minute-level / day-level)
    2. StrategyQuery + Strategy execution (2s timeout)
    3. Risk Level 1 pre-check (outside lock)
    4. Symbol-level lock acquisition (1s timeout)
    5. Risk Level 2 fine-check (inside lock)
    6. Freeze funds + Place order
    7. Fill confirmation + State sync
    8. Confirm funds / Rollback

INTERFACES (from interfaces/ module):
- MarketDataProvider, MarketKLine, MarketTick, VolatilityInfo
- StrategyExecutor, StrategyInstance, TradingSignal
- RiskChecker, RiskLevel, PositionInfo
- ExchangeGateway
- CheckTableProvider

KEY TYPES:
- TradingEngineV2, TradingEngineConfig
- EngineState, EngineStatus, EngineMode, EngineMetricsSnapshot
- StrategyQuery, StrategyResponse, RiskCheckResult
- OrderInfo, FundPool, OrderLifecycle

--------------------------------------------------------------------------------

2.7 g_test - Test Layer
================================================================================
PURPOSE: Integration tests, functional tests

KEY SUBMODULES:
- b_data_source/   b_data_source related tests
- strategy/        Strategy layer black-box tests

--------------------------------------------------------------------------------

2.8 h_sandbox - Sandbox Layer
================================================================================
PURPOSE: Experimental code, simulation, backtest

KEY SUBMODULES:
- config/       ShadowConfig
- simulator/    Account, OrderEngine, Position, ShadowRiskChecker
- gateway/      ShadowBinanceGateway
- tick_generator/ TickGenerator, TickDriver, SimulatedTick
- perf_test/    PerformanceTracker, PerfTickDriver, EngineDriver
- backtest/     BacktestStrategy, BacktestTick, MaCrossStrategy

================================================================================
3. DATA FLOW
================================================================================

3.1 Real-time Trading Flow
--------------------------

    Market Data (WS)
         |
         v
    b_data_source (DataFeeder)
    - K-line synthesis (1m/15m/1d)
    - Order book aggregation
    - Volatility calculation
         |
         v
    c_data_process (Signal Generation)
    - Pine Color detection
    - EMA/RSI calculation
    - Signal generation
         |
         v
    d_checktable (Strategy Checks)
    - h_15m (high-frequency 15m checks)
    - l_1d (low-frequency 1d checks)
         |
         v
    f_engine (Trading Engine)
    - Trigger checks
    - StrategyQuery (2s timeout)
    - Risk pre-check (outside lock)
    - Lock acquisition (1s timeout)
    - Risk fine-check (inside lock)
    - Fund freeze + Order placement
         |
         v
    e_risk_monitor (Risk Control)
    - Position validation
    - Margin check
    - Circuit breaker
         |
         v
    Exchange Gateway (Mock/Real)
    - Order submission
    - Fill confirmation

3.2 State Synchronization Flow
------------------------------

    Local Position vs Exchange Position
              |
              v
    State Syncer (TradingPipeline)
    - Quantity check
    - Price deviation check (<1%)
    - Force sync to exchange values

================================================================================
4. KEY ABSTRACTIONS / INTERFACES
================================================================================

4.1 Exchange Gateway Trait
--------------------------
Location: f_engine/src/interfaces/execution.rs

Purpose: Abstracts exchange operations (place order, cancel, get fills)

Methods:
- async fn place_order(&self, order: OrderRequest) -> Result<OrderResult, Error>
- async fn cancel_order(&self, order_id: &str) -> Result<(), Error>
- async fn get_fills(&self) -> Result<Vec<Fill>, Error>

Implementations:
- MockBinanceGateway (for simulation)
- (Real exchange gateway to be implemented)

4.2 Market Data Provider Trait
------------------------------
Location: f_engine/src/interfaces/market_data.rs

Purpose: Abstracts market data access

Methods:
- async fn next_tick(&self) -> Option<MarketTick>
- async fn next_completed_kline(&self) -> Option<MarketTick>
- fn current_price(&self, symbol: &str) -> Option<Decimal>
- async fn get_klines(&self, symbol: &str, period: &str) -> Vec<MarketKLine>

4.3 Risk Checker Trait
----------------------
Location: f_engine/src/interfaces/risk.rs

Purpose: Abstracts risk checking logic

Methods:
- fn pre_check(&self, order: &OrderRequest, account: &ExchangeAccount) -> RiskCheckResult
- fn post_check(&self, order: &ExecutedOrder, account: &ExchangeAccount) -> RiskCheckResult
- fn scan(&self, positions: &[PositionInfo], account: &ExchangeAccount) -> Vec<RiskWarning>
- fn thresholds(&self) -> RiskThresholds

4.4 Strategy Executor Trait
----------------------------
Location: f_engine/src/interfaces/strategy.rs

Purpose: Abstracts strategy execution and signal aggregation

Methods:
- fn register(&self, strategy: Arc<dyn StrategyInstance>)
- fn dispatch(&self, bar: &MarketKLine) -> Vec<TradingSignal>
- fn get_signal(&self, symbol: &str) -> Option<TradingSignal>
- fn get_all_states(&self) -> Vec<StrategyState>

================================================================================
5. ENTRY POINTS
================================================================================

5.1 Main Binary Entry
---------------------
Location: src/main.rs

Purpose: Program entry, tracing initialization

5.2 Library Entry Points
-------------------------
Each crate exposes its public API via lib.rs re-exports:
- a_common::lib.rs: Infrastructure components
- b_data_source::lib.rs: Data feeding interfaces
- c_data_process::lib.rs: Indicator and signal types
- d_checktable::lib.rs: Check table functionality
- e_risk_monitor::lib.rs: Risk and position management
- f_engine::lib.rs: Trading engine core (main entry)

================================================================================
6. ERROR HANDLING PATTERN
================================================================================

6.1 Error Type Hierarchy
-------------------------
Location: a_common/src/claint/error.rs

- MarketError: Data source errors (WS disconnect, API failure)
- EngineError: Trading engine errors (timeout, lock failure)

6.2 TradingError Enum
---------------------
Location: f_engine/src/core/engine_v2.rs

Variants:
- EngineNotRunning
- InsufficientFunds
- RiskRejected(String)
- LockFailed
- OrderFailed(String)
- Timeout(String)
- StateInconsistent

================================================================================
7. KEY TECHNICAL DECISIONS
================================================================================

[1] NO UNSAFE CODE
    All crates use #![forbid(unsafe_code)]

[2] PARKING_LOT RWLOCK
    Used instead of std::sync::RwLock for better performance

[3] FNVHASHMAP
    O(1) lookup for position/state management

[4] RUST_DECIMAL
    Financial calculations avoid floating point precision issues

[5] CHRONO DATETIME<UTC>
    All timestamps use UTC

[6] THISERROR
    Structured error types with derive(Error)

[7] SERDE
    All types derive Serialize/Deserialize for persistence

================================================================================
END OF ARCHITECTURE DOCUMENT
================================================================================
