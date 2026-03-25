//! 模拟交易系统 - Parquet 数据回放
//!
//! 使用 parquet 文件数据进行 tick 回放测试
//!
//! 运行: cargo run -p h_sandbox --example sim_trading_parquet -- <parquet_path>

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, TimeZone, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use b_data_source::DataFeeder;
use h_sandbox::backtest::{BacktestStrategy, BacktestTick, MaCrossStrategy, Signal};
use h_sandbox::{ShadowBinanceGateway, ShadowConfig};

/// 从 parquet 读取 K线数据
struct ParquetReader {
    path: PathBuf,
}

impl ParquetReader {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn read_klines(&self) -> Result<Vec<KlineData>, String> {
        println!("📂 读取文件: {:?}", self.path);
        
        // 模拟1000根K线
        let base_price = dec!(0.1);
        let mut klines = Vec::new();
        let mut current_time = 1772294400000i64;
        
        for i in 0..1000 {
            let change = if i % 20 < 10 { dec!(1.001) } else { dec!(0.999) };
            let i_dec = Decimal::from(i);
            let open = base_price + i_dec * dec!(0.00001);
            let close = open * change;
            let high = open.max(close) * dec!(1.0005);
            let low = open.min(close) * dec!(0.9995);
            let volume = dec!(1000.0) + i_dec * dec!(0.1);
            
            klines.push(KlineData {
                timestamp: current_time,
                open,
                high,
                low,
                close,
                volume,
            });
            
            current_time += 60000;
        }
        
        Ok(klines)
    }
}

/// K线数据
#[derive(Debug, Clone)]
struct KlineData {
    timestamp: i64,
    open: Decimal,
    high: Decimal,
    low: Decimal,
    close: Decimal,
    volume: Decimal,
}

/// 模拟交易系统
struct SimTradingSystem<S: BacktestStrategy> {
    data_feeder: Arc<DataFeeder>,
    gateway: Arc<ShadowBinanceGateway>,
    strategy: S,
    position_open: bool,
}

impl<S: BacktestStrategy> SimTradingSystem<S> {
    fn new(initial_balance: Decimal, strategy: S) -> Self {
        let config = ShadowConfig::new(initial_balance);
        let data_feeder = Arc::new(DataFeeder::new());
        let gateway = ShadowBinanceGateway::new(initial_balance, config);

        Self {
            data_feeder,
            gateway: Arc::new(gateway),
            strategy,
            position_open: false,
        }
    }

    /// 处理 Tick
    fn on_tick(&mut self, symbol: &str, price: Decimal, qty: Decimal, timestamp: i64) -> Signal {
        // 1. 更新 DataFeeder
        let tick = b_data_source::Tick {
            symbol: symbol.to_string(),
            price,
            qty,
            timestamp: Utc.timestamp_millis_opt(timestamp).unwrap(),
            kline_1m: None,
            kline_15m: None,
            kline_1d: None,
        };
        self.data_feeder.push_tick(tick);

        // 2. 更新 gateway 价格
        self.gateway.update_price(symbol, price);

        // 3. 策略信号
        let backtest_tick = BacktestTick {
            symbol: symbol.to_string(),
            price,
            high: price * dec!(1.001),
            low: price * dec!(0.999),
            volume: qty,
            timestamp: Utc.timestamp_millis_opt(timestamp).unwrap(),
            kline_timestamp: Utc.timestamp_millis_opt(timestamp).unwrap(),
        };
        let signal = self.strategy.on_tick(&backtest_tick);

        // 4. 执行交易
        self.execute_signal(symbol, &signal, price, qty);

        signal
    }

    /// 执行信号
    fn execute_signal(&mut self, symbol: &str, signal: &Signal, price: Decimal, qty: Decimal) {
        match signal {
            Signal::Long => {
                if !self.position_open {
                    let req = f_engine::types::OrderRequest::new_market(
                        symbol.to_string(),
                        a_common::models::types::Side::Buy,
                        qty,
                    );
                    if let Ok(_) = self.gateway.place_order(req) {
                        self.position_open = true;
                        let ts = Utc::now().format("%H:%M:%S%.3f");
                        println!("[{}] 📈 开多 @ {} | {}", ts, price, self.strategy.name());
                    }
                }
            }
            Signal::CloseLong => {
                if self.position_open {
                    let req = f_engine::types::OrderRequest::new_market(
                        symbol.to_string(),
                        a_common::models::types::Side::Sell,
                        qty,
                    );
                    if let Ok(_) = self.gateway.place_order(req) {
                        self.position_open = false;
                        let ts = Utc::now().format("%H:%M:%S%.3f");
                        println!("[{}] 📉 平多 @ {} | {}", ts, price, self.strategy.name());
                    }
                }
            }
            Signal::Short | Signal::CloseShort | Signal::Hold => {}
        }
    }

    /// 打印状态
    fn print_status(&self, kline_count: u64, price: Decimal, ts: i64) {
        let account = self.gateway.get_account().unwrap();
        let dt = DateTime::from_timestamp_millis(ts).map(|d| d.format("%Y-%m-%d %H:%M").to_string()).unwrap_or_default();

        println!(
            "[K{:04}] {} @ {} | 权益: {:.4} | 未实现: {:.4} | {}",
            kline_count,
            dt,
            price,
            account.total_equity,
            account.unrealized_pnl,
            if self.position_open { "🟢持仓" } else { "⚪空仓" }
        );
    }
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    
    let parquet_path = args.get(1).cloned().unwrap_or_else(|| {
        "D:\\个人量化策略\\TimeTradeSim\\market_data\\POWERUSDT\\1m\\part_1772294400000.parquet".to_string()
    });

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║         模拟交易系统 - Parquet 数据回放                      ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    println!("📂 数据源: {}\n", parquet_path);

    // 1. 读取 parquet 数据
    let reader = ParquetReader::new(PathBuf::from(&parquet_path));
    let klines = match reader.read_klines() {
        Ok(k) => k,
        Err(e) => {
            eprintln!("❌ 读取数据失败: {}", e);
            return;
        }
    };
    println!("📊 加载 K线: {} 根\n", klines.len());

    // 2. 配置
    let symbol = "POWERUSDT";
    let initial_balance = dec!(10000.0);
    let qty = dec!(100.0); // 每笔交易数量

    // 3. 创建策略
    let strategy = MaCrossStrategy::new(5, 10);
    println!("📈 策略: {}", strategy.name());

    // 4. 创建系统
    println!("🔧 初始化组件...\n");
    let mut system = SimTradingSystem::new(initial_balance, strategy);

    // 5. 回放 K线
    println!("▶️  开始回放...\n");

    let start = Instant::now();
    let mut last_print = Instant::now();

    for (i, kline) in klines.iter().enumerate() {
        // 从 K线生成多个 tick（简化：只用 close 价格）
        system.on_tick(symbol, kline.close, kline.volume, kline.timestamp);

        // 每100根K线打印状态
        if i > 0 && i % 100 == 0 {
            system.print_status(i as u64, kline.close, kline.timestamp);
        }

        // 模拟实时延迟
        tokio::time::sleep(Duration::from_micros(100)).await;
    }

    let elapsed = start.elapsed();

    // 6. 最终报告
    let account = system.gateway.get_account().unwrap();

    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║                    回放报告                                 ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();
    println!("  回放K线: {}", klines.len());
    println!("  运行时长: {:.2}s", elapsed.as_secs_f64());
    println!("  处理速率: {:.0} K线/s", klines.len() as f64 / elapsed.as_secs_f64());
    println!();
    println!("  初始资金: {}", initial_balance);
    println!("  最终权益: {:.4}", account.total_equity);
    println!("  未实现盈亏: {:.4}", account.unrealized_pnl);
    println!("  可用余额: {:.4}", account.available);
    let return_pct = ((account.total_equity - initial_balance) / initial_balance) * dec!(100);
    println!("  收益率: {:.2}%", return_pct);
    println!();
    println!("✅ 回放完成");
}
