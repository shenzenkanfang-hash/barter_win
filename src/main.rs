//! Trading System v5.3 - Full Project Test with b_data_mock
//!
//! 【架构】
//! - 唯一程序入口：main.rs
//! - 数据源：b_data_mock/ReplaySource（HOTUSDT 历史数据）
//! - 数据流：ReplaySource → KlineStreamGenerator → Kline1mStream
//! - 信号处理：SignalProcessor (CP-001)
//! - 策略层：Trader (DT-002)
//! - 风控层：RiskPreChecker (ER-001) + OrderCheck (ER-003)
//! - 交易所模拟：MockApiGateway
//!
//! v5.3: 完整项目集成测试，所有模块联动
//!       【回滚】移除心跳传递，保留 Python 对齐交易逻辑

use std::sync::Arc;

use a_common::heartbeat::{self as hb, Config as HbConfig};
use b_data_mock::{
    api::{mock_account::Side, MockApiGateway, MockConfig},
    replay_source::ReplaySource,
    ws::kline_1m::ws::Kline1mStream,
};
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

// 数据文件路径
const DATA_FILE: &str = "D:/RusProject/barter-rs-main/data/HOTUSDT_1m_20251009_20251011.csv";

// 心跳配置
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
    tracing::info!("Trading System v5.3 - Full Project Test");
    tracing::info!("Symbol: {}", SYMBOL);
    tracing::info!("Data: {}", DATA_FILE);
    tracing::info!("==============================================");

    // 2. 初始化心跳监控系统
    init_heartbeat().await;
    tracing::info!("Heartbeat system initialized");

    // 3. 创建所有组件
    let components = create_components().await?;

    tracing::info!("All components created:");
    tracing::info!("  - ReplaySource: loaded");
    tracing::info!("  - KlineStreamGenerator: ready");
    tracing::info!("  - SignalProcessor: registered {}", SYMBOL);
    tracing::info!("  - Trader: config loaded");
    tracing::info!("  - RiskPreChecker: registered {}", SYMBOL);
    tracing::info!("  - OrderCheck: ready");
    tracing::info!("  - MockApiGateway: balance={}", INITIAL_BALANCE);

    // 4. 运行完整数据流测试
    run_full_test(components).await?;

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
    tracing::info!("Heartbeat config: stale_threshold={}", stale_threshold);
}

// ============================================================================
// 组件创建
// ============================================================================

struct SystemComponents {
    // 数据源
    kline_stream: Arc<tokio::sync::Mutex<Kline1mStream>>,

    // 策略组件
    signal_processor: Arc<SignalProcessor>,
    trader: Arc<Trader>,

    // 风控组件
    risk_checker: Arc<RiskPreChecker>,
    order_checker: Arc<OrderCheck>,

    // 交易所模拟
    gateway: Arc<MockApiGateway>,
}

async fn create_components() -> Result<SystemComponents, Box<dyn std::error::Error>> {
    // ========== 数据源：ReplaySource ==========
    tracing::info!("Loading historical data from: {}", DATA_FILE);
    let replay_source = ReplaySource::from_csv(DATA_FILE).await?;
    let kline_count = replay_source.len();
    tracing::info!("Loaded {} K-lines from CSV", kline_count);

    // 创建 Kline1mStream（ReplaySource 实现了 Iterator trait，可直接传入）
    let kline_stream = {
        let stream = Kline1mStream::from_klines(
            SYMBOL.to_string(),
            Box::new(replay_source),
        );
        Arc::new(tokio::sync::Mutex::new(stream))
    };
    tracing::info!("[BS-001] Kline1mStream created");

    // ========== SignalProcessor (CP-001) ==========
    let signal_processor = Arc::new(SignalProcessor::new());
    signal_processor.register_symbol(SYMBOL);
    tracing::info!("[CP-001] SignalProcessor created");

    // ========== Trader (DT-002) ==========
    let trader = create_trader()?;
    tracing::info!("[DT-002] Trader created");

    // ========== RiskPreChecker (ER-001) ==========
    let mut risk_checker = RiskPreChecker::new(dec!(0.15), dec!(100.0));
    risk_checker.register_symbol(SYMBOL.to_string());
    let risk_checker = Arc::new(risk_checker);
    tracing::info!("[ER-001] RiskPreChecker created");

    // ========== OrderCheck (ER-003) ==========
    let order_checker = Arc::new(OrderCheck::new());
    tracing::info!("[ER-003] OrderCheck created");

    // ========== MockApiGateway ==========
    let mock_config = MockConfig::default();
    let gateway = Arc::new(MockApiGateway::new(INITIAL_BALANCE, mock_config));
    tracing::info!("[Gateway] MockApiGateway created, balance={}", INITIAL_BALANCE);

    Ok(SystemComponents {
        kline_stream,
        signal_processor,
        trader,
        risk_checker,
        order_checker,
        gateway,
    })
}

fn create_trader() -> Result<Arc<Trader>, Box<dyn std::error::Error>> {
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

    let executor_config = ExecutorConfig {
        symbol: SYMBOL.to_string(),
        order_interval_ms: trader_config.order_interval_ms,
        initial_ratio: trader_config.initial_ratio,
        lot_size: trader_config.lot_size,
        max_position: trader_config.max_position,
    };
    let executor = Arc::new(Executor::new(executor_config));

    let repository = Arc::new(Repository::new(SYMBOL, DB_PATH)?);

    let store: StoreRef = b_data_source::default_store().clone();
    let trader = Arc::new(Trader::new(trader_config, executor, repository, store));

    Ok(trader)
}

// ============================================================================
// 完整数据流测试（Python 对齐交易逻辑）
// ============================================================================

async fn run_full_test(components: SystemComponents) -> Result<(), Box<dyn std::error::Error>> {
    let mut heartbeat_tick = interval(Duration::from_millis(HEARTBEAT_INTERVAL_MS));
    let mut loop_count = 0u64;
    let mut kline_count = 0usize;
    let mut tick_count = 0usize;
    let mut trader_executions = 0usize;
    let mut signals_processed = 0usize;

    tracing::info!("Starting full data flow test...");

    loop {
        tokio::select! {
            // 心跳定时器
            _ = heartbeat_tick.tick() => {
                tracing::trace!("[HB] Heartbeat tick #{}", loop_count);
            }

            // 数据处理
            _ = tokio::time::sleep(Duration::from_millis(50)) => {
                loop_count += 1;

                // ========== 步骤1: 获取 K 线数据 (BS-001) ==========
                let kline_data = {
                    let mut stream = components.kline_stream.lock().await;
                    stream.next_message()
                };

                if let Some(data) = kline_data {
                    kline_count += 1;

                    // 解析 K 线
                    if let Ok(kline) = parse_kline_data(&data) {
                        let current_price = kline.close;

                        // 更新 MockApiGateway 价格
                        components.gateway.update_price(SYMBOL, current_price);

                        // ========== 步骤2: SignalProcessor 更新 (CP-001) ==========
                        let signal_result = components.signal_processor
                            .min_update(SYMBOL, kline.high, kline.low, kline.close, kline.volume);

                        if signal_result.is_ok() {
                            signals_processed += 1;
                        }

                        // ========== 步骤3: Trader 执行 (DT-002) ==========
                        // Python 对齐：execute_once_wal 执行完整交易逻辑
                        match components.trader.execute_once_wal().await {
                            Ok(result) => {
                                trader_executions += 1;

                                match &result {
                                    d_checktable::h_15m::ExecutionResult::Executed { qty, .. } => {
                                        tracing::debug!(
                                            "[Loop {}] Trader executed qty={}",
                                            loop_count,
                                            qty
                                        );

                                        // ========== 步骤4: 风控检查 (ER-001 + ER-003) ==========
                                        let risk_result = components.risk_checker
                                            .pre_check(
                                                SYMBOL,
                                                INITIAL_BALANCE,
                                                dec!(100),
                                                INITIAL_BALANCE,
                                            );

                                        // 订单检查
                                        let _order_result = components.order_checker
                                            .pre_check(
                                                &format!("order_{}", loop_count),
                                                SYMBOL,
                                                "h_15m_strategy",
                                                dec!(100),
                                                INITIAL_BALANCE,
                                                dec!(0),
                                            );

                                        if risk_result.is_ok() {
                                            // 执行模拟订单（买入）
                                            if let Ok(order) = components.gateway
                                                .place_order(SYMBOL, Side::Buy, *qty, None)
                                            {
                                                tracing::info!(
                                                    "[Order #{}] Filled: price={}, qty={}",
                                                    loop_count,
                                                    order.filled_price,
                                                    order.filled_qty
                                                );
                                                tick_count += 1;
                                            }
                                        }
                                    }
                                    d_checktable::h_15m::ExecutionResult::Skipped(reason) => {
                                        tracing::trace!("[Loop {}] Skipped: {}", loop_count, reason);
                                    }
                                    d_checktable::h_15m::ExecutionResult::Failed(e) => {
                                        tracing::warn!("[Loop {}] Failed: {}", loop_count, e);
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::error!("[Loop {}] Trader error: {}", loop_count, e);
                            }
                        }
                    }
                } else {
                    // 数据源耗尽
                    tracing::info!(
                        "ReplaySource exhausted at loop {}, ending test",
                        loop_count
                    );
                    break;
                }

                // 每 100 次迭代打印进度
                if loop_count % 100 == 0 {
                    let status = components.trader.current_status();
                    tracing::info!(
                        "[Progress {}] Klines: {}, Signals: {}, Trades: {}, Status: {:?}",
                        loop_count,
                        kline_count,
                        signals_processed,
                        trader_executions,
                        status
                    );
                }

                // 安全退出：数据耗尽或达到最大迭代
                if loop_count >= 1000 {
                    tracing::info!("Max iterations reached, ending test");
                    break;
                }
            }
        }
    }

    tracing::info!("Full test completed:");
    tracing::info!("  - Total loops: {}", loop_count);
    tracing::info!("  - K-lines processed: {}", kline_count);
    tracing::info!("  - Signals processed: {}", signals_processed);
    tracing::info!("  - Trader executions: {}", trader_executions);
    tracing::info!("  - Orders filled: {}", tick_count);

    Ok(())
}

fn parse_kline_data(data: &str) -> Result<MockKlineData, Box<dyn std::error::Error>> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "kebab-case")]
    struct RawKline {
        kline_start_time: i64,
        symbol: String,
        #[serde(rename = "open")]
        open_str: String,
        #[serde(rename = "close")]
        close_str: String,
        #[serde(rename = "high")]
        high_str: String,
        #[serde(rename = "low")]
        low_str: String,
        #[serde(rename = "volume")]
        volume_str: String,
        is_closed: bool,
    }

    // 尝试解析为 JSON（支持两种格式：有外层包裹 or 直接）
    let raw: RawKline = serde_json::from_str(data)
        .or_else(|_| serde_json::from_str(data))?;

    Ok(MockKlineData {
        open: raw.open_str.parse()?,
        close: raw.close_str.parse()?,
        high: raw.high_str.parse()?,
        low: raw.low_str.parse()?,
        volume: raw.volume_str.parse()?,
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
    tracing::info!("HEARTBEAT REPORT");
    tracing::info!("==============================================");

    let reporter = hb::global();
    let summary = reporter.summary().await;

    tracing::info!("Summary:");
    tracing::info!("  Total points: {}", summary.total_points);
    tracing::info!("  Active: {}", summary.active_count);
    tracing::info!("  Inactive: {}", summary.inactive_count);
    tracing::info!("  Total reports: {}", summary.reports_count);

    // 详细报告
    let report = reporter.generate_report().await;
    if !report.points_detail.is_empty() {
        tracing::info!("Points:");
        for point in &report.points_detail {
            let status = if point.is_stale { "STALE" } else { "OK" };
            tracing::info!(
                "  {} [{}]: {} reports, last={}",
                point.point_id,
                status,
                point.reports_count,
                point.last_report_at.format("%H:%M:%S")
            );
        }
    }

    // 失联点检查
    if !report.stale_points.is_empty() {
        tracing::warn!("STALE POINTS DETECTED:");
        for point in &report.stale_points {
            tracing::warn!("  - {} (stale since HB #{})", point.point_id, point.since_sequence);
        }
    }

    tracing::info!("==============================================");

    // 保存 JSON 报告
    if let Err(e) = reporter.save_report("heartbeat_report.json").await {
        tracing::warn!("Failed to save heartbeat report: {}", e);
    } else {
        tracing::info!("Heartbeat report saved to heartbeat_report.json");
    }
}
