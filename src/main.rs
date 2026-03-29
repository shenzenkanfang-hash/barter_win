//! Trading System Rust Version - Main Entry (v5.0)
//!
//! 【架构】
//! - 唯一程序入口：main.rs
//! - 数据源：b_data_mock（模拟/回测）
//! - 策略层：d_checktable/h_15m（Pin策略 + Trader）
//! - 心跳监控：a_common/heartbeat（真实心跳系统）
//!
//! 【使用项目已有组件】
//! - MarketDataStoreImpl: 数据存储（b_data_source）
//! - MockApiGateway: 模拟网关（b_data_mock）
//! - Trader: 交易逻辑（d_checktable/h_15m）
//! - MinSignalGenerator: 信号生成（d_checktable/h_15m）
//! - HeartbeatReporter: 心跳监控（a_common/heartbeat）

use std::sync::Arc;

use a_common::heartbeat::{self as hb, Config as HbConfig};
use b_data_mock::{
    api::{MockApiGateway, MockConfig},
};
use d_checktable::h_15m::{
    Executor, ExecutorConfig, Repository, ThresholdConfig, Trader, TraderConfig,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tokio::time::{interval, Duration};
use tracing_subscriber::prelude::__tracing_subscriber_SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

// ============================================================================
// 常量配置
// ============================================================================

const INITIAL_BALANCE: Decimal = dec!(10000);
const SYMBOL: &str = "HOTUSDT";
const DB_PATH: &str = "D:/RusProject/barter-rs-main/data/trade_records.db";

// ============================================================================
// 主程序
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 初始化日志
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_target(true).with_level(true))
        .init();

    tracing::info!("==============================================");
    tracing::info!("Trading System v5.0 - Main Entry");
    tracing::info!("Symbol: {}", SYMBOL);
    tracing::info!("==============================================");

    // 2. 初始化心跳监控系统
    init_heartbeat().await;
    tracing::info!("Heartbeat system initialized");

    // 3. 初始化组件
    let gateway = init_gateway()?;
    let trader = create_trader().await?;

    tracing::info!("All components initialized");
    tracing::info!("Trader config: {:?}", trader.config());

    // 4. 主循环
    run_main_loop(trader, gateway).await?;

    // 5. 打印心跳报告
    print_heartbeat_report().await;

    Ok(())
}

// ============================================================================
// 心跳模块（使用 a_common/heartbeat 真实系统）
// ============================================================================

/// 心跳点名称
const POINT_MAIN_LOOP: &str = "DT-002";

/// 初始化心跳系统
async fn init_heartbeat() {
    let stale_threshold = 3u64;
    let config = HbConfig {
        stale_threshold,
        report_interval_secs: 300,
        max_file_age_hours: 24,
        max_file_size_mb: 100,
    };
    hb::init(config);
    tracing::info!("Heartbeat reporter initialized, stale_threshold={}", stale_threshold);
}

/// 全局报到（异步）
async fn heartbeat_report(point_id: &str, module: &str, function: &str) {
    let reporter = hb::global();
    let token = reporter.generate_token().await;
    reporter.report(&token, point_id, module, function, file!()).await;
}

// ============================================================================
// 组件初始化
// ============================================================================

fn init_gateway() -> Result<MockApiGateway, Box<dyn std::error::Error>> {
    let mock_config = MockConfig::new(INITIAL_BALANCE);
    let gateway = MockApiGateway::new(INITIAL_BALANCE, mock_config);

    tracing::info!("MockApiGateway initialized with balance: {}", INITIAL_BALANCE);

    Ok(gateway)
}

// ============================================================================
// Trader创建
// ============================================================================

async fn create_trader() -> Result<Arc<Trader>, Box<dyn std::error::Error>> {
    // 心跳报到
    heartbeat_report(POINT_MAIN_LOOP, "main", "create_trader").await;

    // 1. Trader配置（包含 Python 对齐阈值 v3.0）
    let trader_config = TraderConfig {
        symbol: SYMBOL.to_string(),
        interval_ms: 100,
        max_position: dec!(0.15),
        initial_ratio: dec!(0.05),
        db_path: DB_PATH.to_string(),
        order_interval_ms: 100,
        lot_size: dec!(0.001),
        thresholds: ThresholdConfig::default(),
    };

    tracing::info!("Trader config thresholds: {:?}", trader_config.thresholds);

    // 2. Executor配置
    let executor_config = ExecutorConfig {
        symbol: SYMBOL.to_string(),
        order_interval_ms: trader_config.order_interval_ms,
        initial_ratio: trader_config.initial_ratio,
        lot_size: trader_config.lot_size,
        max_position: trader_config.max_position,
    };
    let executor = Arc::new(Executor::new(executor_config));

    // 3. Repository
    let repository = Arc::new(Repository::new(SYMBOL, DB_PATH)?);

    // 4. 创建Trader（使用 with_default_store，内部自动转换 StoreRef）
    let trader = Arc::new(Trader::with_default_store(
        trader_config,
        executor,
        repository,
    ));

    tracing::info!("Trader created successfully");

    Ok(trader)
}

// ============================================================================
// 主循环
// ============================================================================

async fn run_main_loop(trader: Arc<Trader>, _gateway: MockApiGateway) -> Result<(), Box<dyn std::error::Error>> {
    let mut loop_count = 0u64;
    let mut main_tick = interval(Duration::from_millis(100));

    tracing::info!("Main loop started");

    loop {
        main_tick.tick().await;
        loop_count += 1;

        // 心跳：主循环（异步报到）
        heartbeat_report(POINT_MAIN_LOOP, "main", "run_main_loop").await;

        // 每1000次迭代打印状态
        if loop_count % 1000 == 0 {
            let status = trader.current_status();
            tracing::info!(
                "[Loop {}] System alive | Trader status: {:?}",
                loop_count,
                status
            );
        }

        // 安全退出：避免无限循环
        if loop_count >= 10000 {
            tracing::info!("Loop limit reached ({}), exiting", loop_count);
            break;
        }
    }

    Ok(())
}

// ============================================================================
// 心跳报告
// ============================================================================

async fn print_heartbeat_report() {
    tracing::info!("==============================================");
    tracing::info!("Heartbeat Report:");
    let reporter = hb::global();
    let summary = reporter.summary().await;
    tracing::info!(
        "  Total points: {}, Active: {}, Inactive: {}, Reports: {}",
        summary.total_points,
        summary.active_count,
        summary.inactive_count,
        summary.reports_count,
    );
    tracing::info!("==============================================");

    // 保存 JSON 报告到文件
    if let Err(e) = reporter.save_report("heartbeat_report.json").await {
        tracing::warn!("Failed to save heartbeat report: {}", e);
    } else {
        tracing::info!("Heartbeat report saved to heartbeat_report.json");
    }
}
