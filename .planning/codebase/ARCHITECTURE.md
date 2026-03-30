ARCHITECTURE DOCUMENT
=====================

Layer Dependency Chain
=====================

a_common -> x_data -> b_data_source -> c_data_process -> d_checktable -> e_risk_monitor -> f_engine

The system follows a strict unidirectional data flow through these layers.
Each layer only depends on the layer directly to its left.


Design Patterns
===============

Gateway Pattern
---------------
Used for exchange connectivity abstraction. Concrete implementations:
- BinanceApiGateway: REST API connectivity to Binance
- MockApiGateway: Mock exchange for testing
- BinanceWsConnector: WebSocket connector for real-time data

These gateways abstract the exchange interface from the rest of the system.


Repository Pattern
------------------
MarketDataStore trait defines the repository interface for market data.
Location: b_data_source/src/store/store_trait.rs

All market data access goes through this trait, enabling:
- Real data source (b_data_source)
- Mock data source (b_data_mock)
- Replay data source for backtesting

The trait defines standard CRUD operations for klines, order books, and trades.


Pipeline Pattern
----------------
Event processing pipeline in EventEngine:

tick -> update_store -> calc_indicators -> decide -> risk_check -> place_order

Each stage is a separate processing step:
1. tick: Market data tick arrives
2. update_store: Update internal market data store
3. calc_indicators: Calculate technical indicators
4. decide: Strategy decision logic
5. risk_check: Risk validation
6. place_order: Order execution


Observer Pattern
---------------
EventBus implements Observer pattern via mpsc channel distribution.

Components subscribe to events they care about. When an event is published,
it is distributed to all subscribers via channels. This decouples event
producers from consumers.


Key Architectural Constraints
=============================

a_common MUST NOT contain business types
---------------------------------------
The a_common crate is a shared foundation crate. It must NOT contain:
- TradingDecision
- OrderRequest
- LocalPosition
- Any other business-domain types

Business types belong in x_data (shared domain types) or in the
respective functional crates.


Incremental O(1) Calculations Only
---------------------------------
All hot path calculations must be incremental with O(1) complexity.
No full recalculation allowed on each tick. Examples:
- Indicator updates use incremental formulas
- Position updates use delta calculations

This ensures consistent low-latency processing regardless of data volume.


Hot Path Lock-Free Design
------------------------
The hot path (tick processing) must be lock-free to minimize latency.
Only cold path operations may use parking_lot::RwLock for sharing state.

Hot path characteristics:
- Single producer, multiple consumers
- Lock-free data structures
- No blocking operations


Zero tokio::spawn in Hot Path
-----------------------------
Async spawning is not allowed in the hot path. The hot path:
- Must not allocate or spawn tasks
- Uses synchronous processing within a single async context
- Async/await only for I/O operations that can tolerate latency


Zero Polling (recv().await Blocking)
------------------------------------
The hot path must not use polling-based receive (recv().await).
Instead:
- Use blocking channel receive where latency is bounded
- Design for backpressure handling
- Avoid busy-waiting or wake-up patterns


Dependency Injection
====================

EventEngine is generic over two key traits:

EventEngine<S, G> where:
- S: Strategy trait - defines trading strategy interface
- G: ExchangeGateway trait - defines exchange connectivity interface

This enables:
- Different strategies to be plugged in
- Different exchange gateways (live, mock, replay)
- Easy testing with mock implementations


Version Tracking
================

AtomicU64 versioning system for data lineage tracking:
- data version: Raw market data version
- indicator version: Indicator calculation version
- signal version: Signal generation version
- decision version: Trading decision version

This enables:
- Change detection without full comparison
- Incremental processing when version hasn't changed
- Debugging and tracing of data flow


a_common Re-exports from x_data
===============================

For developer convenience, a_common re-exports certain types from x_data.
This reduces import churn and provides a stable internal API surface.

All re-exports are documented and versioned to prevent breaking changes.