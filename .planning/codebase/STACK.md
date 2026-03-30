STACK
===

Language
---
Rust (edition 2024)

Async Runtime
---
tokio version 1 with features = ["full"]

Financial Precision
---
rust_decimal version 1.36 with features = ["maths"]

Concurrency
---
parking_lot version 0.12

Error Handling
---
thiserror version 2.0

Serialization
---
serde version 1.0 with derive feature

Time
---
chrono version 0.4 with serde features

Database
---
rusqlite version 0.32 with features = ["bundled"]

Workspace Configuration
---
Resolver version "3"

Feature Flags
---
mock feature for sandbox mode

Platform Paths
---
Windows: E:/shm/backup/
Linux: /dev/shm/backup/

Cargo Workspace Members
---
a_common
b_data_source
b_data_mock
c_data_process
d_checktable
e_risk_monitor
f_engine
x_data
g_test
