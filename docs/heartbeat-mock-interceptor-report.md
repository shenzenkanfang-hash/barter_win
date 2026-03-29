================================================================================
HEARTBEAT-MOCK INTERCEPTOR SYSTEM REPORT
================================================================================
Date: 2026-03-30
Status: COMPLETED

================================================================================
1. OVERVIEW
================================================================================

本报告描述心跳延迟监控系统在 b_data_mock 中的实现，
用于追踪 Tick 数据流和订单执行的延迟。

设计原则：
- 非侵入性：不修改主业务逻辑
- 可选启用：通过 feature flag 控制
- 零开销：禁用时完全无开销

================================================================================
2. IMPLEMENTED FEATURES
================================================================================

2.1 Heartbeat Token Enhancement
--------------------------------------------------------------------------------
File: crates/a_common/src/heartbeat/token.rs

Changes:
- Added data_timestamp field for tracking data creation time
- Added data_latency_ms() method for latency calculation
- Added data_latency_secs() method for display

New Methods:
+ HeartbeatToken::with_data_timestamp(sequence, data_timestamp)
+ HeartbeatToken::data_latency_ms() -> Option<i64>
+ HeartbeatToken::data_latency_secs() -> Option<f64>

2.2 ReportEntry Enhancement
--------------------------------------------------------------------------------
File: crates/a_common/src/heartbeat/entry.rs

New Fields:
- total_latency_ms: i64  (cumulative latency for avg calculation)
- max_latency_ms: i64    (peak latency observed)
- min_latency_ms: i64    (minimum latency observed)
- last_latency_ms: i64   (most recent latency)

New Methods:
+ ReportEntry::record_report_with_latency(seq, latency_ms)
+ ReportEntry::avg_latency_ms() -> Option<i64>

2.3 HeartbeatReporter Enhancement
--------------------------------------------------------------------------------
File: crates/a_common/src/heartbeat/reporter.rs

New Method:
+ report_with_latency(token, point_id, module, function, file, latency_ms)

Report Structure Updated:
+ PointDetail.avg_latency_ms: Option<i64>
+ PointDetail.max_latency_ms: Option<i64>
+ PointDetail.last_latency_ms: Option<i64>

2.4 Tick Interceptor
--------------------------------------------------------------------------------
File: crates/b_data_mock/src/interceptor/tick_interceptor.rs

Features:
- inject_timestamp(): Add timestamp to Tick data
- calc_latency_ms(): Calculate latency from data creation
- is_latency_anormal(): Check if latency exceeds threshold

2.5 Order Interceptor
--------------------------------------------------------------------------------
File: crates/b_data_mock/src/interceptor/order_interceptor.rs

Features:
- OrderInterceptor: Wraps MockApiGateway
- OrderInterceptorConfig: Configure thresholds
- OrderStats: Execution statistics

Statistics Tracked:
- total_orders: Total orders placed
- successful_orders: Successful executions
- failed_orders: Failed executions
- avg_latency_ms: Average execution latency
- max_latency_ms: Maximum execution latency

================================================================================
3. COMPILATION STATUS
================================================================================

| Package     | Status | Warnings |
|------------|--------|----------|
| a_common   | PASS   | 2 dead_code |
| b_data_mock| PASS   | 1 unused variable |

================================================================================
4. TEST RESULTS
================================================================================

4.1 Heartbeat Tests
--------------------------------------------------------------------------------
| Test                    | Status |
|-------------------------|--------|
| test_sampling_mode      | PASS   |
| Lib tests (43)          | PASS   |
| Doc tests (6)           | PASS   |

4.2 Interceptor Tests
--------------------------------------------------------------------------------
| Test                              | Status |
|-----------------------------------|--------|
| test_tick_interceptor_latency     | PASS   |
| test_tick_interceptor_anormal_detection | PASS |
| test_order_interceptor_stats      | PASS   |
| test_order_interceptor_order_execution | PASS |
| test_order_interceptor_custom_config | PASS |

Total: 5 passed, 0 failed

================================================================================
5. USAGE EXAMPLE
================================================================================

5.1 Tick Interceptor Usage
--------------------------------------------------------------------------------
```rust
use b_data_mock::TickInterceptor;

// Create interceptor
let interceptor = TickInterceptor::new();

// When Tick is created
let tick_timestamp = interceptor.now();

// When processing Tick
let latency_ms = interceptor.calc_latency_ms(tick_timestamp);

// Check for anomalies
if interceptor.is_latency_anormal(latency_ms, 1000) {
    tracing::warn!("Tick processing latency anomaly: {}ms", latency_ms);
}
```

5.2 Order Interceptor Usage
--------------------------------------------------------------------------------
```rust
use b_data_mock::{MockApiGateway, OrderInterceptor};

let gateway = MockApiGateway::with_default_config(dec!(10000));
let interceptor = OrderInterceptor::with_default_config(gateway);

// Place order (with automatic latency tracking)
let result = interceptor.place_order("BTCUSDT", Side::Buy, dec!(0.01), Some(dec!(50000)));

// Get statistics
let stats = interceptor.get_stats();
println!("Avg latency: {}ms, Max latency: {}ms", stats.avg_latency_ms, stats.max_latency_ms);
```

5.3 Heartbeat with Latency
--------------------------------------------------------------------------------
```rust
use a_common::heartbeat::{global, HeartbeatToken};

// Generate token with data timestamp
let token = HeartbeatToken::with_data_timestamp(seq, data_timestamp);

// Report with latency
global().report_with_latency(
    &token,
    "BS-001",
    "b_data_source",
    "on_kline",
    file!(),
    token.data_latency_ms().unwrap_or(0)
).await;
```

================================================================================
6. DATA FLOW DIAGRAM
================================================================================

Mock Tick Generation:
┌─────────────────┐
│ KlineStream      │
│ Generator        │
└────────┬────────┘
         │
         ▼ (data_timestamp = now())
┌─────────────────┐
│ TickInterceptor  │
│ - inject        │
│ - calc_latency  │
└────────┬────────┘
         │
         ▼ (emit with timestamp)
┌─────────────────┐
│ b_data_mock     │
│ DataFeeder      │
└────────┬────────┘
         │
         ▼ (process)
┌─────────────────┐
│ Component       │
│ (SignalProcessor│
│  etc.)          │
└────────┬────────┘
         │
         ▼ (report with latency)
┌─────────────────┐
│ Heartbeat       │
│ Reporter        │
│ + avg_latency   │
│ + max_latency   │
└─────────────────┘

Mock Order Flow:
┌─────────────────┐
│ Strategy        │
│ Decision        │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ OrderInterceptor│
│ - measure time  │
│ - record stats   │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ MockApiGateway  │
└─────────────────┘

================================================================================
7. METRICS AVAILABLE
================================================================================

Tick Processing:
- latency_ms: Time from data creation to component processing
- avg_latency_ms: Average latency across all reports
- max_latency_ms: Peak latency observed

Order Execution:
- avg_latency_ms: Average order execution time
- max_latency_ms: Maximum order execution time
- total_orders: Total orders placed
- successful_orders: Successful executions
- failed_orders: Failed executions

================================================================================
8. CONCLUSIONS
================================================================================

Status: COMPLETED

The heartbeat-mock interceptor system is fully implemented and tested.

Key Features:
1. Zero business logic modification
2. Non-invasive interception layer
3. Comprehensive latency tracking
4. Order execution monitoring
5. Anomaly detection support

All tests pass. The system is ready for integration testing.

================================================================================
END OF REPORT
================================================================================
