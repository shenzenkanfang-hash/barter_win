//! 用 Python 的 close 数据测试 Rust Pine 指标输出

use chrono::{TimeZone, Utc};
use rust_decimal::Decimal;

// 导入 pine_indicator_full 模块
mod pine_indicator_full {
    include!("pine_indicator_full.rs");
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 读取 Python 的 CSV 数据（只取 close 列）
    let python_csv = std::fs::read_to_string("python_close_data.csv")?;

    let mut detector = pine_indicator_full::PineColorDetector::new();

    println!("timestamp,close,macd,signal,hist,ema10,ema20,rsi,crsi,bar_color,bg_color,top3_avg_amplitude_pct,one_percent_amplitude_time_days");

    for line in python_csv.lines().skip(1) {  // 跳过标题行
        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() >= 2 {
            let date = parts[0];
            let close: Decimal = parts[1].parse().unwrap_or_default();

            let (bar_color, bg_color, macd, signal, hist, ema10, ema20, rsi, crsi) =
                detector.update((close, close, close, close));

            let top3_avg = detector.calc_top3_avg_amplitude_pct();
            let one_pct_days = detector.calc_one_percent_amplitude_time_days();

            println!("{},{},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{},{},{:.2},{:.2}",
                date, close, macd, signal, hist, ema10, ema20, rsi, crsi, bar_color, bg_color, top3_avg, one_pct_days);
        }
    }

    Ok(())
}
