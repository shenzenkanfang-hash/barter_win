================================================================================
ARCHITECTURE.md - Barter-Rs Layered Architecture
================================================================================
Author: Claude Code
Created: 2026-03-29
Status: Complete
================================================================================

1. LAYERED ARCHITECTURE OVERVIEW
================================================================================

The system follows a strict layered architecture where each layer has specific
responsibilities and can only depend on layers below it.

Layer Dependency Chain (bottom to top):
--------------------------------------------------------------------------------
  a_common (Infrastructure)
       |
       v
  x_data (Business Data Types)
       |
       v
  b_data_source / b_data_mock (Data Acquisition)
       |
       v
  c_data_process (Indicators & Signals)
       |
       v
  d_checktable (Strategy Decision Tables)
       |
       v
  e_risk_monitor (Risk Management)
       |
       v
  f_engine (Trading Engine)
       |
       v
  g_test (Integration Tests)


--------------------------------------------------------------------------------
Layer Responsibilities:
--------------------------------------------------------------------------------

a_common (Infrastructure Layer)
  - No business type dependencies
  - Pure infrastructure: API/WS gateways, configuration, errors, backup
  - Provides: BinanceApiGateway, BinanceWsConnector, MemoryBackup, RateLimiter
  - Re-exports types from x_data for convenience

x_data (Business Data Abstraction Layer)
  - Unified business data types: Tick, KLine, Position, Account
  - State management traits: StateViewer, StateManager
  - Eliminates cross-module type duplication

b_data_source (Real Market Data)
  - DataFeeder: Unifies WS/REST data interfaces
  - K-line synthesis (1m, 1d)
  - Order book aggregation
  - Volatility detection
  - SymbolRegistry for trading pair management
  - ReplaySource for historical data playback

b_data_mock (Mock/Sandbox Data)
  - Mirror of b_data_source with simulated data
  - Feature flag switch: `cargo run --features mock`

c_data_process (Indicator & Signal Processing)
  - PineIndicator: Full Pine Script v5 indicators
  - SignalProcessor: Manages 1m and 1d indicator calculators
  - StrategyState: Persistent strategy state with SQLite

d_checktable (Strategy Check Tables)
  - h_15m: High-frequency 15-minute strategy checks
  - l_1d: Low-frequency 1-day strategy checks
  - h_volatility_trader: Volatility-based auto trader

e_risk_monitor (Risk Management)
  - RiskPreChecker: Pre-order risk validation
  - PositionManager: Local position tracking
  - DisasterRecovery: SQLite persistence + memory backup
  - Shared: AccountPool, PnlManager, MarginConfig

f_engine (Trading Engine Core)
  - EventEngine: Event-driven tick processing (zero polling)
  - Pipeline: on_tick -> update_store -> calc_indicators -> decide -> risk_check -> place_order
  - Interfaces: RiskChecker trait, ExchangeGateway trait

g_test (Integration Tests)
  - b_data_source tests
  - Strategy black-box tests


================================================================================
2. DESIGN PATTERNS
================================================================================

2.1 Gateway Pattern
--------------------------------------------------------------------------------
All external exchange communication goes through gateway interfaces:

  a_common::api::BinanceApiGateway
    - REST API client for Binance
    - fetch_symbol_rules(), fetch_account_info(), fetch_position_risk()
    - fetch_klines() for historical data
    - RateLimiter with REQUEST_WEIGHT and ORDERS limits

  a_common::ws::BinanceWsConnector
    - WebSocket client for Binance streams
    - Trade stream (@trade), Kline stream (@kline_1m), Depth stream (@depth)
    - Exponential backoff reconnection
    - Returns raw JSON messages (business conversion in b_data_source)

  b_data_mock::api::MockApiGateway
    - Mock implementation for sandbox testing


2.2 Repository Pattern
--------------------------------------------------------------------------------
Data access is abstracted through repository interfaces:

  b_data_source::store::MarketDataStore (trait)
    - write_kline(), get_current_kline()
    - get_volatility(), get_orderbook()
    - Memory + disk persistence

  b_data_source::history::HistoryDataManager (trait)
    - HistoryDataProvider trait for backtesting
    - KLineSource for replay

  d_checktable::h_15m::repository::CheckTableRepository
    - Persists check table state


2.3 Pipeline Pattern
--------------------------------------------------------------------------------
Event-driven tick processing pipeline (f_engine::event::EventEngine):

  Tick Event
      │
      v
  [1] update_store()      -> Write to MarketDataStore
      │
      v
  [2] calc_indicators()   -> O(1) incremental EMA/RSI/volatility
      │
      v
  [3] strategy.decide()   -> Generate TradingDecision
      │
      v
  [4] risk_checker.pre_check() -> RiskPreChecker validation
      │
      v
  [5] gateway.place_order() -> Submit to exchange

  Design Principles:
    - Zero tokio::spawn (fully synchronous await)
    - Zero tokio::sleep (event-driven, no polling)
    - Zero data races (single-threaded serial processing)


2.4 Observer Pattern
--------------------------------------------------------------------------------
  f_engine::event::EventBus
    - mpsc::Channel for tick event distribution
    - EventBusHandle for subscribers

  b_data_source::engine::Clock
    - Clock updates trigger downstream calculations


================================================================================
3. DATA FLOW
================================================================================

3.1 Real-time Tick Flow
--------------------------------------------------------------------------------

Binance WebSocket
      │
      │ raw JSON (@trade, @kline_1m)
      v
a_common::ws::BinanceWsConnector
      │ parses to BinanceTradeMsg, BinanceKlineMsg
      v
b_data_source::api::DataFeeder
      │ converts to Tick, KLine, updates MarketDataStore
      v
b_data_source::ws::kline_1m::KlinePersistence
      │ synthesizes 1m K-lines from trades
      v
c_data_process::SignalProcessor
      │ min_update() / day_update() with OHLCV data
      v
c_data_process::min::trend::Indicator1m
      │ calculates: tr_ratio, velocity, zscore, pine_color
      v
c_data_process::day::trend::BigCycleCalculator
      │ calculates: tr_ratio_5d_20d, pine_color_100_200
      v
f_engine::EventEngine::on_tick()
      │ receives tick events via mpsc channel
      v
d_checktable (h_15m / l_1d) CheckTable
      │ executes trading strategy decision tables
      v
e_risk_monitor::RiskPreChecker
      │ validates: max_position, lot_size, price_deviation
      v
e_risk_monitor::PositionManager
      │ updates local position state
      v
a_common::api::BinanceApiGateway (or MockApiGateway)
      │ place_order() via REST API
      v
Binance Exchange


3.2 Historical Data Replay Flow
--------------------------------------------------------------------------------

b_data_source::replay_source::ReplaySource
      │ reads CSV/JSON historical data
      v
c_data_process (indicator calculators)
      │ warm-up with historical bars
      v
f_engine::EventEngine (backtest mode)
      │ simulates tick-by-tick processing


3.3 Persistence & Recovery Flow
--------------------------------------------------------------------------------

Normal Operation:
  e_risk_monitor::SqliteEventRecorder -> SQLite (E:/backup/trading_events.db)
  e_risk_monitor::MemoryBackup -> E:/shm/backup/ (high-speed memory disk)

Recovery on Startup:
  e_risk_monitor::StartupRecoveryManager
      │
      ├── SqliteRecoverySource (hard disk)
      ├── MemoryDiskRecoverySource (E:/shm/backup/)
      └── HardDiskRecoverySource (E:/backup/)
      │
      v
  UnifiedAccountSnapshot, UnifiedPositionSnapshot


================================================================================
4. KEY ARCHITECTURAL CONSTRAINTS
================================================================================

4.1 No Business Types in a_common
--------------------------------------------------------------------------------
  a_common MUST NOT contain:
    - TradingDecision, OrderRequest (f_engine types)
    - CheckSignal, CheckChainResult (d_checktable types)
    - LocalPosition, PositionSide (e_risk_monitor types)

  a_common CAN contain:
    - MarketError, EngineError (pure infrastructure errors)
    - SymbolRulesData (exchange-agnostic trading rules)
    - MemoryBackup types (backup infrastructure)


4.2 Incremental O(1) Calculations
--------------------------------------------------------------------------------
  All indicators MUST support incremental updates:
    - EMA: new_ema = price * multiplier + prev_ema * (1 - multiplier)
    - RSI: avg_gain/loss updated with exponential moving average
    - K-line: update current bar in-place (no rebuild)


4.3 High-Frequency Path Lock-Free
--------------------------------------------------------------------------------
  Tick reception, indicator updates, strategy decisions: LOCK-FREE
  Order execution, position update: RwLock protected

  Lock order: 1. PositionManager (parking_lot::RwLock)
              2. AccountPool (parking_lot::RwLock)


4.4 Event-Driven (No Polling)
--------------------------------------------------------------------------------
  Engine event loop uses recv().await (blocking channel receive)
  No tokio::spawn for background polling
  No tokio::time::interval for periodic checks


================================================================================
5. DEPENDENCY INJECTION
================================================================================

f_engine::EventEngine<S, G> takes generic parameters:
  - S: Strategy (implements Strategy trait)
  - G: ExchangeGateway (implements ExchangeGateway trait)

This enables:
  - Real trading: MockApiGateway or real BinanceApiGateway
  - Backtesting: replay source feeds ticks directly
  - Unit testing: mock strategy and gateway

Example:
  let engine = EventEngine::new(
      config,
      risk_checker,
      my_strategy,
      BinanceApiGateway::new_futures(),
  );


================================================================================
6. STORAGE ARCHITECTURE
================================================================================

Platform-Aware Path Selection:
  Platform::detect() -> Windows (E:/) or Linux (/dev/shm/)

Primary Storage (High-Speed Memory Disk):
  E:/shm/backup/ (Windows)
  /dev/shm/backup/ (Linux)

Secondary Storage (Hard Disk):
  E:/backup/trading_events.db (Windows)
  data/trading_events.db (Linux)

Backup Contents:
  - KLINE_1M_REALTIME_DIR, KLINE_1D_REALTIME_DIR
  - INDICATORS_1M_REALTIME_DIR, INDICATORS_1D_REALTIME_DIR
  - DEPTH_DIR (order book snapshots)
  - POSITIONS_FILE
  - SYSTEM_CONFIG_FILE (rate limits state)
  - TASKS_DIR (pending orders)


================================================================================
END OF ARCHITECTURE.md
================================================================================
