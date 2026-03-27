//! 模拟 HOTUSDT 历史数据生成器
//!
//! 由于 HOTUSDT 可能已下架或 API 不可用，生成模拟历史 K 线数据
//! 模拟 2025-10-09 ~ 2025-10-11 的 1m K 线数据

use std::path::PathBuf;
use chrono::{DateTime, Utc, Duration, TimeZone};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tracing::info;
use tracing_subscriber::fmt;

/// 生成模拟的 HOTUSDT 1m K 线数据
fn generate_mock_klines() -> Vec<MockKline> {
    let mut klines = Vec::new();

    // 起始时间: 2025-10-09 00:00:00 UTC
    let mut timestamp = Utc.with_ymd_and_hms(2025, 10, 9, 0, 0, 0).unwrap();

    // 3天 = 72小时 = 4320 分钟
    let total_minutes = 72 * 60; // 4320 根 K 线

    // 模拟价格参数
    let base_price = dec!(0.0001); // 基础价格
    let volatility = dec!(0.000005); // 波动幅度
    let trend = dec!(0.0000001); // 轻微上涨趋势

    let mut current_price = base_price;

    for i in 0..total_minutes {
        // 生成随机波动
        let random_change = {
            use rand::Rng;
            let mut rng = rand::thread_rng();
            let change: f64 = rng.gen_range(-1.0..1.0);
            Decimal::from(change) * volatility
        };

        // 添加趋势
        current_price = current_price + random_change + trend;
        current_price = current_price.max(dec!(0.00001)).min(dec!(0.001)); // 限制价格范围

        // 生成 OHLC
        let open = current_price;
        let change_pct = {
            use rand::Rng;
            let mut rng = rand::thread_rng();
            Decimal::from(rng.gen_range(-0.02..0.02))
        };
        let high = open * (dec!(1) + change_pct);
        let low = open * (dec!(1) - change_pct);
        let close = open * (dec!(1) + Decimal::from(rand::random::<f64>().unwrap() * 0.02 - 0.01));

        // 成交量（随机）
        let volume = {
            use rand::Rng;
            let mut rng = rand::thread_rng();
            Decimal::from(rng.gen_range(1000.0..100000.0))
        };

        klines.push(MockKline {
            timestamp_ms: timestamp.timestamp_millis(),
            open,
            high,
            low,
            close,
            volume,
        });

        timestamp = timestamp + Duration::minutes(1);
    }

    klines
}

#[derive(Debug, Clone, serde::Serialize)]
struct MockKline {
    timestamp_ms: i64,
    open: Decimal,
    high: Decimal,
    low: Decimal,
    close: Decimal,
    volume: Decimal,
}

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("生成 HOTUSDT 模拟历史数据...");

    let klines = generate_mock_klines();
    info!("生成 {} 条 K 线", klines.len());

    // 保存为 CSV
    let csv_path = PathBuf::from("data/HOTUSDT_1m_20251009_20251011.csv");
    let mut csv_content = String::from("timestamp,open,high,low,close,volume\n");
    for kline in &klines {
        csv_content.push_str(&format!(
            "{},{},{},{},{},{}\n",
            kline.timestamp_ms,
            kline.open,
            kline.high,
            kline.low,
            kline.close,
            kline.volume
        ));
    }
    std::fs::write(&csv_path, csv_content).unwrap();
    info!("CSV 已保存: {}", csv_path.display());

    // 保存为 JSON
    let json_path = PathBuf::from("data/HOTUSDT_1m_20251009_20251011.json");
    let json_content = serde_json::to_string_pretty(&klines).unwrap();
    std::fs::write(&json_path, json_content).unwrap();
    info!("JSON 已保存: {}", json_path.display());
}