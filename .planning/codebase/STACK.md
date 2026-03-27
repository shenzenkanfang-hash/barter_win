================================================================================
STACK.md - Technology Stack and Dependencies
================================================================================

Language
--------------------------------------------------------------------------------
- Rust (Edition 2024 for most crates, Edition 2021 for f_engine and x_data)

Runtime & Concurrency
--------------------------------------------------------------------------------
- Tokio 1.x - Async runtime with full features
  - Used in: a_common, b_data_source, c_data_process, d_checktable, e_risk_monitor, f_engine, g_test, mock 组件

Numeric Computing
--------------------------------------------------------------------------------
- rust_decimal 1.36 - Financial precision decimal arithmetic (maths feature enabled)
- rust_decimal_macros 1.36 - Macro support for decimal literals

Time Handling
--------------------------------------------------------------------------------
- chrono 0.4 - DateTime<Utc> with serde support

Serialization
--------------------------------------------------------------------------------
- serde 1.0 - Serialize/Deserialize derive macros
- serde_json 1.0 - JSON format support

Error Handling
--------------------------------------------------------------------------------
- thiserror 2.0 - Error type derivation with thiserror::Error

Synchronization
--------------------------------------------------------------------------------
- parking_lot 0.12 - More efficient than std::sync::RwLock
- Used in: a_common, b_data_source, c_data_process, d_checktable, e_risk_monitor, f_engine, x_data

Hash Maps
--------------------------------------------------------------------------------
- fnv 1.0 - FnvHashMap for O(1) lookups in hot paths

HTTP & WebSocket
--------------------------------------------------------------------------------
- reqwest 0.12 - HTTP client with json and blocking features
- tokio-tungstenite 0.24/0.26 - WebSocket client with native-tls
- native-tls 0.2 - TLS support
- futures-util 0.3 - Stream/Sink utilities for async I/O

Database
--------------------------------------------------------------------------------
- rusqlite 0.32 (bundled) - SQLite with bundled SQLite engine
  - Used in: c_data_process (strategy_state/db.rs), e_risk_monitor (persistence)
- redis 0.27 - Redis client with tokio-comp and connection-manager
  - Used in: b_data_source

Data Formats
--------------------------------------------------------------------------------
- csv 1.1 - CSV parsing/writing
- parquet 56 - Apache Parquet format (mock 组件 only, default-features: false, snap compression)

CLI & Configuration
--------------------------------------------------------------------------------
- clap 4.4 - Command-line argument parsing with derive feature
- tracing 0.1 - Structured logging (info!/warn!/error!)
- tracing-subscriber 0.3 - Logging subscriber setup

Async Traits
--------------------------------------------------------------------------------
- async-trait 0.1 - Async method support for traits

Testing
--------------------------------------------------------------------------------
- tempfile 3.10 - Temporary file/directory for tests

Workspace Dependencies (from workspace Cargo.toml)
--------------------------------------------------------------------------------
parking_lot = "0.12"
rust_decimal = { version = "1.36", features = ["maths"] }
rust_decimal_macros = "1.36"
thiserror = "2.0"
tracing = "0.1"
tracing-subscriber = "0.3"
chrono = { version = "0.4", features = ["serde"] }
tokio = { version = "1", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
async-trait = "0.1"
futures = "0.3"
fnv = "1.0"
rusqlite = { version = "0.32", features = ["bundled"] }
tempfile = "3.10"
reqwest = { version = "0.12", features = ["json", "blocking"] }
csv = "1.1"

Crate Dependency Graph
================================================================================

trading-system (root)
├── a_common
│   ├── parking_lot
│   ├── rust_decimal
│   ├── serde
│   ├── thiserror
│   ├── tracing
│   ├── tokio
│   ├── chrono
│   ├── reqwest
│   ├── async-trait
│   └── tokio-tungstenite
│
├── b_data_source
│   ├── a_common
│   ├── tokio-tungstenite
│   ├── futures-util
│   ├── redis
│   ├── parking_lot
│   ├── fnv
│   ├── reqwest
│   └── rand
│
├── c_data_process
│   ├── a_common
│   ├── b_data_source
│   ├── parking_lot
│   ├── rusqlite
│   └── fnv
│
├── d_checktable
│   ├── b_data_source
│   ├── c_data_process
│   ├── e_risk_monitor
│   ├── a_common
│   └── x_data
│
├── e_risk_monitor
│   ├── a_common
│   ├── b_data_source
│   ├── x_data
│   ├── rusqlite
│   ├── tempfile
│   └── reqwest
│
├── f_engine
│   ├── a_common
│   ├── b_data_source
│   ├── d_checktable
│   └── x_data
│
├── x_data (core types - minimal dependencies)
│   ├── rust_decimal
│   ├── chrono
│   ├── serde
│   ├── thiserror
│   └── parking_lot
│
├── g_test
│   ├── b_data_source
│   ├── a_common
│   ├── c_data_process
│   ├── d_checktable
│   ├── e_risk_monitor
│   └── f_engine
│
└── mock 组件
    ├── a_common
    ├── b_data_source
    ├── c_data_process
    ├── d_checktable
    ├── e_risk_monitor
    ├── f_engine
    ├── parquet
    └── rand

================================================================================
End of STACK.md
================================================================================
