PROJECT STRUCTURE
=================

Top Level
=========

Trading System v5.5

Cargo.toml          - Workspace manifest defining all crates
src/main.rs         - Single entry point for the trading system
crates/             - All crate source code directory


Crate Structure
===============

a_common
--------
Shared foundation crate containing common utilities.

Key modules:
- api/          - Common API utilities and types
- ws/           - WebSocket common infrastructure
- config/       - Configuration loading and management
- backup/       - Backup and recovery utilities
- heartbeat/    - Heartbeat monitoring
- claint/       - Client interface abstractions
- volatility/   - Volatility calculations
- logs/         - Structured logging utilities
- models/       - Common data models
- exchange/     - Exchange-related common types


b_data_source
--------------
Market data source implementations for real exchange data.

Key modules:
- api/                    - REST API client implementations
- ws/kline_1m/           - WebSocket 1-minute kline data
- ws/kline_1d/           - WebSocket 1-day kline data
- ws/order_books/        - WebSocket order book data
- store/                 - Core market data store
  - store_trait.rs       - MarketDataStore trait definition (KEY TRAIT LOCATION)
- replay_source.rs       - Historical data replay implementation


b_data_mock
-----------
Mock market data source mirroring b_data_source structure.

Contains mock implementations for testing without real exchange connectivity.
Same module structure as b_data_source but with simulated/mock data.


c_data_process
--------------
Data processing and indicator calculation.

Key modules:
- pine_indicator_full.rs - Full Pine script indicator implementation
- processor.rs          - Data processor core
- min/                  - Minute-level processing
- day/                  - Day-level processing
- strategy_state/       - Strategy state management


d_checktable
------------
Trading check table and validation logic.

Key modules:
- h_15m/                - 15-minute timeframe trading checks
- l_1d/                 - 1-day timeframe trading checks
- h_volatility_trader/  - High volatility trading strategy

Key type:
- StoreRef type alias at d_checktable/src/h_15m/trader.rs


e_risk_monitor
--------------
Risk monitoring and control systems.

Key modules:
- risk/common/         - Common risk utilities
- risk/pin/            - Pin risk monitoring
- risk/trend/          - Trend risk monitoring
- position/            - Position risk management
- persistence/         - Risk state persistence
- shared/              - Shared risk components


f_engine
--------
Trading engine core implementation.

Key modules:
- event/               - Event processing engine
  - event_engine.rs    - EventEngine definition
                        - KEY TRAIT LOCATION: Strategy trait
                        - KEY TRAIT LOCATION: ExchangeGateway trait
- interfaces/         - Interface definitions
- core/                - Engine core implementation


x_data
------
Shared domain types and data structures.

Key modules:
- position/            - Position domain types
- account/            - Account domain types
- market/             - Market data domain types
- trading/            - Trading domain types
- state/              - State management types


Key Trait Locations
==================

MarketDataStore trait
b_data_source/src/store/store_trait.rs

Defines the repository interface for market data operations.
All market data access is abstracted behind this trait.


Strategy trait
f_engine/src/event/event_engine.rs

Defines the interface for trading strategies.
Implementations provide the decide() method for signal generation.


ExchangeGateway trait
f_engine/src/event/event_engine.rs

Defines the interface for exchange connectivity.
Implementations handle order placement, cancellation, and market data retrieval.


Single Entry Point
==================

src/main.rs

Trading System v5.5

All system initialization and event loop starts here.
No other entry points should be used for running the trading system.


StoreRef Type Alias
===================

d_checktable/src/h_15m/trader.rs

StoreRef is a type alias for referencing the market data store.
Used throughout the checktable layer to access market data.