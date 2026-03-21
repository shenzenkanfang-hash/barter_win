//! 量化交易系统 - 实时监控程序
//!
//! 功能：
//! - 实时交易数据（简洁输出）
//! - 高波动信号检测和记录
//! - 每分钟记录 1m/15m 状态
//! - 关键日志输出到文件和控制台

use clap::Parser;
use chrono::{DateTime, Local, TimeZone, Timelike, Utc};
use futures_util::StreamExt;
use market::{VolatilityDetector, VolatilityStats};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::Serialize;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{info, warn, Level};
use tracing_subscriber::fmt::format::FmtSpan;

/// 命令行参数
#[derive(Parser, Debug)]
#[command(name = "trading-system")]
#[command(about = "量化交易系统 - 实时监控")]
struct Args {
    /// 交易对符号列表，逗号分隔，如 BTCUSDT,ETHUSDT,SOLUSDT
    #[arg(short, long, default_value = "BTCUSDT", value_delimiter = ',')]
    symbols: Vec<String>,

    /// 日志输出目录
    #[arg(short, long, default_value = "logs")]
    log_dir: String,

    /// 是否启用详细交易输出
    #[arg(short, long, default_value = "false")]
    verbose_trades: bool,
}

// ============================================================================
// 数据结构
// ============================================================================

/// 交易记录（简洁格式）
#[derive(Debug, Clone, Serialize)]
struct TradeRecord {
    time: String,
    price: String,
    qty: String,
    side: String,
}

/// 高波动信号记录
#[derive(Debug, Clone, Serialize)]
struct VolatilitySignal {
    timestamp: String,
    symbol: String,
    signal_type: String,      // "ENTER_HIGH_VOL" | "EXIT_HIGH_VOL"
    trigger_period: String,   // "1M" | "15M" | "BOTH" - 触发高波动的周期
    vol_1m: String,
    vol_15m: String,
    price: String,
    threshold_1m: String,    // 3%
    threshold_15m: String,    // 6%
}

/// 状态快照记录（每分钟）
#[derive(Debug, Clone, Serialize)]
struct StatusSnapshot {
    timestamp: String,
    symbol: String,
    price: String,
    vol_1m: String,
    vol_15m: String,
    is_high_vol: bool,
    trade_count_1m: u32,
    kline_1m_close: String,
    kline_15m_close: String,
}

// ============================================================================
// 文件写入
// ============================================================================

struct FileLogger {
    signal_path: PathBuf,
    status_path: PathBuf,
    signal_file: Mutex<Option<File>>,
    status_file: Mutex<Option<File>>,
}

impl FileLogger {
    fn new(log_dir: &str) -> std::io::Result<Self> {
        let signal_path = PathBuf::from(log_dir).join("volatility_signals.jsonl");
        let status_path = PathBuf::from(log_dir).join("status_snapshots.jsonl");

        // 确保目录存在
        std::fs::create_dir_all(log_dir)?;

        Ok(Self {
            signal_path,
            status_path,
            signal_file: Mutex::new(None),
            status_file: Mutex::new(None),
        })
    }

    fn write_signal(&self, signal: &VolatilitySignal) -> std::io::Result<()> {
        let mut file_guard = self.signal_file.lock().unwrap();
        if file_guard.is_none() {
            let f = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.signal_path)?;
            *file_guard = Some(f);
        }
        if let Some(ref mut f) = *file_guard {
            let line = serde_json::to_string(signal).unwrap();
            writeln!(f, "{}", line)?;
            f.flush()?;
        }
        Ok(())
    }

    fn write_status(&self, status: &StatusSnapshot) -> std::io::Result<()> {
        let mut file_guard = self.status_file.lock().unwrap();
        if file_guard.is_none() {
            let f = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.status_path)?;
            *file_guard = Some(f);
        }
        if let Some(ref mut f) = *file_guard {
            let line = serde_json::to_string(status).unwrap();
            writeln!(f, "{}", line)?;
            f.flush()?;
        }
        Ok(())
    }
}

// ============================================================================
// 时间工具
// ============================================================================

fn format_time(ts: i64) -> String {
    if let Some(dt) = Utc.timestamp_millis_opt(ts).single() {
        dt.format("%H:%M:%S%.3f").to_string()
    } else {
        ts.to_string()
    }
}

fn get_local_minute() -> (u32, u32, u32) {
    let now = Local::now();
    (now.hour(), now.minute(), now.second())
}

// ============================================================================
// WebSocket 交易处理
// ============================================================================

struct TradeHandler {
    symbol: String,
    detector: VolatilityDetector,
    file_logger: FileLogger,
    verbose_trades: bool,
    // 统计
    trade_count: u32,
    last_minute: Option<(u32, u32)>,
    last_vol_state: bool,
    // 上次打印状态的分钟
    last_status_minute: Option<(u32, u32, u32)>,
}

impl TradeHandler {
    fn new(symbol: String, log_dir: String, verbose_trades: bool) -> std::io::Result<Self> {
        let detector = VolatilityDetector::new(symbol.clone());
        let file_logger = FileLogger::new(&log_dir)?;

        Ok(Self {
            symbol,
            detector,
            file_logger,
            verbose_trades,
            trade_count: 0,
            last_minute: None,
            last_vol_state: false,
            last_status_minute: None,
        })
    }

    fn handle_trade(&mut self, price_str: &str, qty_str: &str, is_maker: bool, ts: i64) {
        let price: Decimal = price_str.parse().unwrap_or(dec!(0));
        let qty: Decimal = qty_str.parse().unwrap_or(dec!(0));

        // 更新时间
        let timestamp = Utc.timestamp_millis_opt(ts).unwrap();

        // 更新波动率检测器
        let vol_stats = self.detector.update(price, timestamp);

        // 更新计数
        self.trade_count += 1;

        // 每笔交易输出状态（方便定位问题）
        info!(
            "[{:02}:{:02}:{:02}] trade#{} price={} qty={} vol_1m={:.4} vol_15m={:.4} is_high={}",
            timestamp.hour(), timestamp.minute(), timestamp.second(),
            self.trade_count, price, qty, vol_stats.vol_1m, vol_stats.vol_15m, vol_stats.is_high_volatility
        );

        // 高波动信号检测（先判断，因为会影响记录逻辑）
        let is_high = vol_stats.is_high_volatility;
        if is_high != self.last_vol_state {
            info!(
                "[{:02}:{:02}:{:02}] VOL_STATE_CHANGE: {} -> {} | vol_1m={:.4} vol_15m={:.4}",
                timestamp.hour(), timestamp.minute(), timestamp.second(),
                self.last_vol_state, is_high, vol_stats.vol_1m, vol_stats.vol_15m
            );
            self.record_signal(is_high, &vol_stats, price, ts);
            self.last_vol_state = is_high;
        }

        // 高波动期间：每分钟状态记录
        if is_high {
            let current_min = get_local_minute();
            if self.last_status_minute.is_none() || self.last_status_minute != Some(current_min) {
                self.record_status(&vol_stats, price, current_min, true);
                self.last_status_minute = Some(current_min);
            }
        }

        // 简洁交易输出（可选）
        if self.verbose_trades {
            let side = if is_maker { "SELL" } else { "BUY" };
            let side_ico = if is_maker { "<<" } else { ">>" };
            println!(
                "{} {} {} {}",
                format_time(ts),
                price_str,
                qty_str,
                format!("{} {}", side_ico, side)
            );
        }
    }

    fn record_signal(&self, is_high: bool, stats: &VolatilityStats, price: Decimal, ts: i64) {
        let signal_type = if is_high { "ENTER_HIGH_VOL" } else { "EXIT_HIGH_VOL" };

        let (th_1m, th_15m) = self.detector.thresholds();

        // 判断触发周期
        let vol_1m_high = stats.vol_1m >= th_1m;
        let vol_15m_high = stats.vol_15m >= th_15m;
        let trigger_period = match (vol_1m_high, vol_15m_high) {
            (true, true) => "BOTH",
            (true, false) => "1M",
            (false, true) => "15M",
            (false, false) => "NONE",
        };

        let signal = VolatilitySignal {
            timestamp: Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            symbol: self.symbol.clone(),
            signal_type: signal_type.to_string(),
            trigger_period: trigger_period.to_string(),
            vol_1m: format!("{:.4}", stats.vol_1m),
            vol_15m: format!("{:.4}", stats.vol_15m),
            price: price.to_string(),
            threshold_1m: format!("{:.2}", th_1m * dec!(100)),
            threshold_15m: format!("{:.2}", th_15m * dec!(100)),
        };

        // 输出到控制台
        if is_high {
            warn!(
                "⚠️ HIGH VOL [{}] vol_1m={:.2}% vol_15m={:.2}% price={}",
                trigger_period,
                stats.vol_1m * dec!(100),
                stats.vol_15m * dec!(100),
                price
            );
        } else {
            info!("📉 Volatility normalized. vol_1m={:.2}% vol_15m={:.2}%", stats.vol_1m * dec!(100), stats.vol_15m * dec!(100));
        }

        // 写入文件
        if let Err(e) = self.file_logger.write_signal(&signal) {
            eprintln!("Failed to write signal: {}", e);
        }
    }

    fn record_status(&self, stats: &VolatilityStats, price: Decimal, minute: (u32, u32, u32), is_continuous_high: bool) {
        let status = StatusSnapshot {
            timestamp: Utc::now().format("%Y-%m-%d %H:%M").to_string(),
            symbol: self.symbol.clone(),
            price: price.to_string(),
            vol_1m: format!("{:.4}", stats.vol_1m),
            vol_15m: format!("{:.4}", stats.vol_15m),
            is_high_vol: stats.is_high_volatility,
            trade_count_1m: self.trade_count,
            kline_1m_close: "-".to_string(),
            kline_15m_close: "-".to_string(),
        };

        // 高波动持续期间的记录，用更醒目的格式
        if is_continuous_high {
            warn!(
                "[{:02}:{:02}] [HIGH_VOL_CONTINUE] price={} vol_1m={:.2}% vol_15m={:.2}% trades={}",
                minute.0, minute.1,
                price,
                stats.vol_1m * dec!(100),
                stats.vol_15m * dec!(100),
                self.trade_count
            );
        }

        // 写入文件
        if let Err(e) = self.file_logger.write_status(&status) {
            eprintln!("Failed to write status: {}", e);
        }
    }
}

// ============================================================================
// Binance Trade 原始数据
// ============================================================================

/// 币安组合Stream消息格式
#[derive(serde::Deserialize)]
struct CombinedStreamMsg {
    #[serde(rename = "stream")]
    stream: String,
    #[serde(rename = "data")]
    data: TradeRaw,
}

/// 多交易对处理器
struct MultiTradeHandler {
    handlers: HashMap<String, TradeHandler>,
    file_logger: FileLogger,
}

impl MultiTradeHandler {
    fn new(symbols: Vec<String>, log_dir: String, verbose_trades: bool) -> std::io::Result<Self> {
        let mut handlers = HashMap::new();
        for symbol in symbols {
            handlers.insert(symbol.to_lowercase(), TradeHandler::new(symbol.clone(), log_dir.clone(), verbose_trades)?);
        }
        let file_logger = FileLogger::new(&log_dir)?;
        Ok(Self { handlers, file_logger })
    }

    fn handle_trade(&mut self, symbol: &str, price_str: &str, qty_str: &str, is_maker: bool, ts: i64) {
        if let Some(handler) = self.handlers.get_mut(symbol) {
            handler.handle_trade(price_str, qty_str, is_maker, ts);
        }
    }
}

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

// ============================================================================
// 主程序
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // 初始化日志
    let log_file = format!("{}/monitor_all.log", args.log_dir);
    std::fs::create_dir_all(&args.log_dir)?;

    let file_appender = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file)?;

    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_span_events(FmtSpan::CLOSE)
        .with_writer(file_appender)
        .with_ansi(false)
        .init();

    let symbols_str = args.symbols.join(", ");

    info!("===========================================");
    info!("  量化交易系统 - 实时监控");
    info!("===========================================");
    info!("  交易对: {}", symbols_str);
    info!("  日志目录: {}", args.log_dir);
    info!("  详细交易: {}", if args.verbose_trades { "是" } else { "否" });
    info!("===========================================\n");

    // 创建多交易对处理器
    let mut multi_handler = MultiTradeHandler::new(
        args.symbols.clone(),
        args.log_dir.clone(),
        args.verbose_trades,
    )?;

    // 构建组合 WebSocket URL: btcusdt@trade/ethusdt@trade/...
    let streams: Vec<String> = args.symbols
        .iter()
        .map(|s| format!("{}@trade", s.to_lowercase()))
        .collect();
    let url = format!(
        "wss://stream.binance.com:9443/stream?streams={}",
        streams.join("/")
    );

    info!("连接 WebSocket: {}", url);
    println!("\n========================================");
    println!("  {} 实时监控", symbols_str);
    println!("  WebSocket: {}", url);
    println!("========================================\n");

    let (ws_stream, _) = connect_async(&url).await?;
    let mut reader = ws_stream;

    info!("已连接，开始接收数据...\n");

    while let Some(msg) = reader.next().await {
        let msg = msg?;
        if let Message::Text(text) = msg {
            if let Ok(msg) = serde_json::from_str::<CombinedStreamMsg>(&text) {
                // 从stream名提取symbol，如 "btcusdt@trade" -> "btcusdt"
                let symbol = msg.stream.replace("@trade", "");
                multi_handler.handle_trade(
                    &symbol,
                    &msg.data.price,
                    &msg.data.quantity,
                    msg.data.is_buyer_maker,
                    msg.data.trade_time,
                );
            }
        }
    }

    Ok(())
}
