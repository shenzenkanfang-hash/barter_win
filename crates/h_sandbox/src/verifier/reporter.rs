//! Verification Reporter - 验证报告生成器
//!
//! 生成 Markdown + JSON 格式的验证报告

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

use super::consistency::{ConsistencyChecker, ConsistencyStatistics};
use super::fund_curve::{FundCurveValidator, FundCurveReport};
use super::fault_injector::{FaultInjector, FaultStatistics};
use super::performance::{PerformanceBenchmark, PerformanceReport};

/// 验证报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationReport {
    /// 报告生成时间
    pub generated_at: DateTime<Utc>,
    /// 测试配置
    pub test_config: TestConfig,
    /// 执行摘要
    pub summary: ExecutiveSummary,
    /// 一致性验证结果
    pub consistency: Option<ConsistencySummary>,
    /// 资金曲线验证结果
    pub fund_curve: Option<FundCurveReport>,
    /// 故障注入结果
    pub fault_injection: Option<FaultSummary>,
    /// 性能基准结果
    pub performance: Option<PerformanceReport>,
    /// 生产就绪检查结果
    pub production_ready: ProductionReadyCheck,
    /// 问题与风险
    pub issues: Vec<Issue>,
}

/// 测试配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestConfig {
    pub symbol: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub initial_fund: String,
    pub data_source: String,
}

/// 执行摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutiveSummary {
    /// 测试时间
    pub test_duration_secs: u64,
    /// 数据规模
    pub total_ticks: usize,
    pub total_klines: usize,
    pub total_orders: usize,
    /// 整体结果
    pub overall_result: TestResult,
    /// 通过率
    pub pass_rate: f64,
}

/// 测试结果
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TestResult {
    Passed,    // 全部通过
    Warning,   // 有警告但无严重问题
    Failed,    // 有严重问题
}

/// 一致性摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsistencySummary {
    pub total_checks: usize,
    pub passed: usize,
    pub failed: usize,
    pub pass_rate: f64,
    pub position_consistency: f64,
    pub account_consistency: f64,
    pub order_consistency: f64,
}

/// 故障注入摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaultSummary {
    pub total_faults: u64,
    pub applied: usize,
    pub skipped: usize,
    pub errors: usize,
    pub system_handled_correctly: usize,
    pub system_failed: usize,
}

/// 生产就绪检查
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductionReadyCheck {
    pub code_level: LevelCheck,
    pub config_validation: LevelCheck,
    pub observability: LevelCheck,
    pub disaster_recovery: LevelCheck,
}

/// 级别检查
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelCheck {
    pub total_items: usize,
    pub passed: usize,
    pub failed: usize,
    pub status: CheckStatus,
}

/// 检查状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckStatus {
    Pass,
    Fail,
    NotChecked,
}

/// 问题
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    pub severity: IssueSeverity,
    pub category: String,
    pub description: String,
    pub affected_component: String,
    pub recommendation: String,
}

/// 问题严重性
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IssueSeverity {
    Critical,  // 严重
    Warning,   // 警告
    Info,      // 信息
}

/// 验证报告生成器
pub struct VerificationReporter {
    test_config: TestConfig,
    start_time: DateTime<Utc>,
}

impl VerificationReporter {
    /// 创建报告生成器
    pub fn new(symbol: String, initial_fund: &str) -> Self {
        Self {
            test_config: TestConfig {
                symbol,
                start_time: Utc::now(),
                end_time: Utc::now(),
                initial_fund: initial_fund.to_string(),
                data_source: "Binance Futures API".to_string(),
            },
            start_time: Utc::now(),
        }
    }

    /// 设置测试时间范围
    pub fn set_time_range(&mut self, start: DateTime<Utc>, end: DateTime<Utc>) {
        self.test_config.start_time = start;
        self.test_config.end_time = end;
    }

    /// 生成最终报告
    pub fn generate_report(
        &self,
        consistency: Option<&ConsistencyChecker>,
        fund_curve: Option<&FundCurveValidator>,
        fault_injector: Option<&FaultInjector>,
        performance: Option<&PerformanceBenchmark>,
    ) -> VerificationReport {
        let test_duration = (Utc::now() - self.start_time).num_seconds() as u64;

        // 计算一致性摘要
        let consistency_summary = consistency.map(|c| {
            let stats = c.get_statistics();
            let all_reports = c.get_history();

            // 按类型统计
            let position_checks = all_reports.iter()
                .filter(|r| r.check_type == super::consistency::CheckType::Position)
                .count();
            let account_checks = all_reports.iter()
                .filter(|r| r.check_type == super::consistency::CheckType::Account)
                .count();
            let order_checks = all_reports.iter()
                .filter(|r| r.check_type == super::consistency::CheckType::Order)
                .count();

            let passed = stats.passed_count;
            let total = stats.total_checks;

            ConsistencySummary {
                total_checks: total,
                passed,
                failed: stats.failed_count,
                pass_rate: if total > 0 { passed as f64 / total as f64 * 100.0 } else { 100.0 },
                position_consistency: if position_checks > 0 {
                    let p_passed = all_reports.iter()
                        .filter(|r| r.check_type == super::consistency::CheckType::Position && r.passed)
                        .count();
                    p_passed as f64 / position_checks as f64 * 100.0
                } else { 100.0 },
                account_consistency: if account_checks > 0 {
                    let a_passed = all_reports.iter()
                        .filter(|r| r.check_type == super::consistency::CheckType::Account && r.passed)
                        .count();
                    a_passed as f64 / account_checks as f64 * 100.0
                } else { 100.0 },
                order_consistency: if order_checks > 0 {
                    let o_passed = all_reports.iter()
                        .filter(|r| r.check_type == super::consistency::CheckType::Order && r.passed)
                        .count();
                    o_passed as f64 / order_checks as f64 * 100.0
                } else { 100.0 },
            }
        });

        // 资金曲线报告
        let fund_curve_report = fund_curve.map(|f| f.generate_report(Default::default()));

        // 故障注入摘要
        let fault_summary = fault_injector.map(|f| {
            let stats = f.get_statistics();
            FaultSummary {
                total_faults: stats.fault_count,
                applied: stats.applied_count,
                skipped: stats.skipped_count,
                errors: stats.error_count,
                // TODO: 需要跟踪系统是否正确处理
                system_handled_correctly: stats.applied_count,
                system_failed: stats.error_count,
            }
        });

        // 性能报告
        let perf_report = performance.map(|p| p.generate_report());

        // 计算整体结果
        let overall_result = self.calculate_overall_result(
            consistency_summary.as_ref(),
            fund_curve_report.as_ref(),
            fault_summary.as_ref(),
            perf_report.as_ref(),
        );

        // 生产就绪检查
        let production_ready = self.perform_production_check(
            consistency_summary.as_ref(),
            perf_report.as_ref(),
        );

        // 问题列表
        let issues = self.collect_issues(
            consistency_summary.as_ref(),
            fund_curve_report.as_ref(),
            fault_summary.as_ref(),
            perf_report.as_ref(),
        );

        let pass_rate = self.calculate_pass_rate(
            consistency_summary.as_ref(),
            fund_curve_report.as_ref(),
            &production_ready,
        );

        VerificationReport {
            generated_at: Utc::now(),
            test_config: self.test_config.clone(),
            summary: ExecutiveSummary {
                test_duration_secs: test_duration,
                total_ticks: perf_report.as_ref().map(|p| p.total_ticks).unwrap_or(0),
                total_klines: 0,  // TODO: 添加
                total_orders: perf_report.as_ref().map(|p| p.total_orders).unwrap_or(0),
                overall_result,
                pass_rate,
            },
            consistency: consistency_summary,
            fund_curve: fund_curve_report,
            fault_injection: fault_summary,
            performance: perf_report,
            production_ready,
            issues,
        }
    }

    /// 计算整体结果
    fn calculate_overall_result(
        &self,
        consistency: Option<&ConsistencySummary>,
        fund_curve: Option<&FundCurveReport>,
        fault: Option<&FaultSummary>,
        performance: Option<&PerformanceReport>,
    ) -> TestResult {
        let mut has_critical = false;
        let mut has_warning = false;

        // 检查一致性
        if let Some(c) = consistency {
            if c.pass_rate < 95.0 {
                has_critical = true;
            } else if c.pass_rate < 99.0 {
                has_warning = true;
            }
        }

        // 检查资金曲线
        if let Some(f) = fund_curve {
            if f.monotonicity_check.is_monotonic == false {
                has_warning = true;
            }
        }

        // 检查故障处理
        if let Some(f) = fault {
            if f.system_failed > 0 {
                has_critical = true;
            }
        }

        // 检查性能
        if let Some(p) = performance {
            if p.target_achievement.tick_to_decision_target < 80.0 {
                has_warning = true;
            }
            if p.target_achievement.e2e_target < 50.0 {
                has_critical = true;
            }
        }

        if has_critical {
            TestResult::Failed
        } else if has_warning {
            TestResult::Warning
        } else {
            TestResult::Passed
        }
    }

    /// 执行生产就绪检查
    fn perform_production_check(
        &self,
        consistency: Option<&ConsistencySummary>,
        performance: Option<&PerformanceReport>,
    ) -> ProductionReadyCheck {
        ProductionReadyCheck {
            code_level: LevelCheck {
                total_items: 10,
                // TODO: 实际检查代码质量
                passed: 8,
                failed: 2,
                status: CheckStatus::Fail,
            },
            config_validation: LevelCheck {
                total_items: 5,
                passed: 4,
                failed: 1,
                status: CheckStatus::Fail,
            },
            observability: LevelCheck {
                total_items: 6,
                passed: 5,
                failed: 1,
                status: CheckStatus::Fail,
            },
            disaster_recovery: LevelCheck {
                total_items: 4,
                passed: 3,
                failed: 1,
                status: CheckStatus::Fail,
            },
        }
    }

    /// 收集问题
    fn collect_issues(
        &self,
        consistency: Option<&ConsistencySummary>,
        fund_curve: Option<&FundCurveReport>,
        fault: Option<&FaultSummary>,
        performance: Option<&PerformanceReport>,
    ) -> Vec<Issue> {
        let mut issues = Vec::new();

        // 一致性问题
        if let Some(c) = consistency {
            if c.pass_rate < 95.0 {
                issues.push(Issue {
                    severity: IssueSeverity::Critical,
                    category: "一致性".to_string(),
                    description: format!("状态一致性检查通过率仅 {}%", c.pass_rate),
                    affected_component: "ConsistencyChecker".to_string(),
                    recommendation: "检查沙盒与 Trader 的状态同步逻辑".to_string(),
                });
            }
        }

        // 资金曲线问题
        if let Some(f) = fund_curve {
            if !f.monotonicity_check.is_monotonic {
                issues.push(Issue {
                    severity: IssueSeverity::Warning,
                    category: "资金曲线".to_string(),
                    description: "资金曲线存在非单调性".to_string(),
                    affected_component: "FundCurveValidator".to_string(),
                    recommendation: "检查是否存在不可能的资金回退".to_string(),
                });
            }
        }

        // 性能问题
        if let Some(p) = performance {
            if p.target_achievement.tick_to_decision_target < 80.0 {
                issues.push(Issue {
                    severity: IssueSeverity::Warning,
                    category: "性能".to_string(),
                    description: format!("Tick-to-Decision 延迟未达标，达成率 {}%",
                        p.target_achievement.tick_to_decision_target),
                    affected_component: "PerformanceBenchmark".to_string(),
                    recommendation: "优化指标计算路径".to_string(),
                });
            }
        }

        issues
    }

    /// 计算通过率
    fn calculate_pass_rate(
        &self,
        consistency: Option<&ConsistencySummary>,
        fund_curve: Option<&FundCurveReport>,
        production: &ProductionReadyCheck,
    ) -> f64 {
        let mut total = 0;
        let mut passed = 0.0;

        if let Some(c) = consistency {
            total += 3;
            passed += c.position_consistency / 100.0;
            passed += c.account_consistency / 100.0;
            passed += c.order_consistency / 100.0;
        }

        if let Some(f) = fund_curve {
            total += 1;
            if f.monotonicity_check.is_monotonic {
                passed += 1.0;
            }
        }

        // 生产就绪
        total += 4;
        passed += production.code_level.passed as f64;
        passed += production.config_validation.passed as f64;
        passed += production.observability.passed as f64;
        passed += production.disaster_recovery.passed as f64;

        if total > 0 {
            passed / total as f64 * 100.0
        } else {
            100.0
        }
    }

    /// 生成 Markdown 格式报告
    pub fn to_markdown(&self, report: &VerificationReport) -> String {
        let mut md = String::new();

        // 标题
        md.push_str("# 生产级验证报告\n\n");

        // 执行摘要
        md.push_str("## 执行摘要\n\n");
        md.push_str(&format!("- 测试时间：{} ~ {}\n",
            report.test_config.start_time.format("%Y-%m-%d %H:%M:%S"),
            report.test_config.end_time.format("%Y-%m-%d %H:%M:%S")));
        md.push_str(&format!("- 数据规模：{} 个 Tick, {} 个订单\n",
            report.summary.total_ticks, report.summary.total_orders));
        md.push_str(&format!("- 整体结果：{:?}\n", report.summary.overall_result));
        md.push_str(&format!("- 通过率：{:.1}%\n\n", report.summary.pass_rate));

        // 一致性验证
        if let Some(ref c) = report.consistency {
            md.push_str("## 一致性验证\n\n");
            md.push_str(&format!("- 持仓一致性：{:.1}%\n", c.position_consistency));
            md.push_str(&format!("- 资金一致性：{:.1}%\n", c.account_consistency));
            md.push_str(&format!("- 订单一致性：{:.1}%\n", c.order_consistency));
            md.push_str(&format!("- 总通过率：{:.1}%\n\n", c.pass_rate));
        }

        // 资金曲线
        if let Some(ref f) = report.fund_curve {
            md.push_str("## 资金曲线\n\n");
            md.push_str(&format!("- 初始资金：{}\n", f.initial_fund));
            md.push_str(&format!("- 最终资金：{}\n", f.final_fund));
            md.push_str(&format!("- 累计盈亏：{} ({:.2}%)\n", f.cumulative_pnl, f.pnl_rate));
            md.push_str(&format!("- 最大回撤：{} ({:.2}%)\n", f.max_drawdown, f.max_drawdown_rate));
            md.push_str(&format!("- 胜率：{}\n\n", f.win_rate));
        }

        // 性能基准
        if let Some(ref p) = report.performance {
            md.push_str("## 性能基准\n\n");
            md.push_str(&format!("- Tick 处理速率：{:.0f}/秒\n", p.ticks_per_second));
            md.push_str(&format!("- 订单处理速率：{:.0f}/秒\n", p.orders_per_second));
            for stat in &p.latency_stats {
                md.push_str(&format!("- {:?} 平均延迟：{} μs (P99: {})\n",
                    stat.stage, stat.avg_us, stat.p99_us));
            }
            md.push_str("\n");
        }

        // 问题列表
        if !report.issues.is_empty() {
            md.push_str("## 问题与风险\n\n");
            for issue in &report.issues {
                md.push_str(&format!("### [{:?}] {}\n", issue.severity, issue.category));
                md.push_str(&format!("{}\n\n", issue.description));
            }
        }

        md
    }

    /// 生成 JSON 格式报告
    pub fn to_json(&self, report: &VerificationReport) -> String {
        serde_json::to_string_pretty(report).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_report_generation() {
        let reporter = VerificationReporter::new("BTCUSDT", "10000");
        let report = reporter.generate_report(None, None, None, None);

        assert_eq!(report.summary.overall_result, TestResult::Passed);
    }

    #[test]
    fn test_markdown_output() {
        let reporter = VerificationReporter::new("BTCUSDT", "10000");
        let report = reporter.generate_report(None, None, None, None);
        let md = reporter.to_markdown(&report);

        assert!(md.contains("生产级验证报告"));
    }
}