================================================================================
                    心跳报告器设计文档
                    版本: 2.0 (审核修订版)
                    日期: 2026-03-29
================================================================================

Author: Claude Code (Brainstorming)
Reviewer: User
Created: 2026-03-29
Stage: brainstorming
Status: 审核通过，待实施
Next: writing-plans

================================================================================
修订记录
================================================================================

v2.0 (2026-03-29):
  - 修复阻塞性问题 #1: 统一 report() API 设计
  - 修复阻塞性问题 #2: 明确 HeartbeatToken 生成机制
  - 修复阻塞性问题 #3: 补充并发/性能影响评估
  - 补充: 使用 Instant 计算 duration，避免 NTP 回拨
  - 补充: 增加批量报到接口
  - 补充: 心跳超时机制
  - 补充: 降级模式 (ReportMode)
  - 补充: 配置化测试点映射
  - 补充: 集成测试方案

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
│  并发模型: tokio::sync::RwLock (读多写少优化)                    │
│  时钟模型: Instant (duration) + DateTime (展示)                  │
│  降级模式: Full -> Sampling -> Disabled                          │
└─────────────────────────────────────────────────────────────────┘
                              ↓
                    定时生成报告文件:
              data/heartbeat_report_YYYY-MM-DD_HH-MM.json

2.2 心跳机制

| 项目 | 设计 |
|------|------|
| 格式 | HB_序号 (如 HB_1, HB_2, HB_3...) |
| 刷新 | 每分钟自动+1，序号全局原子计数 |
| 生成位置 | HeartbeatClock::generate() |
| 传递方式 | 通过函数参数传递 HeartbeatToken |
| 超时检测 | 使用 Instant 计算，超 N 分钟未报到标记为"失联" |

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

3.1 HeartbeatToken (修订)

```rust
pub struct HeartbeatToken {
    pub sequence: u64,           // 心跳序号 HB_1, HB_2...
    pub created_at: DateTime,    // wall-clock，用于报告展示 (ISO8601)
    pub started_at: Instant,     // 单调时钟，用于超时检测
}

impl HeartbeatToken {
    // 由 HeartbeatClock::generate() 生成，不允许外部直接创建
    pub(crate) fn new(sequence: u64) -> Self
    pub fn sequence_str(&self) -> String { format!("HB_{}", self.sequence) }
    pub fn elapsed(&self) -> Duration { self.started_at.elapsed() }
}
```

3.2 ReportEntry (修订)

```rust
pub struct ReportEntry {
    pub point_id: String,          // 测试点ID，如 "BS-001"
    pub point_name: String,       // 功能名称，如 "Kline1mStream"
    pub module: String,            // 模块名，如 "b_data_source"
    pub function: String,          // 函数名
    pub file: String,              // 文件路径
    pub reports_count: u64,       // 报到次数
    pub last_report_at: DateTime,  // 最后报到时间 (wall-clock)
    pub last_heartbeat_seq: u64,  // 最后报到对应的心跳序号
    pub is_stale: bool,           // 是否失联 (>N 个心跳周期未报到)
}

impl ReportEntry {
    pub fn new(point_id: &str, point_name: &str, module: &str,
               function: &str, file: &str) -> Self
    pub fn record_report(&mut self, seq: u64, now: DateTime)
    pub fn check_stale(&mut self, current_seq: u64, threshold: u64) -> bool
}
```

3.3 HeartbeatReporter (统一API设计)

```rust
// ============ 全局访问点 ============
pub struct HeartbeatReporter {
    clock: HeartbeatClock,
    entries: RwLock<HashMap<String, ReportEntry>>,
    config: ReporterConfig,
    mode: AtomicReportMode,
}

impl HeartbeatReporter {
    // 全局单例访问点
    pub fn global() -> &'static Self { ... }

    // ============ 核心方法 ============

    // 生成下一个心跳 token
    pub async fn generate_token(&self) -> HeartbeatToken {
        HeartbeatToken::new(self.clock.next_sequence())
    }

    // 报到接口 (统一为实例方法)
    pub async fn report(
        &self,
        token: &HeartbeatToken,
        point_id: &str,
        module: &str,
        function: &str,
        file: &str,
    ) {
        // 1. 检查降级模式
        // 2. 检查采样率
        // 3. 获取写锁
        // 4. 更新 entry
    }

    // 批量报到 (减少锁竞争)
    pub async fn report_batch(&self, token: &HeartbeatToken, reports: Vec<Report>) { ... }

    // 查询方法
    pub async fn summary(&self) -> Summary { ... }
    pub async fn generate_report(&self) -> HeartbeatReport { ... }
    pub async fn get_token(&self) -> HeartbeatToken { ... }
    pub async fn get_stale_points(&self) -> Vec<String> { ... }

    // 模式切换
    pub async fn set_mode(&self, mode: ReportMode) { ... }
}

// ============ 配置 ============
#[derive(Clone)]
pub struct ReporterConfig {
    pub stale_threshold: u64,      // 失联阈值 (心跳周期数，默认 3)
    pub report_interval_secs: u64,  // 报告生成间隔 (秒，默认 300)
    pub max_file_age_hours: u64,    // 文件保留时间 (小时，默认 24)
    pub max_file_size_mb: u64,      // 文件大小限制 (MB，默认 100)
}

impl Default for ReporterConfig {
    fn default() -> Self {
        Self {
            stale_threshold: 3,
            report_interval_secs: 300,
            max_file_age_hours: 24,
            max_file_size_mb: 100,
        }
    }
}

// ============ 降级模式 ============
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportMode {
    Full,           // 正常报到
    Sampling(u32),  // 1/N 采样，如 Sampling(10) 表示 1/10 概率报到
    Disabled,       // 完全关闭
}

impl ReportMode {
    pub fn should_report(&self) -> bool {
        match self {
            ReportMode::Full => true,
            ReportMode::Sampling(n) => rand::thread_rng().gen_range(0..*n) == 0,
            ReportMode::Disabled => false,
        }
    }
}
```

3.4 HeartbeatClock (序号生成器)

```rust
use std::sync::atomic::{AtomicU64, Ordering};

pub struct HeartbeatClock {
    sequence: AtomicU64,        // 原子递增序号
    interval_secs: u64,         // 心跳间隔 (秒)
    last_generate_at: Mutex<Instant>,  // 上次生成时间
}

impl HeartbeatClock {
    pub fn new(interval_secs: u64) -> Self
    pub fn next_sequence(&self) -> u64 { self.sequence.fetch_add(1, Ordering::SeqCst) }
    pub fn try_generate(&self) -> Option<HeartbeatToken> { ... }
    pub fn current_sequence(&self) -> u64 { self.sequence.load(Ordering::SeqCst) }
}
```

3.5 ReportEntry (修订)

```rust
pub struct ReportEntry {
    pub point_id: String,
    pub point_name: String,       // 从配置映射获取，非报到时传入
    pub module: String,
    pub function: String,
    pub file: String,
    pub reports_count: u64,
    pub last_report_at: DateTime,
    pub last_heartbeat_seq: u64,
    pub is_stale: bool,
}

impl ReportEntry {
    pub fn new(point_id: &str, module: &str, function: &str, file: &str) -> Self
    pub fn record_report(&mut self, seq: u64, now: DateTime)
}
```

================================================================================
四、测试点配置 (新增)
================================================================================

4.1 测试点映射表

```rust
// 硬编码配置，便于维护
const TEST_POINT_NAMES: &[(&str, &str, &str)] = &[
    // a_common
    ("AC-001", "BinanceApiGateway", "REST API连接与请求"),
    ("AC-004", "BinanceWsConnector", "WebSocket连接建立"),
    ("AC-005", "BinanceWsConnector", "心跳保活机制"),
    ("AC-006", "BinanceWsConnector", "重连逻辑"),

    // b_data_source
    ("BS-001", "Kline1mStream", "1分钟K线WebSocket订阅"),
    ("BS-003", "Kline1dStream", "1天K线WebSocket订阅"),
    ("BS-004", "DepthStream", "订单簿深度流订阅"),
    ("BS-007", "FuturesDataSyncer", "账户数据同步"),

    // c_data_process
    ("CP-001", "SignalProcessor", "策略信号处理"),
    ("CP-007", "StrategyStateManager", "策略状态管理"),

    // d_checktable
    ("DT-001", "CheckTable", "检查表注册与调度"),
    ("DT-002", "h_15m::Trader", "15分钟高频交易员"),

    // e_risk_monitor
    ("ER-001", "RiskPreChecker", "订单预检查"),
    ("ER-003", "OrderCheck", "订单风控校验"),

    // f_engine
    ("FE-001", "EventEngine", "事件驱动引擎启动"),
    ("FE-003", "EventBus", "事件总线发布/订阅"),
];

pub fn get_point_name(point_id: &str) -> Option<&'static str> {
    TEST_POINT_NAMES.iter()
        .find(|(id, _, _)| *id == point_id)
        .map(|(_, name, _)| *name)
}
```

================================================================================
五、并发与性能评估 (新增)
================================================================================

5.1 并发模型

| 组件 | 并发原语 | 原因 |
|------|---------|------|
| entries | RwLock | 读多写少，80%读/20%写 |
| mode | AtomicReportMode | 无锁读取 |
| clock.sequence | AtomicU64 | 无锁原子递增 |
| clock.last_generate | Mutex | 仅写入时需要 |

5.2 性能开销估算

| 场景 | 事件频率 | 单次报到延迟 | CPU占用 |
|------|---------|-------------|---------|
| 正常 | 100 events/s | < 1us | < 0.01% |
| 高频 | 10000 events/s | < 5us | < 0.1% |
| 采样模式 | 1/10 报到 | < 1us | < 0.01% |

计算依据:
- RwLock 读锁: ~100ns
- HashMap 查找: ~50ns
- atomic fetch_add: ~20ns
- 总计: < 200ns (Full模式)

5.3 降级策略

```
正常负载 (< 1000 events/s):
  -> Full 模式，每事件报到

中等负载 (1000-10000 events/s):
  -> 自动切换 Sampling(10)，每10事件报到1次

高负载 (> 10000 events/s):
  -> Sampling(100)，每100事件报到1次

故障/调试:
  -> 手动 Disabled，完全关闭报到
```

5.4 内存占用

| 数据结构 | 每条目大小 | 100个测试点 |
|----------|-----------|-------------|
| entries | ~200 bytes | ~20 KB |
| ReportEntry | ~150 bytes | ~15 KB |
| 总计 | - | < 50 KB |

================================================================================
六、API 使用方式 (修订)
================================================================================

6.1 便捷宏 (推荐)

```rust
#[macro_export]
macro_rules! heartbeat {
    ($token:expr, $point_id:expr) => {{
        $crate::heartbeat_reporter::global()
            .report($token, $point_id, module!(), function_name!(), file!())
            .await;
    }};
}

// 获取当前函数名 (Rust stable 可用)
macro_rules! function_name {
    () => {{
        fn f() {}
        fn type_name<T>(_: &T) -> &'static str { std::any::type_name::<T>() }
        let name = type_name(&f);
        &name[5..name.len() - 1]
    }};
}
```

6.2 调用示例 (修订)

```rust
use crate::heartbeat;

// 在关键函数中添加报到调用
pub async fn on_kline(&mut self, kline: Kline, token: &HeartbeatToken) -> Result<()> {
    // 报到
    heartbeat!(token, "BS-001");

    // ... 原有逻辑
}

pub async fn on_order(&self, order: OrderRequest, token: &HeartbeatToken) -> Result<()> {
    heartbeat!(token, "ER-001");

    // ... 原有逻辑
}
```

6.3 初始化

```rust
// main.rs 或模块初始化
use a_common::heartbeat_reporter::{HeartbeatReporter, ReporterConfig};

#[tokio::main]
async fn main() {
    // 初始化全局报告器
    let config = ReporterConfig {
        stale_threshold: 3,
        report_interval_secs: 300,
        max_file_age_hours: 24,
        max_file_size_mb: 100,
    };
    HeartbeatReporter::init(config);

    // 启动定时报告任务
    tokio::spawn(async {
        let reporter = HeartbeatReporter::global();
        let mut interval = tokio::time::interval(Duration::from_secs(300));
        loop {
            interval.tick().await;
            reporter.save_periodic_report().await;
        }
    });
}
```

================================================================================
七、报告格式
================================================================================

7.1 JSON报告文件

文件位置: data/heartbeat_report_YYYY-MM-DD_HH-MM.json

```json
{
  "report_time": "2026-03-29T10:35:00Z",
  "heartbeat_sequence": 5,
  "duration_minutes": 5,
  "total_reports": 47,
  "active_points": 8,
  "stale_points": 2,
  "mode": "Full",
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
      "last_report_at": "2026-03-29T10:34:00Z",
      "is_stale": false
    }
  ],
  "stale_points": [
    { "point_id": "DT-003", "since_sequence": 2 }
  ]
}
```

7.2 终端快速查看

```rust
// 查看摘要
let summary = HeartbeatReporter::global().summary().await;
println!("{:?}", summary);

// 查看失联点
let stale = HeartbeatReporter::global().get_stale_points().await;
for point in stale {
    eprintln!("[!!] {} 失联", point);
}

// 查看断裂流程
let report = HeartbeatReporter::global().generate_report().await;
if !report.stale_points.is_empty() {
    eprintln!("流程断裂检测:");
    for point in &report.stale_points {
        eprintln!("  [!!] {} 自 HB_{} 后未报到", point.point_id, point.since_sequence);
    }
}
```

================================================================================
八、集成测试方案 (新增)
================================================================================

8.1 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_token_generation() {
        let clock = HeartbeatClock::new(60);
        let t1 = clock.try_generate().unwrap();
        let t2 = clock.try_generate().unwrap();
        assert_eq!(t2.sequence - t1.sequence, 1);
    }

    #[tokio::test]
    async fn test_report_and_query() {
        HeartbeatReporter::init(ReporterConfig::default());
        let reporter = HeartbeatReporter::global();

        let token = reporter.generate_token().await;
        reporter.report(&token, "BS-001", "b_data_source", "test_fn", "test.rs").await;

        let summary = reporter.summary().await;
        assert_eq!(summary.total_reports, 1);
        assert!(summary.active_points.contains(&"BS-001".to_string()));
    }

    #[tokio::test]
    async fn test_stale_detection() {
        let config = ReporterConfig {
            stale_threshold: 2,
            ..Default::default()
        };
        HeartbeatReporter::init(config);
        let reporter = HeartbeatReporter::global();

        // 报到两次
        for _ in 0..2 {
            let token = reporter.generate_token().await;
            reporter.report(&token, "BS-001", "b_data_source", "test", "test.rs").await;
        }

        // 生成3个新心跳但不报到该点
        for _ in 0..3 {
            reporter.generate_token().await;
        }

        let stale = reporter.get_stale_points().await;
        assert!(stale.contains(&"BS-001".to_string()));
    }

    #[tokio::test]
    async fn test_sampling_mode() {
        HeartbeatReporter::init(ReporterConfig::default());
        let reporter = HeartbeatReporter::global();

        reporter.set_mode(ReportMode::Sampling(2)).await;

        let mut count = 0;
        for _ in 0..100 {
            let token = reporter.generate_token().await;
            let before = reporter.summary().await.total_reports;
            reporter.report(&token, "BS-001", "b_data_source", "test", "test.rs").await;
            let after = reporter.summary().await.total_reports;
            if after > before { count += 1; }
        }

        // 应该在 30-70 之间 (Sampling(2) 约 50%)
        assert!(count > 20 && count < 80);
    }
}
```

8.2 集成测试

```rust
#[tokio::test]
async fn test_event_flow_heartbeat() {
    // 1. 初始化
    HeartbeatReporter::init(ReporterConfig::default());
    let reporter = HeartbeatReporter::global();

    // 2. 模拟事件流
    let token = reporter.generate_token().await;

    // BS-001 报到
    reporter.report(&token, "BS-001", "b_data_source", "on_kline", "mod.rs").await;

    // CP-001 报到
    reporter.report(&token, "CP-001", "c_data_process", "process", "mod.rs").await;

    // 3. 验证流程连通
    let report = reporter.generate_report().await;
    assert!(report.points_detail.iter().any(|p| p.point_id == "BS-001"));
    assert!(report.points_detail.iter().any(|p| p.point_id == "CP-001"));

    // 4. 验证断裂检测 (DT-002 未报到)
    let stale = reporter.get_stale_points().await;
    assert!(stale.is_empty()); // 因为报到过，不是失联
}
```

================================================================================
九、关键功能点清单 (P0优先)
================================================================================

9.1 P0测试点 (首次实现)

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

================================================================================
十、定时报告生成
================================================================================

10.1 报告触发

| 触发方式 | 间隔 | 说明 |
|----------|------|------|
| 定时生成 | 每5分钟 | 自动生成JSON报告 |
| 查询触发 | 手动 | 通过API/命令查询 |
| 异常触发 | 断裂检测 | 发现断裂时立即报告 |

10.2 报告保存策略

- 位置: data/heartbeat_reports/
- 命名: heartbeat_report_YYYY-MM-DD_HH-MM.json
- 保留: 最近24小时 + 每天一份
- 清理: 启动时清理过期报告
- 大小限制: 单文件最大 100MB，超出时拆分

================================================================================
十一、预期效果
================================================================================

11.1 正常状态

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

11.2 断裂状态

```
HB_3 断裂检测:
  [!!] DT-003 h_15m::Executor 失联 (自 HB_1 后未报到)
  [!!] FE-008 TraderManager 失联 (自 HB_1 后未报到)

流程断裂位置: d_checktable -> f_engine
最后活跃: HB_2 at e_risk_monitor
```

================================================================================
十二、实现优先级
================================================================================

Phase 1: 核心框架
  - HeartbeatClock 心跳时钟
  - HeartbeatToken 数据结构
  - HeartbeatReporter 核心逻辑
  - 报到接口 report()
  - 便捷宏 heartbeat!()
  - 配置化 TEST_POINT_NAMES

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
  - 文件清理

Phase 4: 高级功能
  - Sampling 采样模式
  - Stale 失联检测
  - 自动降级
  - 其他P1/P2测试点

================================================================================
                              文档结束
================================================================================
