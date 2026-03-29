================================================================================
HEARTBEAT-MOCK 拦截器系统报告
================================================================================
日期: 2026-03-30
状态: 已完成

================================================================================
1. 概述
================================================================================

本报告描述心跳延迟监控系统在 b_data_mock 中的实现，
用于追踪 Tick 数据流和订单执行的延迟。

设计原则：
- 非侵入性：不修改主业务逻辑
- 可选启用：通过 feature flag 控制
- 零开销：禁用时完全无开销

================================================================================
2. 已实现功能
================================================================================

2.1 心跳令牌增强
--------------------------------------------------------------------------------
文件: crates/a_common/src/heartbeat/token.rs

修改内容:
- 新增 data_timestamp 字段，用于追踪数据创建时间
- 新增 data_latency_ms() 方法计算延迟
- 新增 data_latency_secs() 方法用于显示

新增方法:
+ HeartbeatToken::with_data_timestamp(sequence, data_timestamp)
+ HeartbeatToken::data_latency_ms() -> Option<i64>
+ HeartbeatToken::data_latency_secs() -> Option<f64>

2.2 报到条目增强
--------------------------------------------------------------------------------
文件: crates/a_common/src/heartbeat/entry.rs

新增字段:
- total_latency_ms: i64  (累计延迟，用于计算平均值)
- max_latency_ms: i64    (观察到的峰值延迟)
- min_latency_ms: i64    (最小延迟)
- last_latency_ms: i64   (最近一次延迟)

新增方法:
+ ReportEntry::record_report_with_latency(seq, latency_ms)
+ ReportEntry::avg_latency_ms() -> Option<i64>

2.3 心跳报告器增强
--------------------------------------------------------------------------------
文件: crates/a_common/src/heartbeat/reporter.rs

新增方法:
+ report_with_latency(token, point_id, module, function, file, latency_ms)

报告结构更新:
+ PointDetail.avg_latency_ms: Option<i64>
+ PointDetail.max_latency_ms: Option<i64>
+ PointDetail.last_latency_ms: Option<i64>

2.4 Tick 拦截器
--------------------------------------------------------------------------------
文件: crates/b_data_mock/src/interceptor/tick_interceptor.rs

功能:
- inject_timestamp(): 为 Tick 数据注入时间戳
- calc_latency_ms(): 从数据创建时计算延迟
- is_latency_anormal(): 检查延迟是否超过阈值

2.5 订单拦截器
--------------------------------------------------------------------------------
文件: crates/b_data_mock/src/interceptor/order_interceptor.rs

功能:
- OrderInterceptor: 包装 MockApiGateway
- OrderInterceptorConfig: 配置阈值
- OrderStats: 执行统计

追踪的统计数据:
- total_orders: 总下单数
- successful_orders: 成功执行数
- failed_orders: 失败执行数
- avg_latency_ms: 平均执行延迟
- max_latency_ms: 最大执行延迟

================================================================================
3. 编译状态
================================================================================

| 包名         | 状态 | 警告           |
|------------|--------|----------------|
| a_common   | 通过   | 2个 dead_code  |
| b_data_mock| 通过   | 1个未使用变量   |

================================================================================
4. 测试结果
================================================================================

4.1 心跳测试
--------------------------------------------------------------------------------
| 测试                    | 状态 |
|-------------------------|------|
| test_sampling_mode      | 通过  |
| Lib tests (43)          | 通过  |
| Doc tests (6)           | 通过  |

4.2 拦截器测试
--------------------------------------------------------------------------------
| 测试                                    | 状态 |
|-----------------------------------------|------|
| test_tick_interceptor_latency           | 通过  |
| test_tick_interceptor_anormal_detection | 通过  |
| test_order_interceptor_stats            | 通过  |
| test_order_interceptor_order_execution   | 通过  |
| test_order_interceptor_custom_config    | 通过  |

总计: 5个通过, 0个失败

================================================================================
5. 使用示例
================================================================================

5.1 Tick 拦截器使用
--------------------------------------------------------------------------------
```rust
use b_data_mock::TickInterceptor;

// 创建拦截器
let interceptor = TickInterceptor::new();

// Tick 创建时
let tick_timestamp = interceptor.now();

// 处理 Tick 时
let latency_ms = interceptor.calc_latency_ms(tick_timestamp);

// 检查异常
if interceptor.is_latency_anormal(latency_ms, 1000) {
    tracing::warn!("Tick 处理延迟异常: {}ms", latency_ms);
}
```

5.2 订单拦截器使用
--------------------------------------------------------------------------------
```rust
use b_data_mock::{MockApiGateway, OrderInterceptor};

let gateway = MockApiGateway::with_default_config(dec!(10000));
let interceptor = OrderInterceptor::with_default_config(gateway);

// 下单（自动追踪延迟）
let result = interceptor.place_order("BTCUSDT", Side::Buy, dec!(0.01), Some(dec!(50000)));

// 获取统计
let stats = interceptor.get_stats();
println!("平均延迟: {}ms, 最大延迟: {}ms", stats.avg_latency_ms, stats.max_latency_ms);
```

5.3 带延迟的心跳报到
--------------------------------------------------------------------------------
```rust
use a_common::heartbeat::{global, HeartbeatToken};

// 生成带数据时间戳的令牌
let token = HeartbeatToken::with_data_timestamp(seq, data_timestamp);

// 带延迟报到
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
6. 数据流架构图
================================================================================

Mock Tick 生成:
┌─────────────────┐
│ KlineStream      │
│ Generator        │  K线流生成器
└────────┬────────┘
         │
         ▼ (data_timestamp = now())
┌─────────────────┐
│ TickInterceptor │
│ - inject        │  Tick拦截器
│ - calc_latency  │
└────────┬────────┘
         │
         ▼ (emit with timestamp)
┌─────────────────┐
│ b_data_mock     │
│ DataFeeder      │  数据喂养器
└────────┬────────┘
         │
         ▼ (process)
┌─────────────────┐
│ Component       │
│ (SignalProcessor│  各组件处理
│  etc.)          │
└────────┬────────┘
         │
         ▼ (report with latency)
┌─────────────────┐
│ Heartbeat       │
│ Reporter        │  心跳报告器
│ + avg_latency   │
│ + max_latency   │
└─────────────────┘

Mock 订单流程:
┌─────────────────┐
│ Strategy        │  策略决策
│ Decision        │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ OrderInterceptor│  订单拦截器
│ - measure time  │
│ - record stats  │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ MockApiGateway  │  模拟API网关
└─────────────────┘

================================================================================
7. 可用指标
================================================================================

Tick 处理:
- latency_ms: 从数据创建到组件处理的延迟
- avg_latency_ms: 所有报文的平均延迟
- max_latency_ms: 观察到的峰值延迟

订单执行:
- avg_latency_ms: 平均订单执行时间
- max_latency_ms: 最大订单执行时间
- total_orders: 总下单数
- successful_orders: 成功执行数
- failed_orders: 失败执行数

================================================================================
8. 结论
================================================================================

状态: 已完成

心跳-Mock 拦截器系统已完全实现并通过测试。

核心特性:
1. 零业务逻辑修改
2. 非侵入式拦截层
3. 全面的延迟追踪
4. 订单执行监控
5. 异常检测支持

所有测试通过。系统已准备好进行集成测试。

================================================================================
报告结束
================================================================================
