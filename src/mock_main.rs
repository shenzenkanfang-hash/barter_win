//! Mock Trading System - 使用 b_data_mock 数据源
//!
//! 这是一个使用模拟数据源的完整交易系统演示
//! 主要用于测试心跳延迟监控系统
//!
//! 运行: cargo run --bin mock_trading

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tokio::time::interval;

use b_data_mock::{
    OrderInterceptor, OrderInterceptorConfig,
    TickInterceptor, MockApiGateway,
    KlineStreamGenerator, KLine,
    Period, ReplaySource,
};
use a_common::heartbeat as hb;
use futures_util::{stream, StreamExt};

// ============================================================================
// 常量配置
// ============================================================================

const INITIAL_BALANCE: Decimal = dec!(10000);
const PROCESS_DELAY_MS: u64 = 5; // 模拟处理延迟
const SYMBOL: &str = "HOTUSDT";   // 测试品种
const CSV_PATH: &str = "D:/RusProject/barter-rs-main/data/HOTUSDT_1m_20251009_20251011.csv"; // 真实历史数据

// ============================================================================
// 测试点ID
// ============================================================================

const BS_001: &str = "BS-001"; // Kline1mStream
const CP_001: &str = "CP-001"; // SignalProcessor
const DT_001: &str = "DT-001"; // CheckTable
const ER_001: &str = "ER-001"; // RiskPreChecker
const FE_001: &str = "FE-001"; // EventEngine

// ============================================================================
// 模拟指标计算
// ============================================================================

fn simulate_indicator_calc() {
    std::thread::sleep(Duration::from_millis(PROCESS_DELAY_MS));
}

// ============================================================================
// 模拟策略决策
// ============================================================================

fn simulate_strategy_decide() {
    std::thread::sleep(Duration::from_millis(PROCESS_DELAY_MS));
}

// ============================================================================
// 模拟风控检查
// ============================================================================

fn simulate_risk_check() {
    std::thread::sleep(Duration::from_millis(PROCESS_DELAY_MS));
}

// ============================================================================
// 模拟订单执行
// ============================================================================

fn simulate_order_execution() {
    std::thread::sleep(Duration::from_millis(PROCESS_DELAY_MS));
}

// ============================================================================
// 主程序
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("========================================");
    println!("  MOCK TRADING SYSTEM WITH HEARTBEAT");
    println!("  心跳延迟监控系统测试");
    println!("========================================");
    println!();

    // 2. 初始化心跳报告器
    tracing::info!("Initializing Heartbeat Reporter...");
    hb::init(hb::Config::default());
    tracing::info!("Heartbeat Reporter initialized");
    println!();

    // 3. 初始化组件
    println!("[1] Initializing Components...");
    println!();

    // 3.1 Tick 拦截器
    let tick_interceptor = TickInterceptor::new();
    println!("  - TickInterceptor: OK");

    // 3.2 订单拦截器
    let gateway = MockApiGateway::with_default_config(INITIAL_BALANCE);
    let order_config = OrderInterceptorConfig {
        enable_heartbeat: true,
        latency_warning_ms: 100,
        latency_critical_ms: 500,
    };
    let order_interceptor = OrderInterceptor::new(gateway, order_config);
    println!("  - OrderInterceptor: OK");

    println!();
    println!("========================================");
    println!("  STARTING MOCK DATA STREAM");
    println!("========================================");
    println!();

    // 4. 从 CSV 加载真实 K 线数据 (HOTUSDT 2025-10-09 ~ 2025-10-11 UTC)
    let klines = load_klines_from_csv(CSV_PATH, SYMBOL)
        .expect("Failed to load K-line data from CSV");

    // 5. 创建 K 线流生成器 (每根 1m KLine → 60 个子 K线，模拟 WS 流)
    let mut kline_stream = KlineStreamGenerator::new(
        SYMBOL.to_string(),
        Box::new(klines.into_iter()),
    );

    println!("[2] Mock KLine Stream Created (HOTUSDT 2025-10-09 ~ 2025-10-11)");
    println!();

    // 6. 主循环 - 处理模拟数据流
    let mut tick_count = 0u64;
    let mut report_interval = interval(Duration::from_secs(10));
    let mut last_report_time = Utc::now();

    println!("[3] Starting Data Processing Loop...");
    println!();

    // 预加载所有 K 线数据（避免 ThreadRng Send 问题）
    let all_klines: Vec<_> = kline_stream.by_ref().collect();
    let start_time = Utc::now();

    // 将 Vec 转换为异步流（使用 futures_util::StreamExt）
    let kline_stream = stream::iter(all_klines);
    let mut kline_stream = Box::pin(kline_stream.fuse());
    let mut stream_exhausted = false;

    loop {
        tokio::select! {
            // 处理下一条 K 线
            kline_opt = kline_stream.next() => {
                match kline_opt {
                    Some(sub_kline) => {
                        tick_count += 1;

                        // 记录数据产生时间戳
                        let data_timestamp = sub_kline.timestamp;

                        // ===== 心跳报到序列 =====

                        // 1. BS-001: Kline1mStream 报到
                        let token1 = hb::Token::with_data_timestamp(tick_count, data_timestamp);
                        let latency1 = token1.data_latency_ms().unwrap_or(0);
                        hb::global().report_with_latency(
                            &token1, BS_001, "b_data_mock",
                            "kline_1m_stream", "mock_main.rs", latency1
                        ).await;

                        // 模拟 Kline 合成处理
                        simulate_indicator_calc();

                        // 2. CP-001: SignalProcessor 报到
                        let token2 = hb::Token::with_data_timestamp(tick_count + 1, data_timestamp);
                        let latency2 = token2.data_latency_ms().unwrap_or(0);
                        hb::global().report_with_latency(
                            &token2, CP_001, "c_data_process",
                            "calc_indicators", "mock_main.rs", latency2
                        ).await;

                        // 3. DT-001: CheckTable 报到
                        let token3 = hb::Token::with_data_timestamp(tick_count + 2, data_timestamp);
                        let latency3 = token3.data_latency_ms().unwrap_or(0);
                        hb::global().report_with_latency(
                            &token3, DT_001, "d_checktable",
                            "check_signals", "mock_main.rs", latency3
                        ).await;

                        // 4. ER-001: RiskPreChecker 报到
                        let token4 = hb::Token::with_data_timestamp(tick_count + 3, data_timestamp);
                        let latency4 = token4.data_latency_ms().unwrap_or(0);
                        hb::global().report_with_latency(
                            &token4, ER_001, "e_risk_monitor",
                            "pre_check", "mock_main.rs", latency4
                        ).await;

                        // 5. 模拟订单执行
                        simulate_order_execution();

                        // 6. FE-001: EventEngine 报到
                        let token5 = hb::Token::with_data_timestamp(tick_count + 4, data_timestamp);
                        let latency5 = token5.data_latency_ms().unwrap_or(0);
                        hb::global().report_with_latency(
                            &token5, FE_001, "f_engine",
                            "place_order", "mock_main.rs", latency5
                        ).await;

                        // 打印进度（每100条）
                        if tick_count % 100 == 0 {
                            let elapsed = (Utc::now() - start_time).num_seconds();
                            println!(
                                "  [Tick #{:>6}] Price: {:>10} | Latencies: BS={}ms, CP={}ms, DT={}ms, ER={}ms, FE={}ms | Elapsed: {}s",
                                tick_count,
                                sub_kline.price,
                                latency1, latency2, latency3, latency4, latency5,
                                elapsed
                            );
                        }
                    }
                    None => {
                        if !stream_exhausted {
                            println!("\n[KLine Stream Exhausted]");
                            stream_exhausted = true;
                        }
                        // 流已结束，等待定时器或退出
                    }
                }
            }

            // 定期报告
            _ = report_interval.tick() => {
                // 生成心跳报告
                let report = hb::global().generate_report().await;
                last_report_time = Utc::now();

                println!();
                println!("========================================");
                println!("  HEARTBEAT REPORT (Every 10s)");
                println!("========================================");
                println!("  Report Time: {}", Utc::now());
                println!("  Total Ticks: {}", tick_count);
                println!("  Heartbeat Sequence: {}", report.heartbeat_sequence);
                println!();
                println!("  [Latency Summary]");
                println!("  {:<10} {:<20} {:>10} {:>10} {:>10}",
                    "Point", "Name", "Last(ms)", "Avg(ms)", "Max(ms)");

                for detail in &report.points_detail {
                    println!(
                        "  {:<10} {:<20} {:>10} {:>10} {:>10}",
                        detail.point_id,
                        detail.point_name,
                        detail.last_latency_ms.unwrap_or(0),
                        detail.avg_latency_ms.unwrap_or(0),
                        detail.max_latency_ms.unwrap_or(0)
                    );
                }

                println!();
                println!("  [Status]");
                println!("  Active Points: {}", report.active_points);
                println!("  Stale Points: {}", report.stale_points_count);
                println!("  Total Reports: {}", report.total_reports);

                // 检查失联点
                if !report.stale_points.is_empty() {
                    println!();
                    println!("  [WARNING] Stale Points Detected!");
                    for stale in &report.stale_points {
                        println!("    - {} (stale since seq {})", stale.point_id, stale.since_sequence);
                    }
                }

                println!();
                println!("========================================");
                println!();

                // 获取订单统计
                let order_stats = order_interceptor.get_stats();
                if order_stats.total_orders > 0 {
                    println!("  [Order Statistics]");
                    println!("    Total Orders: {}", order_stats.total_orders);
                    println!("    Successful: {}", order_stats.successful_orders);
                    println!("    Failed: {}", order_stats.failed_orders);
                    println!("    Avg Latency: {}ms", order_stats.avg_latency_ms);
                    println!("    Max Latency: {}ms", order_stats.max_latency_ms);
                    println!();
                }
            }
        }

        // 如果流已结束且已报告，跳出循环
        if stream_exhausted {
            break;
        }
    }

    // 7. 最终报告
    println!();
    println!("========================================");
    println!("  FINAL HEARTBEAT REPORT");
    println!("========================================");

    let report = hb::global().generate_report().await;

    println!("  Total Ticks Processed: {}", tick_count);
    println!("  Heartbeat Sequence: {}", report.heartbeat_sequence);
    println!();

    println!("  [Component Latency Details]");
    println!("  {:<10} {:<20} {:>10} {:>10} {:>10} {:>10}",
        "Point", "Name", "Count", "Last(ms)", "Avg(ms)", "Max(ms)");

    for detail in &report.points_detail {
        println!(
            "  {:<10} {:<20} {:>10} {:>10} {:>10} {:>10}",
            detail.point_id,
            detail.point_name,
            detail.reports_count,
            detail.last_latency_ms.unwrap_or(0),
            detail.avg_latency_ms.unwrap_or(0),
            detail.max_latency_ms.unwrap_or(0)
        );
    }

    // 8. 保存报告
    let report_path = "heartbeat_report.json";
    if let Err(e) = hb::global().save_report(report_path).await {
        tracing::error!("Failed to save report: {:?}", e);
    } else {
        println!();
        println!("  [Report saved to: {}]", report_path);
    }

    println!();
    println!("========================================");
    println!("  SYSTEM STOPPED");
    println!("========================================");

    Ok(())
}

// ============================================================================
// 辅助函数：从 CSV 文件加载真实 K 线数据
// ============================================================================

fn load_klines_from_csv(path: &str, symbol: &str) -> Result<Vec<KLine>, Box<dyn std::error::Error>> {
    use std::io::BufRead;
    use rust_decimal::Decimal;
    use chrono::DateTime;

    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    let mut lines = reader.lines();

    let mut klines = Vec::new();

    // 跳过表头
    if let Some(header) = lines.next() {
        tracing::info!("CSV header: {}", header?);
    }

    for line in lines {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() < 6 {
            continue;
        }

        // 解析时间戳 (毫秒)
        let ts_ms: i64 = parts[0].trim().parse()?;
        let timestamp = DateTime::from_timestamp(ts_ms / 1000, ((ts_ms % 1000) as u32) * 1_000_000)
            .ok_or_else(|| format!("Invalid timestamp: {}", ts_ms))?;

        // 解析 OHLCV
        let open: Decimal = parts[1].trim().parse()?;
        let high: Decimal = parts[2].trim().parse()?;
        let low: Decimal = parts[3].trim().parse()?;
        let close: Decimal = parts[4].trim().parse()?;
        let volume: Decimal = parts[5].trim().parse()?;

        klines.push(KLine {
            symbol: symbol.to_string(),
            period: Period::Minute(1),
            open,
            high,
            low,
            close,
            volume,
            timestamp,
            is_closed: true,
        });
    }

    tracing::info!("Loaded {} K-lines from {}", klines.len(), path);
    Ok(klines)
}
