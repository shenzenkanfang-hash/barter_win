================================================================================
                    心跳报告器设计文档
                    版本: 1.0
                    日期: 2026-03-29
================================================================================

Author: Claude Code (Brainstorming)
Created: 2026-03-29
Stage: brainstorming
Status: 待审核
Next: writing-plans

================================================================================
一、项目概述
================================================================================

1. 项目名称: Heartbeat Reporter (心跳报告器)
2. 目的: 诊断 barter-rs 事件驱动流程的连通性
3. 问题: 当前无法准确知道数据流程哪里通、哪里断
4. 解决方案: 通过心跳标记在关键功能点传递，每个点向统一接口报告

================================================================================
二、核心设计
================================================================================

2.1 架构

┌─────────────────────────────────────────────────────────────────┐
│              a_common::heartbeat_reporter                       │
│                    (心跳报告器)                                   │
├─────────────────────────────────────────────────────────────────┤
│  存储内容:                                                       │
│  - 当前心跳序号: HB_1, HB_2, HB_3...                            │
│  - 每个测试点报到次数: { "AC-001": 5, "BS-001": 5, ... }        │
│  - 最后报到时间                                                   │
│  - 心跳生成时间                                                   │
└─────────────────────────────────────────────────────────────────┘
                              ↓
                    定时生成报告文件:
              data/heartbeat_report_YYYY-MM-DD_HH-MM.json

2.2 心跳机制

| 项目 | 设计 |
|------|------|
| 格式 | HB_序号 (如 HB_1, HB_2, HB_3...) |
| 刷新 | 每分钟自动+1 |
| 生成位置 | a_common::heartbeat_reporter |
| 传递方式 | 通过函数参数传递 HeartbeatToken |

2.3 报到标识

报到ID与 FUNCTIONAL_TEST_POINTS.md 中的测试点ID对应:

| 模块 | 测试点前缀 | 优先级 |
|------|-----------|--------|
| a_common | AC-* | P0/P1/P2 |
| b_data_source | BS-* | P0/P1/P2 |
| b_data_mock | BM-* | P0/P1/P2 |
| c_data_process | CP-* | P0/P1/P2 |
| d_checktable | DT-* | P0/P1/P2 |
| e_risk_monitor | ER-* | P0/P1/P2 |
| f_engine | FE-* | P0/P1/P2 |
| x_data | XD-* | P0/P1/P2 |

================================================================================
三、数据结构
================================================================================

3.1 HeartbeatToken

```rust
pub struct HeartbeatToken {
    pub sequence: u64,           // 心跳序号 HB_1, HB_2...
    pub generated_at: DateTime,  // 生成时间
}

impl HeartbeatToken {
    pub fn new() -> Self
    pub fn sequence_str(&self) -> String  // "HB_1"
}
```

3.2 ReportEntry

```rust
pub struct ReportEntry {
    pub point_id: String,         // 测试点ID，如 "BS-001"
    pub point_name: String,       // 功能名称，如 "Kline1mStream"
    pub module: String,           // 模块名，如 "b_data_source"
    pub function: String,         // 函数名
    pub file: String,             // 文件路径
    pub reports_count: u64,       // 报到次数
    pub last_report_at: DateTime, // 最后报到时间
}
```

3.3 HeartbeatReporter

```rust
pub struct HeartbeatReporter {
    token: HeartbeatToken,
    entries: HashMap<String, ReportEntry>,
    started_at: DateTime,
}

impl HeartbeatReporter {
    pub fn new() -> Self
    pub fn generate_next_token(&mut self) -> HeartbeatToken
    pub fn report(&mut self, point_id: &str, point_name: &str, module: &str, function: &str, file: &str)
    pub fn generate_report(&self) -> HeartbeatReport
    pub fn get_token(&self) -> &HeartbeatToken
    pub fn summary(&self) -> Summary
}

pub struct HeartbeatReport {
    pub report_time: DateTime,
    pub heartbeat_sequence: u64,
    pub duration_minutes: u64,
    pub total_reports: u64,
    pub active_points: Vec<String>,
    pub inactive_points: Vec<String>,
    pub modules: HashMap<String, ModuleSummary>,
    pub points_detail: Vec<ReportEntry>,
}

pub struct Summary {
    pub total_points: usize,
    pub active_count: usize,
    pub inactive_count: usize,
    pub reports_count: u64,
}
```

================================================================================
四、报告格式
================================================================================

4.1 JSON报告文件

文件位置: data/heartbeat_report_YYYY-MM-DD_HH-MM.json

```json
{
  "report_time": "2026-03-29T10:35:00Z",
  "heartbeat_sequence": 5,
  "duration_minutes": 5,
  "total_reports": 47,
  "active_points": 8,
  "inactive_points": 2,
  "modules": {
    "a_common": { "reports": 10, "point_count": 2 },
    "b_data_source": { "reports": 5, "point_count": 1 },
    "c_data_process": { "reports": 8, "point_count": 1 },
    "d_checktable": { "reports": 0, "point_count": 5 },
    "e_risk_monitor": { "reports": 12, "point_count": 3 },
    "f_engine": { "reports": 12, "point_count": 3 }
  },
  "points_detail": [
    {
      "point_id": "AC-001",
      "point_name": "BinanceApiGateway",
      "module": "a_common",
      "function": "fetch_and_save_all_usdt_symbol_rules",
      "file": "crates/a_common/src/api/gateway.rs",
      "reports_count": 5,
      "last_report_at": "2026-03-29T10:34:00Z"
    }
  ],
  "inactive_points": ["DT-003", "FE-008"]
}
```

4.2 终端快速查看

运行时可通过以下方式快速查看:

```rust
// 查看摘要
println!("{:?}", reporter.summary());

// 查看当前心跳
println!("Current: {}", reporter.get_token().sequence_str());

// 查看某模块状态
reporter.print_module_summary("b_data_source");
```

================================================================================
五、关键功能点清单 (P0优先)
================================================================================

5.1 P0测试点 (首次实现)

| 序号 | 测试点 | 模块 | 功能 |
|------|--------|------|------|
| BS-001 | Kline1mStream | b_data_source | 1分钟K线WebSocket订阅 |
| BS-007 | FuturesDataSyncer | b_data_source | 账户数据同步 |
| CP-001 | SignalProcessor | c_data_process | 策略信号处理 |
| CP-007 | StrategyStateManager | c_data_process | 策略状态管理 |
| DT-001 | CheckTable | d_checktable | 检查表注册与调度 |
| DT-002 | h_15m::Trader | d_checktable | 15分钟高频交易员 |
| ER-001 | RiskPreChecker | e_risk_monitor | 订单预检查 |
| ER-003 | OrderCheck | e_risk_monitor | 订单风控校验 |
| FE-001 | EventEngine | f_engine | 事件驱动引擎启动 |
| FE-003 | EventBus | f_engine | 事件总线发布/订阅 |

5.2 报到调用示例

```rust
use a_common::heartbeat_reporter::{HeartbeatReporter, HeartbeatToken};

// 在关键函数中添加报到调用
pub async fn on_kline(&mut self, kline: Kline, token: &HeartbeatToken) -> Result<()> {
    heartbeat_reporter::report(
        token,
        "BS-001",
        "Kline1mStream",
        "b_data_source",
        "on_kline",
        file!(),
        line!()
    );
    // ... 原有逻辑
}
```

================================================================================
六、定时报告生成
================================================================================

6.1 报告触发

| 触发方式 | 间隔 | 说明 |
|----------|------|------|
| 定时生成 | 每5分钟 | 自动生成JSON报告 |
| 查询触发 | 手动 | 通过API/命令查询 |
| 异常触发 | 断裂检测 | 发现断裂时立即报告 |

6.2 报告保存策略

- 位置: data/heartbeat_reports/
- 命名: heartbeat_report_YYYY-MM-DD_HH-MM.json
- 保留: 最近24小时 + 每天一份
- 清理: 启动时清理过期报告

================================================================================
七、使用方式
================================================================================

7.1 初始化

```rust
use a_common::heartbeat_reporter::HeartbeatReporter;

static REPORTER: once_cell::sync::Lazy<Mutex<HeartbeatReporter>> =
    once_cell::sync::Lazy::new(|| Mutex::new(HeartbeatReporter::new()));
```

7.2 事件流传递

```rust
// 1. K线到达 -> 生成心跳
let token = reporter.lock().unwrap().generate_next_token();

// 2. 传递给下游模块
process_kline(kline, &token).await?;
check_table::on_tick(&token).await?;
risk_monitor::on_order(&token).await?;

// 3. 定时生成报告
reporter.lock().unwrap().save_periodic_report().await?;
```

7.3 查询状态

```rust
// 命令行或调试时
let summary = reporter.lock().unwrap().summary();
println!("{:?}", summary);

// 查看断裂点
let report = reporter.lock().unwrap().generate_report();
for point in report.inactive_points {
    eprintln!("断裂: {}", point);
}
```

================================================================================
八、预期效果
================================================================================

8.1 正常状态

```
HB_5 活跃测试点:
  a_common:     AC-001(5) AC-004(5) AC-008(5)  [3/14]
  b_data_source: BS-001(5) BS-004(5) BS-007(5)  [3/17]
  c_data_process: CP-001(5) CP-007(5)  [2/12]
  d_checktable: DT-001(5) DT-002(5)  [2/12]
  e_risk_monitor: ER-001(5) ER-003(5) ER-011(5)  [3/21]
  f_engine: FE-001(5) FE-003(5)  [2/12]

总计: 15/88 测试点活跃
```

8.2 断裂状态

```
HB_3 断裂检测:
  [!!] DT-003 h_15m::Executor 未报到
  [!!] FE-008 TraderManager 未报到

流程断裂位置: d_checktable -> f_engine
最后活跃: HB_2 at e_risk_monitor
```

================================================================================
九、实现优先级
================================================================================

Phase 1: 核心框架
  - HeartbeatToken 数据结构
  - HeartbeatReporter 核心逻辑
  - 报到接口 report()

Phase 2: P0测试点集成
  - BS-001 Kline1mStream
  - FE-001/FE-003 EventEngine/EventBus
  - CP-001 SignalProcessor
  - DT-001 CheckTable
  - ER-001 RiskPreChecker

Phase 3: 报告系统
  - JSON报告生成
  - 定时保存
  - 终端快速查看

Phase 4: 扩展集成
  - 其他P1/P2测试点
  - 断裂告警
  - Web/API查询接口

================================================================================
                              文档结束
================================================================================
