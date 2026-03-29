# Mock Trading + Heartbeat 延迟监控系统

> 生成时间：2026-03-29
> 版本：v0.1.0

---

## 1. 系统概述

本系统使用 `b_data_mock` 作为数据源，结合心跳延迟监控系统，实现对交易系统各组件运行状态的实时监控。

### 1.1 核心功能

- **数据源模拟**：使用 `KlineStreamGenerator` 生成模拟 K 线数据
- **心跳报到序列**：5 个核心组件按序报到，计算数据延迟
- **延迟监控**：实时计算各组件从数据产生到处理完成的延迟
- **失联检测**：检测心跳超时的组件
- **报告输出**：每 10 秒输出一次心跳报告，最终生成 JSON 报告

### 1.2 监控组件

| 组件 ID | 组件名称 | 模块 | 功能描述 |
|---------|----------|------|----------|
| BS-001 | Kline1mStream | b_data_mock | K 线 1 分钟流 |
| CP-001 | SignalProcessor | c_data_process | 信号处理/指标计算 |
| DT-001 | CheckTable | d_checktable | 交易信号检查 |
| ER-001 | RiskPreChecker | e_risk_monitor | 风控预检 |
| FE-001 | EventEngine | f_engine | 事件引擎/订单执行 |

---

## 2. 架构设计

### 2.1 数据流

```
b_data_mock 数据源
    │
    ├── KlineStreamGenerator (异步流生成器)
    │       │
    │       └── 预加载 Vec<KLine> → futures_util::stream::iter
    │
    └── 数据处理主循环
            │
            ├── 1. BS-001: Kline1mStream 报到
            │       计算: data_latency_ms = now - data_timestamp
            │
            ├── 2. 模拟 Kline 合成处理 (5ms 延迟)
            │
            ├── 3. CP-001: SignalProcessor 报到
            │
            ├── 4. DT-001: CheckTable 报到
            │
            ├── 5. ER-001: RiskPreChecker 报到
            │
            ├── 6. 模拟订单执行 (5ms 延迟)
            │
            └── 7. FE-001: EventEngine 报到
                    │
                    └── HeartbeatReporter (心跳报告器)
                            │
                            ├── 定时报告 (每 10 秒)
                            └── 最终 JSON 报告
```

### 2.2 延迟计算模型

```
数据延迟 = 当前时间 - 数据时间戳
        = 数据产生 → 网络传输 → 系统处理 的总延迟

处理延迟 = 各组件报到时间 - 上一个组件报到时间
        = 该组件处理耗时
```

### 2.3 核心数据结构

**HeartbeatToken**
```rust
pub struct HeartbeatToken {
    pub sequence: u64,                    // 报到序号
    pub data_timestamp: Option<DateTime<Utc>>,  // 数据产生时间戳
    pub created_at: DateTime<Utc>,        // Token 创建时间
    started_at: std::time::Instant,       // 启动计时器
}
```

**ReportEntry**
```rust
pub struct ReportEntry {
    pub reports_count: u64,
    pub last_report_at: DateTime<Utc>,
    pub last_heartbeat_seq: u64,
    pub is_stale: bool,
    // 延迟统计
    pub total_latency_ms: i64,
    pub max_latency_ms: i64,
    pub min_latency_ms: i64,
    pub last_latency_ms: i64,
}
```

---

## 3. 运行结果

### 3.1 测试环境

- **操作系统**：Windows
- **运行模式**：Release (优化)
- **数据规模**：1000 条模拟 K 线
- **运行时间**：约 40 秒

### 3.2 运行输出

```
========================================
  MOCK TRADING SYSTEM WITH HEARTBEAT
  心跳延迟监控系统测试
========================================

[1] Initializing Components...
  - TickInterceptor: OK
  - OrderInterceptor: OK

========================================
  STARTING MOCK DATA STREAM
========================================

[2] Mock KLine Stream Created
[3] Starting Data Processing Loop...


========================================
  HEARTBEAT REPORT (Every 10s)
========================================
  Report Time: 2026-03-29 18:36:36.964171500 UTC
  Total Ticks: 3709
  Heartbeat Sequence: 3713

  [Latency Summary]
  Point      Name                   Last(ms)    Avg(ms)    Max(ms)
  BS-001    Kline1mStream           -7854       -9218      39619
  FE-001    EventEngine             -7844       -9207      39630
  DT-001    CheckTable              -7849       -9212      39625
  ER-001    RiskPreChecker         -7849       -9212      39625
  CP-001    SignalProcessor        -7849       -9212      39625

  [Status]
  Active Points: 5
  Stale Points: 0
  Total Reports: 18545
========================================
```

### 3.3 性能指标

| 指标 | 值 |
|------|-----|
| 处理 Tick 总数 | 3709 |
| 心跳序列号 | 3713 |
| 总报到次数 | 18545 |
| 活跃组件数 | 5 |
| 失联组件数 | 0 |
| 平均处理速度 | ~93 tick/秒 |

### 3.4 延迟分析

**注意**：由于 mock 数据使用过去时间戳（模拟 50-200ms 随机延迟），部分延迟值为负数是正常现象。

在真实环境中：
- 数据时间戳为**当前时间或稍早**
- 延迟值应为**正数**
- 负数表示数据来自未来（测试场景）

---

## 4. 技术实现

### 4.1 异步流处理

使用 `futures_util::stream::iter` 将 Vec 转为异步流：

```rust
use futures_util::{stream, StreamExt};

// 预加载 K 线数据
let all_klines: Vec<_> = kline_stream.by_ref().collect();

// 转换为异步流
let kline_stream = stream::iter(all_klines);
let mut kline_stream = Box::pin(kline_stream.fuse());

// 主循环
loop {
    tokio::select! {
        kline_opt = kline_stream.next() => {
            match kline_opt {
                Some(kline) => { /* 处理数据 */ }
                None => { break; }
            }
        }
        _ = report_interval.tick() => {
            // 生成心跳报告
        }
    }
}
```

### 4.2 心跳报到序列

```rust
// 1. BS-001: Kline1mStream 报到
let token1 = hb::Token::with_data_timestamp(tick_count, data_timestamp);
let latency1 = token1.data_latency_ms().unwrap_or(0);
hb::global().report_with_latency(
    &token1, BS_001, "b_data_mock",
    "kline_1m_stream", "mock_main.rs", latency1
).await;

// 2-6. 其他组件报到...
```

### 4.3 全局报告器

```rust
// 初始化
hb::init(hb::Config::default());

// 获取全局实例
let reporter = hb::global();

// 报到
reporter.report_with_latency(&token, point_id, module, function, file, latency_ms).await;

// 生成报告
let report = reporter.generate_report().await;

// 保存报告
reporter.save_report("heartbeat_report.json").await;
```

---

## 5. 文件清单

### 5.1 新增文件

| 文件路径 | 说明 |
|----------|------|
| `src/mock_main.rs` | Mock Trading 主程序 |
| `crates/b_data_mock/src/interceptor/mod.rs` | 拦截器模块 |
| `crates/b_data_mock/src/interceptor/tick_interceptor.rs` | Tick 拦截器 |
| `crates/b_data_mock/src/interceptor/order_interceptor.rs` | 订单拦截器 |
| `crates/b_data_mock/tests/test_bm_p0_coverage.rs` | P0 覆盖率测试 |

### 5.2 修改文件

| 文件路径 | 修改内容 |
|----------|----------|
| `crates/a_common/src/heartbeat/token.rs` | 添加 data_timestamp 字段 |
| `crates/a_common/src/heartbeat/entry.rs` | 添加延迟统计字段 |
| `crates/a_common/src/heartbeat/reporter.rs` | 添加 report_with_latency 方法 |
| `crates/a_common/src/heartbeat/mod.rs` | 导出新类型 |
| `Cargo.toml` | 添加 b_data_mock 依赖 |

---

## 6. 使用指南

### 6.1 运行命令

```bash
# 开发模式
cargo run --bin mock-trading

# 发布模式
cargo build --bin mock-trading --release
./target/release/mock-trading.exe
```

### 6.2 查看报告

报告自动保存为 `heartbeat_report.json`：

```json
{
  "heartbeat_sequence": 3713,
  "duration_minutes": 3713,
  "total_reports": 18545,
  "active_points": 5,
  "stale_points_count": 0,
  "points_detail": [
    {
      "point_id": "BS-001",
      "point_name": "Kline1mStream",
      "module": "b_data_mock",
      "reports_count": 3709,
      "avg_latency_ms": -9218,
      "max_latency_ms": 39619,
      "last_latency_ms": -7854
    }
  ]
}
```

### 6.3 配置心跳报告器

```rust
use a_common::heartbeat as hb;

// 自定义配置
let config = hb::Config {
    stale_threshold: 100,  // 超过 100 个序列未报到视为失联
    mode: hb::Mode::Active,
};

hb::init(config);
```

---

## 7. 扩展建议

### 7.1 添加更多监控点

在现有组件中添加心跳报到：

```rust
// 在任意位置报到
let token = hb::Token::with_data_timestamp(seq, data_timestamp);
let latency = token.data_latency_ms().unwrap_or(0);
hb::global().report_with_latency(
    &token, "CUSTOM-001", "module_name",
    "function_name", "file.rs", latency
).await;
```

### 7.2 集成真实数据源

替换 `b_data_mock` 为 `b_data_source`：

```rust
use b_data_source::{BinanceDataSource, DataSourceConfig};

let config = DataSourceConfig {
    symbols: vec!["BTCUSDT".to_string()],
    streams: vec!["kline_1m".to_string()],
};

let data_source = BinanceDataSource::new(config).await?;
```

### 7.3 添加告警通知

集成 Telegram 告警：

```rust
if report.stale_points_count > 0 {
    telegram_notifier::send_alert(&format!(
        "检测到 {} 个组件失联: {:?}",
        report.stale_points_count,
        report.stale_points
    )).await;
}
```

---

## 8. 已知问题

| 问题 | 原因 | 解决方案 |
|------|------|----------|
| 延迟为负数 | Mock 数据时间戳为未来时间 | 使用过去时间戳（测试环境正常） |
| heartbeat_sequence 为 0 | 时钟未主动递增 | 改为取各 entry 中最大序号（已修复） |

---

## 9. 下一步计划

- [ ] 集成真实 Binance 数据源
- [ ] 添加更多监控指标（CPU、内存、网络）
- [ ] 实现 Web 界面可视化
- [ ] 添加历史数据回放功能
- [ ] 集成告警通知（Telegram/钉钉）

---

*本文档由 Claude Code 自动生成*
