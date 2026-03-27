//! Pure Sandbox - 纯沙盒（仅数据注入）
//!
//! ## 架构
//! ```
//! StreamTickGenerator → Store → Trader
//! ShadowBinanceGateway 拦截订单
//! ```
//!
//! ## 使用
//! ```bash
//! cargo run --bin sandbox_pure -- -s HOTUSDT -f 10000
//! ```

use std::sync::Arc;
use anyhow::Result;
use chrono::TimeZone;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tokio::sync::mpsc;
use tracing_subscriber::prelude::*;

use b_data_source::{MarketDataStore, KLine, Period, Tick, ws::kline_1m::ws::KlineData};
use h_sandbox::{ShadowBinanceGateway, historical_replay::StreamTickGenerator};
use d_checktable::h_15m::{Trader, TraderConfig, Executor, Repository};

// ============================================================================
// Config
// ============================================================================

#[derive(Debug, Clone)]
struct Config {
    symbol: String,
    fund: Decimal,
    data_file: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            symbol: "HOTUSDT".into(),
            fund: dec!(10000),
            data_file: None,
        }
    }
}

impl Config {
    #[allow(dead_code)]
    fn with_symbol(mut self, s: &str) -> Self {
        self.symbol = s.into();
        self
    }
    #[allow(dead_code)]
    fn with_fund(mut self, f: Decimal) -> Self {
        self.fund = f;
        self
    }
    #[allow(dead_code)]
    fn with_data_file(mut self, f: &str) -> Self {
        self.data_file = Some(f.into());
        self
    }
}

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cfg = parse_args();
    println!("\n=== Pure Sandbox ===\n");
    println!("Symbol: {}", cfg.symbol);
    println!("Fund: {}", cfg.fund);
    run(cfg).await
}

async fn run(cfg: Config) -> Result<()> {
    let sym = &cfg.symbol;

    // 1. Gateway
    let gw_cfg = h_sandbox::config::ShadowConfig::new(cfg.fund);
    let gateway = Arc::new(ShadowBinanceGateway::new(cfg.fund, gw_cfg));
    tracing::info!(symbol = sym, "Gateway created");

    // 2. Store
    let store = b_data_source::default_store(); // keep full path for function call
    tracing::info!(symbol = sym, "Store created");

    // 3. Executor
    let exec_cfg = d_checktable::h_15m::ExecutorConfig {
        symbol: sym.clone(),
        order_interval_ms: 100,
        initial_ratio: dec!(0.05),
        lot_size: dec!(0.001),
        max_position: dec!(0.15),
    };
    let executor = Arc::new(Executor::new(exec_cfg));

    // 4. Repository
    let db_path = format!("./data/{}_records.db", sym);
    let repository = Arc::new(Repository::new(sym, &db_path)?);

    // 5. Trader
    let trader_cfg = TraderConfig {
        symbol: sym.clone(),
        interval_ms: 100,
        max_position: dec!(0.15),
        initial_ratio: dec!(0.05),
        db_path,
        order_interval_ms: 100,
        lot_size: dec!(0.001),
    };
    let trader = Arc::new(Trader::new(trader_cfg, executor, repository, store.clone()));

    // 6. Channel
    let (tx, rx) = mpsc::channel(1024);

    // 7. Spawn trader
    let t = trader.clone();
    tokio::spawn(async move { t.run(rx).await });

    // 8. Load data
    let path = cfg.data_file.unwrap_or_else(|| format!("data/{}_1m.csv", sym));
    let klines = load_csv(&path)?;

    // 9. Inject loop
    let mut tick_stream = StreamTickGenerator::new(sym.clone(), Box::new(klines.into_iter()));
    let mut count = 0u64;

    while let Some(tick) = tick_stream.next() {
        write_store(store.clone(), sym, &tick);
        gateway.update_price(sym, tick.price);

        let t = Tick {
            symbol: tick.symbol,
            price: tick.price,
            qty: tick.qty,
            timestamp: tick.timestamp,
            sequence_id: tick.sequence_id,
            kline_1m: None,
            kline_15m: None,
            kline_1d: None,
        };

        if tx.send(t).await.is_err() {
            tracing::warn!(symbol = sym, "Trader channel closed");
            break;
        }

        count += 1;
        if count % 1000 == 0 {
            tracing::info!(symbol = sym, count = count, "injected");
        }
    }

    tracing::info!(symbol = sym, count = count, "Injection done");
    drop(tx);
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // 10. Results
    if let Ok(acc) = gateway.get_account() {
        println!("\n=== Account ===");
        println!("Equity: {}", acc.total_equity);
        println!("Available: {}", acc.available);
        println!("Frozen: {}", acc.frozen_margin);
        println!("Unrealized PnL: {}", acc.unrealized_pnl);
    }
    Ok(())
}

fn load_csv(path: &str) -> Result<Vec<KLine>> {
    use std::fs::File;
    use std::io::BufRead;
    let file = File::open(path)?;
    let reader = std::io::BufReader::new(file);
    let mut klines = Vec::new();
    for line in reader.lines().skip(1) {
        let line = line?;
        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() < 6 { continue; }
        let ts: i64 = parts[0].parse().unwrap_or(0);
        let open: Decimal = parts[1].parse().unwrap_or(dec!(0));
        let high: Decimal = parts[2].parse().unwrap_or(dec!(0));
        let low: Decimal = parts[3].parse().unwrap_or(dec!(0));
        let close: Decimal = parts[4].parse().unwrap_or(dec!(0));
        let vol: Decimal = parts[5].parse().unwrap_or(dec!(0));
        klines.push(KLine {
            symbol: "HOTUSDT".into(),
            period: Period::Minute(1),
            open,
            high,
            low,
            close,
            volume: vol,
            timestamp: chrono::Utc.timestamp_opt(ts, 0).unwrap(),
            is_closed: true,
        });
    }
    Ok(klines)
}

fn write_store(store: Arc<dyn MarketDataStore + Send + Sync>, symbol: &str, tick: &h_sandbox::historical_replay::SimulatedTick) {
    let data = KlineData {
        kline_start_time: tick.timestamp.timestamp_millis(),
        kline_close_time: tick.timestamp.timestamp_millis() + 60000,
        symbol: symbol.into(),
        interval: "1m".into(),
        open: tick.open.to_string(),
        high: tick.high.to_string(),
        low: tick.low.to_string(),
        close: tick.price.to_string(),
        volume: tick.volume.to_string(),
        is_closed: true,
    };
    store.write_kline(symbol, data, true);
}

fn parse_args() -> Config {
    let mut cfg = Config::default();
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-s" | "--symbol" if i + 1 < args.len() => { cfg.symbol = args[i + 1].clone(); i += 2; }
            "-f" | "--fund" if i + 1 < args.len() => {
                if let Ok(f) = args[i + 1].parse::<f64>() {
                    cfg.fund = Decimal::new((f * 10000.0) as i64, 4);
                }
                i += 2;
            }
            "-d" | "--data" if i + 1 < args.len() => { cfg.data_file = Some(args[i + 1].clone()); i += 2; }
            _ => i += 1,
        }
    }
    cfg
}
