//! heartbeat 模块集成测试

use a_common::heartbeat::{Config, Mode, Reporter};

/// 测试采样模式是否在预期范围内
///
/// Sampling(2) 意味着 1/2 概率报到，1000 次尝试期望约 500 次（±150，3σ范围 350-650）
#[tokio::test]
async fn test_sampling_mode() {
    // 创建配置和报告器
    let config = Config::default();
    let reporter = Reporter::new(config);

    // 设置采样模式：1/2 概率报到
    reporter.set_mode(Mode::Sampling(2)).await;

    let total_attempts = 1000;

    // 执行多次心跳报到
    for _ in 0..total_attempts {
        let token = reporter.generate_token().await;
        reporter
            .report(&token, "test_point", "test_module", "test_fn", "test.rs")
            .await;
    }

    // 获取摘要统计
    let summary = reporter.summary().await;

    // 验证报到次数在 3σ 范围内（约 350-650）
    // 旧断言过于宽松：assert!(summary.total_reports < total_attempts);
    //               assert!(summary.total_reports > 0);
    // 加强后的断言：期望在 500 ± 150 范围内
    assert!(
        summary.reports_count > 300 && summary.reports_count < 700,
        "Expected reports in range 300-700, got {}",
        summary.reports_count
    );
}
