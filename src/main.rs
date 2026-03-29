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
//!
//! v3.0: 心跳报到全链路集成

use std::sync::Arc;

use a_common::heartbeat::{self as hb, Config as HbConfig, Token as HeartbeatToken};
use b_data_mock::{
    api::{MockApiGateway, MockConfig},
    models::KLine,
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
const HEARTBEAT_INTERVAL_MS: u64 = 1000;

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
    let kline_stream = create_mock_kline_stream()?;
    let trader = create_trader().await?;

    tracing::info!("All components initialized");
    tracing::info!("Trader config: {:?}", trader.config());

    // 4. 设置心跳 Token 到各个组件
    setup_heartbeat_tokens(&kline_stream, &trader).await;

    // 5. 主循环（使用 mock 数据源）
    run_mock_trading_loop(trader, kline_stream, gateway).await?;

    // 6. 打印心跳报告
    print_heartbeat_report().await;

    Ok(())
}

// ============================================================================
// 心跳模块（使用 a_common/heartbeat 真实系统）
// ============================================================================

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

/// 生成新的心跳 Token
async fn generate_heartbeat_token() -> HeartbeatToken {
    let reporter = hb::global();
    reporter.generate_token().await
}

/// 设置心跳 Token 到各个组件
async fn setup_heartbeat_tokens(
    kline_stream: &b_data_mock::ws::kline_1m::Kline1mStream,
    trader: &Arc<Trader>,
) {
    let token = generate_heartbeat_token().await;
    tracing::info!("Generated heartbeat token: {}", token);

    // 设置到 Kline1mStream (BS-001)
    kline_stream.set_heartbeat_token(token.clone());

    // 设置到 Trader (DT-002)
    trader.set_heartbeat_token(token.clone());

    tracing::info!("Heartbeat tokens set to all components");
}

// ============================================================================
// Mock 数据源
// ============================================================================

/// 创建 Mock K线流（从模拟数据）
fn create_mock_kline_stream() -> Result<b_data_mock::ws::kline_1m::Kline1mStream, Box<dyn std::error::Error>> {
    // 生成模拟 K线数据
    let klines = generate_mock_klines(100);
    let kline_iter = Box::new(klines.into_iter());

    let stream = b_data_mock::ws::kline_1m::Kline1mStream::from_klines(
        SYMBOL.to_string(),
        kline_iter,
    );

    tracing::info!("Mock Kline1mStream created");
    Ok(stream)
}

/// 生成模拟 K线数据
fn generate_mock_klines(count: usize) -> Vec<KLine> {
    use chrono::{DateTime, Utc, Duration as ChronoDuration};
    use rust_decimal_macros::dec;
    use b_data_mock::models::Period;

    let base_price = dec!(0.0001);
    let start_time: DateTime<Utc> = Utc::now() - ChronoDuration::minutes((count as i64) * 60);

    (0..count)
        .map(|i| {
            let timestamp = start_time + ChronoDuration::minutes(i as i64);
            let variation = Decimal::from((i % 10) as i32) * dec!(0.00001);
            let open = base_price + variation;
            let close = base_price + variation + dec!(0.000005);
            let high = open.max(close) + dec!(0.000002);
            let low = open.min(close) - dec!(0.000002);

            KLine {
                symbol: SYMBOL.to_string(),
                period: Period::Minute(1),
                open,
                high,
                low,
                close,
                volume: dec!(1000) + Decimal::from(i as u32 % 100),
                timestamp,
                is_closed: true,
            }
        })
        .collect()
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
// 主循环（Mock 数据源测试）
// ============================================================================

async fn run_mock_trading_loop(
    trader: Arc<Trader>,
    mut kline_stream: b_data_mock::ws::kline_1m::Kline1mStream,
    gateway: MockApiGateway,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut loop_count = 0u64;
    let mut heartbeat_tick = interval(Duration::from_millis(HEARTBEAT_INTERVAL_MS));
    let mut kline_count = 0usize;

    tracing::info!("Main loop started with mock data source");

    loop {
        tokio::select! {
            // 心跳定时器
            _ = heartbeat_tick.tick() => {
                // 生成新的心跳 Token 并设置到各个组件
                let token = generate_heartbeat_token().await;

                // 设置到 Kline1mStream (BS-001)
                kline_stream.set_heartbeat_token(token.clone());

                // 设置到 Trader (DT-002)
                trader.set_heartbeat_token(token.clone());

                tracing::trace!("Heartbeat tick: {}", token);
            }

            // 数据流处理
            _ = tokio::time::sleep(Duration::from_millis(100)) => {
                loop_count += 1;

                // 获取下一个 K线数据（带心跳报到）
                if let Some(msg) = kline_stream.next_message_with_heartbeat().await {
                    kline_count += 1;

                    // 更新网关价格
                    if let Ok(price) = parse_kline_price(&msg) {
                        gateway.update_price(SYMBOL, price);
                    }

                    tracing::debug!(
                        "[Loop {}] Kline {} received, price: {:?}",
                        loop_count,
                        kline_count,
                        parse_kline_price(&msg).ok()
                    );
                }

                // 每100次迭代打印状态
                if loop_count % 100 == 0 {
                    let status = trader.current_status();
                    let summary = hb::global().summary().await;
                    tracing::info!(
                        "[Loop {}] System alive | Trader: {:?} | Heartbeat: {}/{} active",
                        loop_count,
                        status,
                        summary.active_count,
                        summary.total_points
                    );
                }

                // 安全退出：处理完所有 K线后退出
                if kline_count >= 100 {
                    tracing::info!("Kline limit reached ({}), exiting", kline_count);
                    break;
                }
            }
        }
    }

    tracing::info!(
        "Main loop exited after {} iterations, processed {} klines",
        loop_count,
        kline_count
    );

    Ok(())
}

/// 解析 K线价格
fn parse_kline_price(msg: &str) -> Result<Decimal, Box<dyn std::error::Error>> {
    #[derive(serde::Deserialize)]
    struct KlineMsg {
        c: String,
    }

    #[derive(serde::Deserialize)]
    struct OuterMsg {
        data: Option<KlineMsg>,
        // 对于非包装的 kline 数据
        #[serde(rename = "c")]
        close: Option<String>,
    }

    // 尝试解析包装格式
    if let Ok(outer) = serde_json::from_str::<OuterMsg>(msg) {
        if let Some(data) = outer.data {
            return Ok(data.c.parse()?);
        }
        if let Some(close) = outer.close {
            return Ok(close.parse()?);
        }
    }

    // 尝试解析直接格式
    #[derive(serde::Deserialize)]
    struct DirectMsg {
        c: String,
    }
    let direct = serde_json::from_str::<DirectMsg>(msg)?;
    Ok(direct.c.parse()?)
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

    // 获取失联点
    let stale_points = reporter.get_stale_points().await;
    if !stale_points.is_empty() {
        tracing::warn!("Stale points detected:");
        for point in stale_points {
            tracing::warn!("  [!!] {} since HB_{}", point.point_id, point.since_sequence);
        }
    }

    // 获取详细报告
    let report = reporter.generate_report().await;
    if !report.points_detail.is_empty() {
        tracing::info!("Active heartbeat points:");
        for point in report.points_detail.iter().take(10) {
            tracing::info!(
                "  - {} ({}): {} reports, last at {}",
                point.point_id,
                point.point_name,
                point.reports_count,
                point.last_report_at
            );
        }
    }

    tracing::info!("==============================================");

    // 保存 JSON 报告到文件
    if let Err(e) = reporter.save_report("heartbeat_report.json").await {
        tracing::warn!("Failed to save heartbeat report: {}", e);
    } else {
        tracing::info!("Heartbeat report saved to heartbeat_report.json");
    }
}
