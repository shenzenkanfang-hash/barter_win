//! Reporter - 报告生成
//!
//! 生成性能测试报告

use crate::perf_test::{PerfTestConfig, PerfTestResult, PerfStats, TestConclusion};

/// 报告生成器
pub struct Reporter;

impl Reporter {
    /// 生成报告
    pub fn generate(config: &PerfTestConfig, stats: PerfStats) -> PerfTestResult {
        let conclusion = Self::analyze(&stats);

        PerfTestResult {
            stats,
            config: config.clone(),
            conclusion,
        }
    }

    /// 分析测试结果
    fn analyze(stats: &PerfStats) -> TestConclusion {
        // 检查成功率
        if stats.success_rate() < 95.0 {
            return TestConclusion::Failed(format!(
                "成功率过低: {:.2}% (目标: >= 95%)",
                stats.success_rate()
            ));
        }

        // 检查 P99 延迟
        if stats.p99_ms > 50.0 {
            return TestConclusion::Failed(format!(
                "P99 延迟过高: {:.2}ms (目标: < 50ms)",
                stats.p99_ms
            ));
        }

        // 检查平均延迟
        if stats.avg_ms > 16.0 {
            return TestConclusion::Warning(format!(
                "平均延迟 {:.2}ms 接近目标 16ms，留有余量不足",
                stats.avg_ms
            ));
        }

        // 检查最大延迟
        if stats.max_ms > 100.0 {
            return TestConclusion::Warning(format!(
                "最大延迟 {:.2}ms 较高，可能影响极端行情处理",
                stats.max_ms
            ));
        }

        TestConclusion::Pass
    }

    /// 打印报告
    pub fn print(result: &PerfTestResult) {
        let stats = &result.stats;
        let config = &result.config;

        println!();
        println!("╔══════════════════════════════════════════════════════════════╗");
        println!("║                    性能测试报告                                  ║");
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!("║  数据源: {}                                   ║", config.csv_path);
        println!("║  品种: {}                                        ║", config.symbol);
        println!("║  测试模式: {}                               ║", if config.fast_mode { "快速" } else { "实时" });
        println!("║  tick 间隔: {}ms                                   ║", config.tick_interval_ms);
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!("║  测试时长: {:.2}s                                      ║", stats.duration_secs);
        println!("║  Tick 总数: {}                                        ║", stats.total);
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!("║                      处理性能                               ║");
        println!("║  平均延迟: {:.2}ms                                       ║", stats.avg_ms);
        println!("║  中位数:   {:.2}ms                                       ║", stats.p50_ms);
        println!("║  P95:      {:.2}ms                                       ║", stats.p95_ms);
        println!("║  P99:      {:.2}ms                                       ║", stats.p99_ms);
        println!("║  最大延迟: {:.2}ms                                       ║", stats.max_ms);
        println!("║  最小延迟: {:.2}ms                                       ║", stats.min_ms);
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!("║                      吞吐量                                 ║");
        println!("║  实际处理: {:.0} ticks/s                                  ║", stats.throughput);
        println!("║  理论最大: {:.0} ticks/s                                  ║", 1000.0 / config.tick_interval_ms as f64);
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!("║                      成功率                                 ║");
        println!("║  成功: {} ({:.2}%)                                       ║", stats.success, stats.success_rate());
        println!("║  失败: {} ({:.2}%)                                       ║", stats.failed, 100.0 - stats.success_rate());
        println!("╠══════════════════════════════════════════════════════════════╣");

        // 结论
        let conclusion_str = match &result.conclusion {
            TestConclusion::Pass => "✅ PASS - 系统可满足实盘要求",
            TestConclusion::Warning(msg) => &format!("⚠️  WARNING - {}", msg),
            TestConclusion::Failed(msg) => &format!("❌ FAILED - {}", msg),
        };
        println!("║  {}", format!("{:<62}", conclusion_str));
        println!("╚══════════════════════════════════════════════════════════════╝");
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_pass() {
        let stats = PerfStats {
            total: 1000,
            success: 999,
            failed: 1,
            avg_ms: 3.0,
            p50_ms: 2.5,
            p95_ms: 5.0,
            p99_ms: 10.0,
            max_ms: 20.0,
            min_ms: 1.0,
            throughput: 1000.0,
            duration_secs: 1.0,
        };

        let config = PerfTestConfig::default();
        let result = Reporter::generate(&config, stats);

        assert!(matches!(result.conclusion, TestConclusion::Pass));
    }

    #[test]
    fn test_analyze_fail() {
        let stats = PerfStats {
            total: 1000,
            success: 900,
            failed: 100,
            avg_ms: 3.0,
            p50_ms: 2.5,
            p95_ms: 5.0,
            p99_ms: 100.0,
            max_ms: 200.0,
            min_ms: 1.0,
            throughput: 1000.0,
            duration_secs: 1.0,
        };

        let config = PerfTestConfig::default();
        let result = Reporter::generate(&config, stats);

        assert!(matches!(result.conclusion, TestConclusion::Failed(_)));
    }
}
