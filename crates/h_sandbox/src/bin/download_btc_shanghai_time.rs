//! 历史数据下载工具
//!
//! 使用上海时间下载数据
//! 上海时间 2025-10-10 00:00 ~ 2025-10-12 23:59
//! UTC时间 2025-10-09 16:00 ~ 2025-10-12 15:59

use std::path::PathBuf;
use chrono::{DateTime, Utc, TimeZone};
use tokio;
use tracing::info;
use tracing_subscriber::fmt;

use b_data_source::history::HistoryApiClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // 上海时间 2025-10-10 00:00:00 ~ 2025-10-12 23:59:59
    let symbol = "HOTUSDT";
    let start_shanghai = "2025-10-10T00:00:00+08:00";
    let end_shanghai = "2025-10-12T23:59:59+08:00";

    let start_dt: DateTime<Utc> = start_shanghai.parse()?;
    let end_dt: DateTime<Utc> = end_shanghai.parse()?;

    let start_ms = start_dt.timestamp_millis();
    let end_ms = end_dt.timestamp_millis();

    info!("下载 {} {} ~ {} (上海时间)", symbol, start_shanghai, end_shanghai);
    info!("UTC时间: {} ~ {}", start_dt, end_dt);
    info!("时间戳: {} ~ {}", start_ms, end_ms);

    // 创建历史 API 客户端（期货）
    let history_client = HistoryApiClient::new_futures();

    // 下载所有数据（分批，每批1000条）
    let mut all_klines = Vec::new();
    let mut current_start = start_ms;

    while current_start < end_ms {
        let batch_end = (current_start + 1000 * 60 * 1000).min(end_ms);
        info!("下载批次: {} ~ {}", current_start, batch_end);

        let klines = history_client
            .fetch_klines(symbol, "1m", Some(current_start), Some(batch_end), 1000)
            .await?;

        info!("本批获取 {} 条", klines.len());
        all_klines.extend(klines);

        if current_start + 1000 * 60 * 1000 > end_ms {
            break;
        }
        current_start = batch_end;
    }

    info!("总计获取 {} 条 K 线", all_klines.len());

    // 保存 - 用上海时间命名
    let csv_path = PathBuf::from("data/HOTUSDT_1m_20251010_20251012.csv");
    let mut csv_content = String::from("timestamp,open,high,low,close,volume\n");
    for kline in &all_klines {
        csv_content.push_str(&format!(
            "{},{},{},{},{},{}\n",
            kline.timestamp_ms, kline.open, kline.high, kline.low, kline.close, kline.volume
        ));
    }
    std::fs::write(&csv_path, csv_content)?;
    info!("CSV 已保存: {}", csv_path.display());
    info!("共 {} 行数据", all_klines.len());

    Ok(())
}
