//! 量化交易系统 - 数据打印工具
//!
//! 通过命令行参数控制打印不同的实时数据

use clap::Parser;
use futures_util::StreamExt;
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[derive(Parser, Debug)]
#[command(name = "trading-system")]
#[command(about = "量化交易系统 - 数据打印工具")]
enum Args {
    /// 打印分钟级 K 线数据 (1m)
    #[command(name = "kline-1m")]
    Kline1m {
        /// 交易对符号，如 BTCUSDT
        #[arg(short, long, default_value = "BTCUSDT")]
        symbol: String,
    },

    /// 打印日线级 K 线数据 (1d)
    #[command(name = "kline-1d")]
    Kline1d {
        #[arg(short, long, default_value = "BTCUSDT")]
        symbol: String,
    },

    /// 打印订单簿深度数据
    #[command(name = "depth")]
    Depth {
        #[arg(short, long, default_value = "BTCUSDT")]
        symbol: String,
    },

    /// 打印所有数据 (K线 + 深度)
    #[command(name = "all")]
    All {
        #[arg(short, long, default_value = "BTCUSDT")]
        symbol: String,
    },

    /// 打印分钟 + 日线 K 线
    #[command(name = "kline")]
    Kline {
        #[arg(short, long, default_value = "BTCUSDT")]
        symbol: String,
    },
}

// Binance WebSocket K线数据格式
#[derive(Debug, Deserialize)]
struct BinanceKline {
    #[serde(rename = "t")]
    open_time: u64,
    #[serde(rename = "T")]
    close_time: u64,
    #[serde(rename = "s")]
    symbol: String,
    #[serde(rename = "i")]
    interval: String,
    #[serde(rename = "o")]
    open: String,
    #[serde(rename = "c")]
    close: String,
    #[serde(rename = "h")]
    high: String,
    #[serde(rename = "l")]
    low: String,
    #[serde(rename = "v")]
    volume: String,
    #[serde(rename = "x")]
    is_closed: bool,
}

#[derive(Debug, Deserialize)]
struct BinanceDepth {
    #[serde(rename = "lastUpdateId")]
    last_update_id: u64,
    #[serde(rename = "bids")]
    bids: Vec<(String, String)>,
    #[serde(rename = "asks")]
    asks: Vec<(String, String)>,
}

fn parse_timestamp(ts: u64) -> String {
    use chrono::TimeZone;
    use chrono::Utc;
    let dt = Utc.timestamp_millis_opt(ts as i64);
    if let Some(dt) = dt.single() {
        dt.format("%Y-%m-%d %H:%M:%S").to_string()
    } else {
        format!("{}", ts)
    }
}

fn parse_timestamp_short(ts: u64) -> String {
    use chrono::TimeZone;
    use chrono::Utc;
    let dt = Utc.timestamp_millis_opt(ts as i64);
    if let Some(dt) = dt.single() {
        dt.format("%H:%M:%S").to_string()
    } else {
        format!("{}", ts)
    }
}

async fn print_kline_1m(symbol: &str) -> Result<(), Box<dyn std::error::Error>> {
    let url = format!(
        "wss://stream.binance.com:9443/ws/{}@kline_1m",
        symbol.to_lowercase()
    );

    println!("\n========================================");
    println!("  {} 1分钟K线数据", symbol);
    println!("  WebSocket: {}", url);
    println!("========================================\n");
    println!("+--------+----------+----------+----------+----------+----------+");
    println!("|  时间  |   开盘   |   最高   |   最低   |   收盘   |   成交量  |");
    println!("+--------+----------+----------+----------+----------+----------+");

    let (ws_stream, _) = connect_async(&url).await?;
    let mut reader = ws_stream;

    while let Some(msg) = reader.next().await {
        let msg = msg?;
        if let Message::Text(text) = msg {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(data) = json.get("k").and_then(|k| k.as_object()) {
                    let kline = BinanceKline {
                        open_time: data.get("t").and_then(|v| v.as_u64()).unwrap_or(0),
                        close_time: data.get("T").and_then(|v| v.as_u64()).unwrap_or(0),
                        symbol: data.get("s").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        interval: data.get("i").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        open: data.get("o").and_then(|v| v.as_str()).unwrap_or("0").to_string(),
                        close: data.get("c").and_then(|v| v.as_str()).unwrap_or("0").to_string(),
                        high: data.get("h").and_then(|v| v.as_str()).unwrap_or("0").to_string(),
                        low: data.get("l").and_then(|v| v.as_str()).unwrap_or("0").to_string(),
                        volume: data.get("v").and_then(|v| v.as_str()).unwrap_or("0").to_string(),
                        is_closed: data.get("x").and_then(|v| v.as_bool()).unwrap_or(false),
                    };

                    let time_str = parse_timestamp_short(kline.open_time);
                    let is_closed_str = if kline.is_closed { "*" } else { " " };

                    println!(
                        "|{}{}|{}{:>8}|{:>8}|{:>8}|{:>8}|{:>8}|",
                        is_closed_str,
                        time_str,
                        "",
                        kline.open,
                        kline.high,
                        kline.low,
                        kline.close,
                        kline.volume
                    );
                    println!("+--------+----------+----------+----------+----------+----------+");
                }
            }
        }
    }
    Ok(())
}

async fn print_kline_1d(symbol: &str) -> Result<(), Box<dyn std::error::Error>> {
    let url = format!(
        "wss://stream.binance.com:9443/ws/{}@kline_1d",
        symbol.to_lowercase()
    );

    println!("\n========================================");
    println!("  {} 日线K线数据", symbol);
    println!("  WebSocket: {}", url);
    println!("========================================\n");
    println!("+----------+----------+----------+----------+----------+----------+");
    println!("|   日期   |   开盘   |   最高   |   最低   |   收盘   |   成交量  |");
    println!("+----------+----------+----------+----------+----------+----------+");

    let (ws_stream, _) = connect_async(&url).await?;
    let mut reader = ws_stream;

    while let Some(msg) = reader.next().await {
        let msg = msg?;
        if let Message::Text(text) = msg {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(data) = json.get("k").and_then(|k| k.as_object()) {
                    let kline = BinanceKline {
                        open_time: data.get("t").and_then(|v| v.as_u64()).unwrap_or(0),
                        close_time: data.get("T").and_then(|v| v.as_u64()).unwrap_or(0),
                        symbol: data.get("s").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        interval: data.get("i").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        open: data.get("o").and_then(|v| v.as_str()).unwrap_or("0").to_string(),
                        close: data.get("c").and_then(|v| v.as_str()).unwrap_or("0").to_string(),
                        high: data.get("h").and_then(|v| v.as_str()).unwrap_or("0").to_string(),
                        low: data.get("l").and_then(|v| v.as_str()).unwrap_or("0").to_string(),
                        volume: data.get("v").and_then(|v| v.as_str()).unwrap_or("0").to_string(),
                        is_closed: data.get("x").and_then(|v| v.as_bool()).unwrap_or(false),
                    };

                    let time_str = parse_timestamp(kline.open_time);
                    let date_part = &time_str[..10]; // YYYY-MM-DD
                    let is_closed_str = if kline.is_closed { "*" } else { " " };

                    println!(
                        "|{}{}|{}{:>8}|{:>8}|{:>8}|{:>8}|{:>8}|",
                        is_closed_str,
                        date_part,
                        "",
                        kline.open,
                        kline.high,
                        kline.low,
                        kline.close,
                        kline.volume
                    );
                    println!("+----------+----------+----------+----------+----------+----------+");
                }
            }
        }
    }
    Ok(())
}

async fn print_depth(symbol: &str) -> Result<(), Box<dyn std::error::Error>> {
    let url = format!(
        "wss://stream.binance.com:9443/ws/{}@depth20@100ms",
        symbol.to_lowercase()
    );

    println!("\n========================================");
    println!("  {} 订单簿深度数据", symbol);
    println!("  WebSocket: {}", url);
    println!("========================================\n");
    println!("  BID (买方)                              ASK (卖方)");
    println!("  价格        数量                        价格        数量");
    println!("  --------    ------                        --------    ------");

    let (ws_stream, _) = connect_async(&url).await?;
    let mut reader = ws_stream;

    while let Some(msg) = reader.next().await {
        let msg = msg?;
        if let Message::Text(text) = msg {
            if let Ok(depth) = serde_json::from_str::<BinanceDepth>(&text) {
                // 清屏重新打印
                print!("\r                                                                                \r");
                println!("  BID (买方)                              ASK (卖方)");
                println!("  价格        数量                        价格        数量");
                println!("  --------    ------                        --------    ------");

                // 打印前5档
                for i in 0..5.min(depth.bids.len()).min(depth.asks.len()) {
                    println!(
                        "  {:>8}    {:>6}                        {:>8}    {:>6}",
                        depth.bids[i].0, depth.bids[i].1, depth.asks[i].0, depth.asks[i].1
                    );
                }
            }
        }
    }
    Ok(())
}

async fn print_kline_both(symbol: &str) -> Result<(), Box<dyn std::error::Error>> {
    let url_1m = format!(
        "wss://stream.binance.com:9443/ws/{}@kline_1m",
        symbol.to_lowercase()
    );
    let url_1d = format!(
        "wss://stream.binance.com:9443/ws/{}@kline_1d",
        symbol.to_lowercase()
    );

    println!("\n========================================");
    println!("  {} K线数据 (1m + 1d)", symbol);
    println!("========================================\n");

    let (ws_stream_1m, _) = connect_async(&url_1m).await?;
    let (ws_stream_1d, _) = connect_async(&url_1d).await?;

    let mut reader_1m = ws_stream_1m;
    let mut reader_1d = ws_stream_1d;

    let (tx, mut rx) = broadcast::channel(100);

    // 1m reader task
    let tx_clone = tx.clone();
    let symbol_clone = symbol.to_string();
    tokio::spawn(async move {
        let mut reader = reader_1m;
        println!("[1m] 开始接收 1分钟K线数据...");
        while let Some(msg) = reader.next().await {
            if let Ok(Message::Text(text)) = msg {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                    if let Some(data) = json.get("k").and_then(|k| k.as_object()) {
                        let kline = BinanceKline {
                            open_time: data.get("t").and_then(|v| v.as_u64()).unwrap_or(0),
                            close_time: data.get("T").and_then(|v| v.as_u64()).unwrap_or(0),
                            symbol: data.get("s").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            interval: data.get("i").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            open: data.get("o").and_then(|v| v.as_str()).unwrap_or("0").to_string(),
                            close: data.get("c").and_then(|v| v.as_str()).unwrap_or("0").to_string(),
                            high: data.get("h").and_then(|v| v.as_str()).unwrap_or("0").to_string(),
                            low: data.get("l").and_then(|v| v.as_str()).unwrap_or("0").to_string(),
                            volume: data.get("v").and_then(|v| v.as_str()).unwrap_or("0").to_string(),
                            is_closed: data.get("x").and_then(|v| v.as_bool()).unwrap_or(false),
                        };
                        let _ = tx_clone.send((1, kline));
                    }
                }
            }
        }
    });

    // 1d reader task
    let tx_clone2 = tx.clone();
    tokio::spawn(async move {
        let mut reader = reader_1d;
        println!("[1d] 开始接收 日线K线数据...");
        while let Some(msg) = reader.next().await {
            if let Ok(Message::Text(text)) = msg {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                    if let Some(data) = json.get("k").and_then(|k| k.as_object()) {
                        let kline = BinanceKline {
                            open_time: data.get("t").and_then(|v| v.as_u64()).unwrap_or(0),
                            close_time: data.get("T").and_then(|v| v.as_u64()).unwrap_or(0),
                            symbol: data.get("s").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            interval: data.get("i").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            open: data.get("o").and_then(|v| v.as_str()).unwrap_or("0").to_string(),
                            close: data.get("c").and_then(|v| v.as_str()).unwrap_or("0").to_string(),
                            high: data.get("h").and_then(|v| v.as_str()).unwrap_or("0").to_string(),
                            low: data.get("l").and_then(|v| v.as_str()).unwrap_or("0").to_string(),
                            volume: data.get("v").and_then(|v| v.as_str()).unwrap_or("0").to_string(),
                            is_closed: data.get("x").and_then(|v| v.as_bool()).unwrap_or(false),
                        };
                        let _ = tx_clone2.send((2, kline));
                    }
                }
            }
        }
    });

    // Print loop
    let mut last_1m: Option<(String, BinanceKline)> = None;
    let mut last_1d: Option<(String, BinanceKline)> = None;

    while let Some((source, kline)) = rx.recv().await {
        match source {
            1 => {
                let time_str = parse_timestamp_short(kline.open_time);
                last_1m = Some((time_str, kline));
            }
            2 => {
                let time_str = parse_timestamp(kline.open_time);
                let date_part = &time_str[..10];
                last_1d = Some((date_part.to_string(), kline));
            }
            _ => {}
        }

        // 打印当前状态
        print!("\r                                                                    \r");
        if let Some((time, kline)) = &last_1m {
            println!(
                "[1m] {} | O:{} H:{} L:{} C:{} V:{}",
                time, kline.open, kline.high, kline.low, kline.close, kline.volume
            );
        }
        if let Some((date, kline)) = &last_1d {
            println!(
                "[1d] {} | O:{} H:{} L:{} C:{} V:{}",
                date, kline.open, kline.high, kline.low, kline.close, kline.volume
            );
        }
    }

    Ok(())
}

async fn print_all(symbol: &str) -> Result<(), Box<dyn std::error::Error>> {
    let url_1m = format!(
        "wss://stream.binance.com:9443/ws/{}@kline_1m",
        symbol.to_lowercase()
    );
    let url_1d = format!(
        "wss://stream.binance.com:9443/ws/{}@kline_1d",
        symbol.to_lowercase()
    );
    let url_depth = format!(
        "wss://stream.binance.com:9443/ws/{}@depth20@100ms",
        symbol.to_lowercase()
    );

    println!("\n========================================");
    println!("  {} 所有数据 (K线 1m/1d + Depth)", symbol);
    println!("========================================\n");

    let (ws_stream_1m, _) = connect_async(&url_1m).await?;
    let (ws_stream_1d, _) = connect_async(&url_1d).await?;
    let (ws_stream_depth, _) = connect_async(&url_depth).await?;

    let mut reader_1m = ws_stream_1m;
    let mut reader_1d = ws_stream_1d;
    let mut reader_depth = ws_stream_depth;

    let (tx, mut rx) = broadcast::channel(200);

    // 1m task
    let tx_clone = tx.clone();
    tokio::spawn(async move {
        let mut reader = reader_1m;
        while let Some(msg) = reader.next().await {
            if let Ok(Message::Text(text)) = msg {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                    if let Some(data) = json.get("k").and_then(|k| k.as_object()) {
                        let kline = BinanceKline {
                            open_time: data.get("t").and_then(|v| v.as_u64()).unwrap_or(0),
                            close_time: data.get("T").and_then(|v| v.as_u64()).unwrap_or(0),
                            symbol: data.get("s").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            interval: data.get("i").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            open: data.get("o").and_then(|v| v.as_str()).unwrap_or("0").to_string(),
                            close: data.get("c").and_then(|v| v.as_str()).unwrap_or("0").to_string(),
                            high: data.get("h").and_then(|v| v.as_str()).unwrap_or("0").to_string(),
                            low: data.get("l").and_then(|v| v.as_str()).unwrap_or("0").to_string(),
                            volume: data.get("v").and_then(|v| v.as_str()).unwrap_or("0").to_string(),
                            is_closed: data.get("x").and_then(|v| v.as_bool()).unwrap_or(false),
                        };
                        let _ = tx_clone.send((1, serde_json::to_string(&kline).unwrap_or_default()));
                    }
                }
            }
        }
    });

    // 1d task
    let tx_clone2 = tx.clone();
    tokio::spawn(async move {
        let mut reader = reader_1d;
        while let Some(msg) = reader.next().await {
            if let Ok(Message::Text(text)) = msg {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                    if let Some(data) = json.get("k").and_then(|k| k.as_object()) {
                        let kline = BinanceKline {
                            open_time: data.get("t").and_then(|v| v.as_u64()).unwrap_or(0),
                            close_time: data.get("T").and_then(|v| v.as_u64()).unwrap_or(0),
                            symbol: data.get("s").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            interval: data.get("i").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            open: data.get("o").and_then(|v| v.as_str()).unwrap_or("0").to_string(),
                            close: data.get("c").and_then(|v| v.as_str()).unwrap_or("0").to_string(),
                            high: data.get("h").and_then(|v| v.as_str()).unwrap_or("0").to_string(),
                            low: data.get("l").and_then(|v| v.as_str()).unwrap_or("0").to_string(),
                            volume: data.get("v").and_then(|v| v.as_str()).unwrap_or("0").to_string(),
                            is_closed: data.get("x").and_then(|v| v.as_bool()).unwrap_or(false),
                        };
                        let _ = tx_clone2.send((2, serde_json::to_string(&kline).unwrap_or_default()));
                    }
                }
            }
        }
    });

    // Depth task
    let tx_clone3 = tx.clone();
    tokio::spawn(async move {
        let mut reader = reader_depth;
        while let Some(msg) = reader.next().await {
            if let Ok(Message::Text(text)) = msg {
                if let Ok(depth) = serde_json::from_str::<BinanceDepth>(&text) {
                    let _ = tx_clone3.send((3, serde_json::to_string(&depth).unwrap_or_default()));
                }
            }
        }
    });

    // Print loop
    let mut last_1m = String::new();
    let mut last_1d = String::new();
    let mut last_depth = String::new();

    while let Some((source, data)) = rx.recv().await {
        match source {
            1 => last_1m = data,
            2 => last_1d = data,
            3 => last_depth = data,
            _ => {}
        }

        print!("\r");
        print!("[1m] {}  ", last_1m);
        print!("[1d] {}  ", last_1d);
        print!("[Depth] {}", last_depth);
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    println!("===========================================");
    println!("  量化交易系统 - 数据打印工具");
    println!("===========================================");

    match args {
        Args::Kline1m { symbol } => {
            println!("  模式: 打印 1分钟 K线数据");
            println!("  符号: {}", symbol);
            println!("===========================================\n");
            print_kline_1m(&symbol).await?;
        }
        Args::Kline1d { symbol } => {
            println!("  模式: 打印 日线 K线数据");
            println!("  符号: {}", symbol);
            println!("===========================================\n");
            print_kline_1d(&symbol).await?;
        }
        Args::Depth { symbol } => {
            println!("  模式: 打印 订单簿深度");
            println!("  符号: {}", symbol);
            println!("===========================================\n");
            print_depth(&symbol).await?;
        }
        Args::All { symbol } => {
            println!("  模式: 打印 所有数据 (K线 + 深度)");
            println!("  符号: {}", symbol);
            println!("===========================================\n");
            print_all(&symbol).await?;
        }
        Args::Kline { symbol } => {
            println!("  模式: 打印 K线数据 (1m + 1d)");
            println!("  符号: {}", symbol);
            println!("===========================================\n");
            print_kline_both(&symbol).await?;
        }
    }

    println!("\n  退出程序");
    Ok(())
}
