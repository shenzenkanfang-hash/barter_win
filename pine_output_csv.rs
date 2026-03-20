//! Pine Script v5 指标完整实现 - 1000根K线CSV输出程序
//!
//! 从币安API获取1000根BTCUSDT日K线，输出完整指标CSV

use chrono::{TimeZone, Utc};
use rust_decimal::Decimal;

// 导入 pine_indicator_full 模块
mod pine_indicator_full {
    include!("pine_indicator_full.rs");
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("获取 BTCUSDT 1000 根日K线数据...");

    let url = "https://api.binance.com/api/v3/klines?symbol=BTCUSDT&interval=1d&limit=1000";
    let response = reqwest::blocking::get(url)?;
    let klines: Vec<Vec<serde_json::Value>> = response.json()?;

    println!("成功获取 {} 根K线", klines.len());

    let mut detector = pine_indicator_full::PineColorDetector::new();

    // CSV 输出格式: timestamp,close,macd,signal,hist,ema10,ema20,rsi,crsi,bar_color,bg_color
    println!("timestamp,close,macd,signal,hist,ema10,ema20,rsi,crsi,bar_color,bg_color");

    let mut output = String::new();

    for kline in &klines {
        let open_time: i64 = kline[0].as_i64().unwrap_or(0);
        let close: Decimal = kline[4].as_str().unwrap().parse().unwrap_or_default();

        let (bar_color, bg_color, macd, signal, hist, ema10, ema20, rsi, crsi) =
            detector.update(close);

        let dt = Utc.timestamp_millis_opt(open_time)
            .single()
            .map(|dt| dt.format("%Y-%m-%d").to_string())
            .unwrap_or_default();

        let line = format!(
            "{},{},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{},{}\n",
            dt, close, macd, signal, hist, ema10, ema20, rsi, crsi, bar_color, bg_color
        );
        output.push_str(&line);
    }

    // 输出到 stdout
    print!("{}", output);

    // 同时写入文件
    std::fs::write("pine_indicator_1000.csv", &output)?;
    println!("\n已保存到 pine_indicator_1000.csv");

    Ok(())
}
