//! Trading System Rust Version - Main Entry (v5.1)
//!
//! 【架构】
//! - 唯一程序入口：main.rs
//! - 数据源：b_data_mock（模拟/回测）→ Store → Trader
//! - 策略层：d_checktable/h_15m（Pin策略 + Trader）
//! - 心跳监控：a_common/heartbeat（真实心跳系统）
//!
//! 【完整数据流】
//! Mock K线生成 → Store写入 → Trader.execute_once_wal() → 心跳报到
//!
//! v5.1: 完整数据流驱动测试，DT-002 报到

use std::sync::Arc;

use a_common::heartbeat::{self as hb, Config as HbConfig, Token as HeartbeatToken};
use b_data_source::{
    default_store,
    store::{MarketDataStore, MarketDataStoreImpl},
    ws::kline_1m::ws::KlineData,
};
use chrono::{Duration as ChronoDuration, Utc};
use d_checktable::h_15m::{
    Executor, ExecutorConfig, Repository, ThresholdConfig, Trader, TraderConfig,
};
use d_checktable::h_15m::trader::StoreRef;
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
const HISTORY_KLINES_COUNT: usize = 60; // 需要至少14根历史K线用于Z-score计算
const LOOP_ITERATIONS: usize = 100;

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
    tracing::info!("Trading System v5.1 - Full Data Flow Test");
    tracing::info!("Symbol: {}", SYMBOL);
    tracing::info!("==============================================");

    // 2. 初始化心跳监控系统
    init_heartbeat().await;
    tracing::info!("Heartbeat system initialized");

    // 3. 创建并填充 Store（使用全局默认 Store）
    let store = default_store();
    fill_store_with_mock_data(store, SYMBOL, HISTORY_KLINES_COUNT)?;
    tracing::info!("Store filled with {} mock history klines", HISTORY_KLINES_COUNT);

    // 4. 初始化 Trader（使用同一个 Store）
    let trader = create_trader(store).await?;

    tracing::info!("Trader initialized");
    tracing::info!("Initial store kline: {:?}", store.get_current_kline(SYMBOL).map(|k| k.close.clone()));

    // 5. 生成初始心跳 Token 并设置到各个组件
    let initial_token = generate_heartbeat_token().await;
    tracing::info!("Initial heartbeat token: {}", initial_token);

    // 设置到 Trader (DT-002)
    trader.set_heartbeat_token(initial_token.clone());

    // 6. 主循环（完整数据流驱动）
    run_full_data_flow_loop(trader, store).await?;

    // 7. 打印心跳报告
    print_heartbeat_report().await;

    Ok(())
}

// ============================================================================
// 心跳模块（使用 a_common/heartbeat 真实系统）
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

async fn generate_heartbeat_token() -> HeartbeatToken {
    let reporter = hb::global();
    reporter.generate_token().await
}

// ============================================================================
// Store 数据填充（Mock K线）
// ============================================================================

fn fill_store_with_mock_data(
    store: &Arc<MarketDataStoreImpl>,
    symbol: &str,
    count: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let now = Utc::now();
    let one_minute_ms: i64 = 60 * 1000;

    // 生成历史 K 线（闭合的）
    let base_price = 0.0001f64;
    for i in 0..count {
        let start_time = now - ChronoDuration::minutes(((count - i) as i64) + 1);
        let close_time = start_time + ChronoDuration::minutes(1);

        let variation = (i as f64) * 0.000001;
        let open = base_price + variation;
        let close = base_price + variation + 0.0000005;
        let high = open.max(close) + 0.0000002;
        let low = open.min(close) - 0.0000002;

        let kline = KlineData {
            kline_start_time: start_time.timestamp_millis(),
            kline_close_time: close_time.timestamp_millis(),
            symbol: symbol.to_string(),
            interval: "1m".to_string(),
            open: format!("{:.8}", open),
            close: format!("{:.8}", close),
            high: format!("{:.8}", high),
            low: format!("{:.8}", low),
            volume: format!("{:.2}", 1000.0 + (i as f64) * 10.0),
            is_closed: true,
        };

        // 写入 Store（闭合的K线会同时写入历史分区）
        store.write_kline(symbol, kline, true);
    }

    // 生成当前 K 线（未闭合的）
    let current_start = now - ChronoDuration::seconds(30);
    let current_close = now + ChronoDuration::seconds(30);

    let current_kline = KlineData {
        kline_start_time: current_start.timestamp_millis(),
        kline_close_time: current_close.timestamp_millis(),
        symbol: symbol.to_string(),
        interval: "1m".to_string(),
        open: format!("{:.8}", base_price + (count as f64) * 0.000001),
        close: format!("{:.8}", base_price + (count as f64) * 0.000001 + 0.0000003),
        high: format!("{:.8}", base_price + (count as f64) * 0.000001 + 0.0000005),
        low: format!("{:.8}", base_price + (count as f64) * 0.000001 - 0.0000001),
        volume: format!("{:.2}", 500.0),
        is_closed: false,
    };

    let current_close = current_kline.close.clone();
    store.write_kline(symbol, current_kline, false);

    tracing::debug!(
        "Store filled: {} history klines, current kline: {:?}",
        store.get_history_klines(symbol).len(),
        current_close
    );

    Ok(())
}

// ============================================================================
// Trader创建
// ============================================================================

async fn create_trader(
    store: &Arc<MarketDataStoreImpl>,
) -> Result<Arc<Trader>, Box<dyn std::error::Error>> {
    // 1. Trader配置
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

    // 4. 创建 Trader（使用传入的 store）
    let store_ref: StoreRef = store.clone();
    let trader = Arc::new(Trader::new(
        trader_config,
        executor,
        repository,
        store_ref,
    ));

    tracing::info!("Trader created successfully with injected store");

    Ok(trader)
}

// ============================================================================
// 主循环（完整数据流驱动）
// ============================================================================

async fn run_full_data_flow_loop(
    trader: Arc<Trader>,
    store: &Arc<MarketDataStoreImpl>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut loop_count = 0u64;
    let mut heartbeat_tick = interval(Duration::from_millis(HEARTBEAT_INTERVAL_MS));
    let mut trader_executions = 0usize;
    let mut current_price = dec!(0.00012); // 初始价格

    tracing::info!("Full data flow loop started");

    loop {
        tokio::select! {
            // 心跳定时器：每秒更新 Token
            _ = heartbeat_tick.tick() => {
                let token = generate_heartbeat_token().await;

                // 设置到 Trader (DT-002)
                trader.set_heartbeat_token(token.clone());

                tracing::trace!("Heartbeat tick: {}", token);
            }

            // 数据流处理：模拟实时 K 线更新
            _ = tokio::time::sleep(Duration::from_millis(100)) => {
                loop_count += 1;

                // 模拟价格波动
                let price_change = if loop_count % 3 == 0 {
                    dec!(0.0000005)
                } else if loop_count % 5 == 0 {
                    -dec!(0.0000003)
                } else {
                    dec!(0.0000001)
                };
                current_price = (current_price + price_change).max(dec!(0.0001));

                // 更新 Store 中的当前 K 线
                let now = Utc::now();
                let kline = KlineData {
                    kline_start_time: (now - ChronoDuration::seconds(30)).timestamp_millis(),
                    kline_close_time: (now + ChronoDuration::seconds(30)).timestamp_millis(),
                    symbol: SYMBOL.to_string(),
                    interval: "1m".to_string(),
                    open: format!("{:.8}", current_price - dec!(0.0000002)),
                    close: format!("{:.8}", current_price),
                    high: format!("{:.8}", current_price + dec!(0.0000003)),
                    low: format!("{:.8}", current_price - dec!(0.0000005)),
                    volume: format!("{:.2}", 100.0 + (loop_count % 100) as f64),
                    is_closed: false,
                };
                store.write_kline(SYMBOL, kline, false);

                // 驱动 Trader 执行一次 WAL
                match trader.execute_once_wal().await {
                    Ok(result) => {
                        trader_executions += 1;
                        match result {
                            d_checktable::h_15m::ExecutionResult::Executed { qty, order_type } => {
                                tracing::info!(
                                    "[Loop {}] Trader executed: {:?} qty={}",
                                    loop_count,
                                    order_type,
                                    qty
                                );
                            }
                            d_checktable::h_15m::ExecutionResult::Skipped(reason) => {
                                tracing::debug!(
                                    "[Loop {}] Trader skipped: {}",
                                    loop_count,
                                    reason
                                );
                            }
                            d_checktable::h_15m::ExecutionResult::Failed(e) => {
                                tracing::warn!(
                                    "[Loop {}] Trader failed: {}",
                                    loop_count,
                                    e
                                );
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!(
                            "[Loop {}] Trader error: {}",
                            loop_count,
                            e
                        );
                    }
                }

                // 每50次迭代打印状态
                if loop_count % 50 == 0 {
                    let status = trader.current_status();
                    let summary = hb::global().summary().await;
                    tracing::info!(
                        "[Loop {}] Executions: {} | Trader: {:?} | Heartbeat: {}/{} active",
                        loop_count,
                        trader_executions,
                        status,
                        summary.active_count,
                        summary.total_points
                    );
                }

                // 安全退出
                if loop_count >= LOOP_ITERATIONS as u64 {
                    tracing::info!(
                        "Loop limit reached ({}), exiting after {} trader executions",
                        loop_count,
                        trader_executions
                    );
                    break;
                }
            }
        }
    }

    tracing::info!("Full data flow loop exited");

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
