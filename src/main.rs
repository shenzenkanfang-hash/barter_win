//! Trading System Rust Version - Main Entry (v5.2)
//!
//! 【架构】
//! - 唯一程序入口：main.rs
//! - 数据源：b_data_mock/Kline1mStream（模拟 K 线流）
//! - 信号处理：c_data_process/SignalProcessor
//! - 策略层：d_checktable/h_15m/Trader
//! - 风控层：e_risk_monitor
//! - 心跳监控：a_common/heartbeat（真实心跳系统）
//!
//! 【心跳报到流程】
//! 主循环每秒生成一个 Token，Token 在同一心跳周期内被所有组件复用：
//!
//!  Token 生成 → Kline1mStream(BS-001) → SignalProcessor(CP-001)
//!            → Trader(DT-002) → RiskPreChecker(ER-001) → OrderCheck(ER-003)
//!
//! v5.2: 完整心跳报到链路，所有已集成组件均报到

use std::sync::Arc;

use a_common::heartbeat::{self as hb, Config as HbConfig, Token as HeartbeatToken};
use b_data_mock::models::{KLine, Period};
use b_data_mock::ws::kline_1m::ws::Kline1mStream;
use chrono::{Duration as ChronoDuration, Utc};
use c_data_process::processor::SignalProcessor;
use d_checktable::h_15m::{
    Executor, ExecutorConfig, Repository, ThresholdConfig, Trader, TraderConfig,
};
use d_checktable::h_15m::trader::StoreRef;
use e_risk_monitor::risk::common::{OrderCheck, RiskPreChecker};
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
const LOOP_ITERATIONS: usize = 200;

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
    tracing::info!("Trading System v5.2 - Full Heartbeat Chain");
    tracing::info!("Symbol: {}", SYMBOL);
    tracing::info!("==============================================");

    // 2. 初始化心跳监控系统
    init_heartbeat().await;
    tracing::info!("Heartbeat system initialized");

    // 3. 创建所有组件并注入心跳系统
    let components = create_components().await?;

    tracing::info!("All components created with heartbeat integration");

    // 4. 主循环
    run_heartbeat_loop(components).await?;

    // 5. 打印心跳报告
    print_heartbeat_report().await;

    Ok(())
}

// ============================================================================
// 心跳模块
// ============================================================================

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

async fn generate_token() -> HeartbeatToken {
    let reporter = hb::global();
    reporter.generate_token().await
}

// ============================================================================
// 组件创建
// ============================================================================

struct SystemComponents {
    kline_stream: Arc<tokio::sync::Mutex<Kline1mStream>>,
    signal_processor: Arc<SignalProcessor>,
    trader: Arc<Trader>,
    risk_checker: Arc<RiskPreChecker>,
    order_checker: Arc<OrderCheck>,
}

async fn create_components() -> Result<SystemComponents, Box<dyn std::error::Error>> {
    // 生成初始心跳 Token 并设置到所有组件
    let initial_token = generate_token().await;

    // ========== Kline1mStream (BS-001) ==========
    let kline_stream = create_kline_stream().await;
    kline_stream.lock().await.set_heartbeat_token(initial_token.clone());
    tracing::info!("[BS-001] Kline1mStream created and token set");

    // ========== SignalProcessor (CP-001) ==========
    let signal_processor = Arc::new(SignalProcessor::new());
    signal_processor.set_heartbeat_token(initial_token.clone());
    signal_processor.register_symbol(SYMBOL);
    tracing::info!("[CP-001] SignalProcessor created and token set");

    // ========== Trader (DT-002) ==========
    let trader = create_trader().await?;
    trader.set_heartbeat_token(initial_token.clone());
    tracing::info!("[DT-002] Trader created and token set");

    // ========== RiskPreChecker (ER-001) ==========
    let mut risk_checker = RiskPreChecker::new(dec!(0.15), dec!(100.0));
    risk_checker.register_symbol(SYMBOL.to_string());
    let risk_checker = Arc::new(risk_checker);
    risk_checker.set_heartbeat_token(initial_token.clone());
    tracing::info!("[ER-001] RiskPreChecker created and token set");

    // ========== OrderCheck (ER-003) ==========
    let order_checker = Arc::new(OrderCheck::new());
    order_checker.set_heartbeat_token(initial_token.clone());
    tracing::info!("[ER-003] OrderCheck created and token set");

    Ok(SystemComponents {
        kline_stream,
        signal_processor,
        trader,
        risk_checker,
        order_checker,
    })
}

/// 创建 Kline1mStream（从模拟历史数据）
async fn create_kline_stream() -> Arc<tokio::sync::Mutex<Kline1mStream>> {
    // 生成模拟 K 线历史数据
    let klines = generate_mock_klines(SYMBOL, 60);

    // 创建流
    let stream = Kline1mStream::from_klines(
        SYMBOL.to_string(),
        Box::new(klines.into_iter()),
    );

    Arc::new(tokio::sync::Mutex::new(stream))
}

/// 生成模拟 K 线数据
fn generate_mock_klines(symbol: &str, count: usize) -> Vec<KLine> {
    let now = Utc::now();
    let base_price = dec!(0.0001);

    (0..count)
        .map(|i| {
            let variation = Decimal::from((i as i64) * 100); // 每次增加 0.000001
            let open = base_price + variation;
            let close = base_price + variation + dec!(500);
            let high = close + dec!(200);
            let low = open - dec!(200);
            let volume = dec!(1000) + Decimal::from((i as i64) * 100);

            KLine {
                symbol: symbol.to_string(),
                period: Period::Minute(1),
                open,
                high,
                low,
                close,
                volume,
                timestamp: now - ChronoDuration::minutes((count - i) as i64),
                is_closed: true,
            }
        })
        .collect()
}

/// 创建 Trader
async fn create_trader() -> Result<Arc<Trader>, Box<dyn std::error::Error>> {
    // Trader配置
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

    // Executor配置
    let executor_config = ExecutorConfig {
        symbol: SYMBOL.to_string(),
        order_interval_ms: trader_config.order_interval_ms,
        initial_ratio: trader_config.initial_ratio,
        lot_size: trader_config.lot_size,
        max_position: trader_config.max_position,
    };
    let executor = Arc::new(Executor::new(executor_config));

    // Repository
    let repository = Arc::new(Repository::new(SYMBOL, DB_PATH)?);

    // 使用默认 Store
    let store: StoreRef = b_data_source::default_store().clone();
    let trader = Arc::new(Trader::new(trader_config, executor, repository, store));

    Ok(trader)
}

// ============================================================================
// 主循环（心跳驱动）
// ============================================================================

async fn run_heartbeat_loop(components: SystemComponents) -> Result<(), Box<dyn std::error::Error>> {
    let mut loop_count = 0u64;
    let mut heartbeat_tick = interval(Duration::from_millis(HEARTBEAT_INTERVAL_MS));
    let mut kline_messages = 0usize;
    let mut trader_executions = 0usize;

    tracing::info!("Heartbeat loop started");

    loop {
        tokio::select! {
            // 心跳定时器：每秒生成新 Token，设置到所有组件
            _ = heartbeat_tick.tick() => {
                let token = generate_token().await;

                // 设置 Token 到所有组件
                components.kline_stream.lock().await.set_heartbeat_token(token.clone());
                components.signal_processor.set_heartbeat_token(token.clone());
                components.trader.set_heartbeat_token(token.clone());
                components.risk_checker.set_heartbeat_token(token.clone());
                components.order_checker.set_heartbeat_token(token.clone());

                tracing::trace!("[HB] Token {} distributed to all components", token.sequence);
            }

            // 数据流处理
            _ = tokio::time::sleep(Duration::from_millis(100)) => {
                loop_count += 1;

                // ========== 步骤1: Kline1mStream 获取数据 (BS-001) ==========
                let kline_data = {
                    let mut stream = components.kline_stream.lock().await;
                    stream.next_message_with_heartbeat().await
                };

                if let Some(data) = kline_data {
                    kline_messages += 1;

                    // 解析 K 线数据
                    if let Ok(kline) = parse_kline_data(&data) {
                        // ========== 步骤2: SignalProcessor 更新 (CP-001) ==========
                        let _ = components.signal_processor
                            .min_update_with_heartbeat(
                                SYMBOL,
                                kline.high,
                                kline.low,
                                kline.close,
                                kline.volume,
                            )
                            .await;

                        // ========== 步骤3: Trader 执行 (DT-002) ==========
                        match components.trader.execute_once_wal().await {
                            Ok(result) => {
                                trader_executions += 1;
                                match &result {
                                    d_checktable::h_15m::ExecutionResult::Executed { qty, .. } => {
                                        tracing::debug!(
                                            "[Loop {}] Trader executed qty={}",
                                            loop_count, qty
                                        );
                                    }
                                    d_checktable::h_15m::ExecutionResult::Skipped(reason) => {
                                        tracing::trace!("[Loop {}] Trader skipped: {}", loop_count, reason);
                                    }
                                    d_checktable::h_15m::ExecutionResult::Failed(e) => {
                                        tracing::warn!("[Loop {}] Trader failed: {}", loop_count, e);
                                    }
                                }

                                // ========== 步骤4: 如果有订单，调用风控 ==========
                                if matches!(result, d_checktable::h_15m::ExecutionResult::Executed { .. }) {
                                    // ER-001: RiskPreChecker
                                    let _ = components.risk_checker
                                        .pre_check_with_heartbeat(
                                            SYMBOL,
                                            INITIAL_BALANCE,
                                            dec!(100),
                                            INITIAL_BALANCE,
                                        )
                                        .await;

                                    // ER-003: OrderCheck
                                    let _ = components.order_checker
                                        .pre_check_with_heartbeat(
                                            "mock_order_1",
                                            SYMBOL,
                                            "strategy_1",
                                            dec!(100),
                                            INITIAL_BALANCE,
                                            dec!(0),
                                        )
                                        .await;
                                }
                            }
                            Err(e) => {
                                tracing::error!("[Loop {}] Trader error: {}", loop_count, e);
                            }
                        }
                    }
                }

                // 每50次迭代打印状态
                if loop_count % 50 == 0 {
                    let summary = hb::global().summary().await;
                    let status = components.trader.current_status();
                    tracing::info!(
                        "[Loop {}] Klines: {}, Trades: {}, Status: {:?}, Heartbeat: {}/{} active",
                        loop_count,
                        kline_messages,
                        trader_executions,
                        status,
                        summary.active_count,
                        summary.total_points
                    );
                }

                // 安全退出
                if loop_count >= LOOP_ITERATIONS as u64 {
                    tracing::info!(
                        "Loop limit reached ({}), exiting: {} klines, {} trader executions",
                        loop_count,
                        kline_messages,
                        trader_executions
                    );
                    break;
                }
            }
        }
    }

    tracing::info!("Heartbeat loop exited");
    Ok(())
}

/// 解析 K 线数据
fn parse_kline_data(data: &str) -> Result<MockKlineData, Box<dyn std::error::Error>> {
    #[derive(serde::Deserialize)]
    struct RawKline {
        kline_start_time: i64,
        kline_close_time: i64,
        symbol: String,
        interval: String,
        open: String,
        close: String,
        high: String,
        low: String,
        volume: String,
        is_closed: bool,
    }

    let raw: RawKline = serde_json::from_str(data)?;

    Ok(MockKlineData {
        open: raw.open.parse()?,
        close: raw.close.parse()?,
        high: raw.high.parse()?,
        low: raw.low.parse()?,
        volume: raw.volume.parse()?,
        is_closed: raw.is_closed,
    })
}

struct MockKlineData {
    open: Decimal,
    close: Decimal,
    high: Decimal,
    low: Decimal,
    volume: Decimal,
    is_closed: bool,
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
        tracing::info!("Heartbeat points:");
        for point in &report.points_detail {
            let status = if point.is_stale { "STALE" } else { "OK" };
            tracing::info!(
                "  {} ({}) [{}]: {} reports, last at {}",
                point.point_id,
                point.point_name,
                status,
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
