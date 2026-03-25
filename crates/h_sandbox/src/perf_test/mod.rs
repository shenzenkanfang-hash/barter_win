//! 性能测试模块 - 异步回测引擎
//!
//! 用于测试系统处理性能，不改动原有引擎代码

mod tick_driver;
mod engine_driver;
mod tracker;
mod reporter;

pub use tick_driver::TickDriver;
pub use engine_driver::EngineDriver;
pub use tracker::{PerformanceTracker, PerfStats};
pub use reporter::Reporter;

/// 性能测试配置
#[derive(Debug, Clone)]
pub struct PerfTestConfig {
    /// parquet 文件路径
    pub parquet_path: String,
    /// tick 间隔（毫秒）
    pub tick_interval_ms: u64,
    /// 测试品种
    pub symbol: String,
    /// 测试时长（秒），0表示无限
    pub duration_secs: u64,
    /// 是否快速模式（不等待间隔）
    pub fast_mode: bool,
    /// 初始资金
    pub initial_fund: rust_decimal::Decimal,
}

impl Default for PerfTestConfig {
    fn default() -> Self {
        Self {
            parquet_path: String::new(),
            tick_interval_ms: 16,
            symbol: "BTCUSDT".to_string(),
            duration_secs: 60,
            fast_mode: false,
            initial_fund: rust_decimal::Decimal::from(10000),
        }
    }
}

/// 性能测试结果
#[derive(Debug, Clone)]
pub struct PerfTestResult {
    /// 统计信息
    pub stats: PerfStats,
    /// 测试配置
    pub config: PerfTestConfig,
    /// 结论
    pub conclusion: TestConclusion,
}

/// 测试结论
#[derive(Debug, Clone)]
pub enum TestConclusion {
    /// 通过 - 可满足实盘要求
    Pass,
    /// 警告 - 勉强可用，但有风险
    Warning(String),
    /// 失败 - 无法满足实盘要求
    Failed(String),
}

impl std::fmt::Display for TestConclusion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TestConclusion::Pass => write!(f, "✅ PASS - 系统可满足实盘要求"),
            TestConclusion::Warning(msg) => write!(f, "⚠️  WARNING - {}", msg),
            TestConclusion::Failed(msg) => write!(f, "❌ FAILED - {}", msg),
        }
    }
}
