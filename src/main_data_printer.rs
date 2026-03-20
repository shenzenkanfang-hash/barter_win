//! 量化交易系统 - 交易数据打印工具
//!
//! 打印实时交易数据 (Trades)

use clap::Parser;
use futures_util::StreamExt;
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[derive(Parser, Debug)]
#[command(name = "trading-system")]
#[command(about = "量化交易系统 - 交易数据打印")]
enum Args {
    /// 打印实时交易数据
    #[command(name = "trades")]
    Trades {
        /// 交易对符号，如 BTCUSDT
        #[arg(short, long, default_value = "BTCUSDT")]
        symbol: String,

        /// 显示的交易数量 (0 = 持续显示)
        #[arg(short, long, default_value = "20")]
        count: usize,
    },
}

/// Binance Trade 数据格式
#[derive(Debug, Clone)]
struct TradeData {
    trade_id: i64,
    price: String,
    quantity: String,
    time: i64,
    is_buyer_maker: bool,
}

fn parse_timestamp_short(ts: i64) -> String {
    use chrono::TimeZone;
    use chrono::Utc;
    let dt = Utc.timestamp_millis_opt(ts);
    if let Some(dt) = dt.single() {
        dt.format("%H:%M:%S%.3f").to_string()
    } else {
        format!("{}", ts)
    }
}

async fn print_trades(symbol: &str, batch_size: usize) -> Result<(), Box<dyn std::error::Error>> {
    let url = format!(
        "wss://stream.binance.com:9443/ws/{}@trade",
        symbol.to_lowercase()
    );

    println!("========================================");
    println!("  {} 实时交易数据", symbol);
    println!("  WebSocket: {}", url);
    println!("========================================");
    println!("{:>15} | {:>12} | {:>12} | {}", "时间", "价格", "数量", "方向");
    println!("------------------------------------------------");

    let (ws_stream, _) = connect_async(&url).await?;
    let mut reader = ws_stream;

    let mut printed = 0usize;
    let mut batch_count = 0usize;

    while let Some(msg) = reader.next().await {
        let msg = msg?;
        if let Message::Text(text) = msg {
            if let Ok(trade) = serde_json::from_str::<TradeRaw>(&text) {
                let time_str = parse_timestamp_short(trade.trade_time);
                let side = if trade.is_buyer_maker { "SELL" } else { "BUY" };
                let side_indicator = if trade.is_buyer_maker { "<<" } else { ">>";

                println!(
                    "{} {:>12} | {:>12} | {:>12} | {} {}",
                    time_str, trade.price, trade.quantity, side_indicator, side
                );

                printed += 1;
                batch_count += 1;

                if batch_size > 0 && batch_count >= batch_size {
                    println!("------------------------------------------------");
                    println!("已显示 {} 条交易，继续监听...", batch_size);
                    println!("------------------------------------------------");
                    batch_count = 0;
                }
            }
        }
    }

    Ok(())
}

/// Binance Trade 原始数据 (JSON 字段)
#[derive(serde::Deserialize)]
struct TradeRaw {
    #[serde(rename = "t")]
    trade_id: i64,
    #[serde(rename = "p")]
    price: String,
    #[serde(rename = "q")]
    quantity: String,
    #[serde(rename = "T")]
    trade_time: i64,
    #[serde(rename = "m")]
    is_buyer_maker: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    println!("===========================================");
    println!("  量化交易系统 - 交易数据打印");
    println!("===========================================");

    match args {
        Args::Trades { symbol, count } => {
            println!("  模式: 打印实时交易数据");
            println!("  符号: {}", symbol);
            println!("  批次: {}", if count == 0 { "持续显示" } else { "按批次" });
            println!("===========================================\n");
            print_trades(&symbol, count).await?;
        }
    }

    println!("\n  退出程序");
    Ok(())
}
