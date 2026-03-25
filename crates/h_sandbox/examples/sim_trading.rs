//! 模拟交易系统 - 完整闭环
//!
//! 模拟真实生产环境的所有组件：
//! - DataFeeder (数据源)
//! - BacktestStrategy (真实策略: MaCrossStrategy)
//! - ShadowBinanceGateway (劫持网关 + 账户持仓)
//!
//! 运行: cargo run -p h_sandbox --example sim_trading

use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use b_data_source::DataFeeder;
use h_sandbox::backtest::{BacktestStrategy, BacktestTick, MaCrossStrategy, Signal};
use h_sandbox::{ShadowBinanceGateway, ShadowConfig};

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
    fn on_tick(&mut self, symbol: &str, price: Decimal, qty: Decimal) -> Signal {
        // 1. 更新 DataFeeder
        let tick = b_data_source::Tick {
            symbol: symbol.to_string(),
            price,
            qty,
            timestamp: Utc::now(),
            kline_1m: None,
            kline_15m: None,
            kline_1d: None,
        };
        self.data_feeder.push_tick(tick);

        // 2. 更新 gateway 价格（用于计算未实现盈亏）
        self.gateway.update_price(symbol, price);

        // 3. 策略信号
        let backtest_tick = BacktestTick {
            symbol: symbol.to_string(),
            price,
            high: price * dec!(1.001),
            low: price * dec!(0.999),
            volume: qty,
            timestamp: Utc::now(),
            kline_timestamp: Utc::now(),
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
                        println!("[{:?}] 📈 开多 @ {} | 策略: {}", Utc::now(), price, self.strategy.name());
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
                        println!("[{:?}] 📉 平多 @ {} | 策略: {}", Utc::now(), price, self.strategy.name());
                    }
                }
            }
            Signal::Short | Signal::CloseShort | Signal::Hold => {}
        }
    }

    /// 打印状态
    fn print_status(&self, tick_count: u64, symbol: &str, price: Decimal) {
        // 从 gateway 获取真实账户状态
        let account = self.gateway.get_account().unwrap();

        println!(
            "[{:04}] {} @ {} | 权益: {} | 未实现盈亏: {} | 持仓: {}",
            tick_count,
            symbol,
            price,
            account.total_equity,
            account.unrealized_pnl,
            if self.position_open { "有" } else { "无" }
        );
    }
}

#[tokio::main]
async fn main() {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║            模拟交易系统 - 完整闭环测试                          ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    // 配置
    let symbol = "BTCUSDT";
    let initial_balance = dec!(10000.0);
    let tick_count = 1000;

    println!("配置:");
    println!("  品种: {}", symbol);
    println!("  初始资金: {} USDT", initial_balance);
    println!("  测试Tick数: {}", tick_count);
    println!();

    // 1. 创建策略
    let strategy = MaCrossStrategy::new(5, 10);
    println!("  策略: {}", strategy.name());

    // 2. 创建系统
    println!("初始化组件...");
    let mut system = SimTradingSystem::new(initial_balance, strategy);
    println!("✅ 组件初始化完成\n");

    // 3. 生成模拟 Tick
    println!("开始模拟交易...\n");

    let start = Instant::now();
    let mut current_price = dec!(50000.0);
    let mut last_print = Instant::now();

    for i in 0..tick_count {
        // 生成 tick
        let price_change = if i % 20 < 10 { dec!(1.0005) } else { dec!(0.9995) };
        current_price = current_price * price_change;

        // 处理 tick
        let _signal = system.on_tick(symbol, current_price, dec!(0.001));

        // 定期打印状态
        if last_print.elapsed() > Duration::from_secs(5) || i == tick_count - 1 {
            system.print_status(i, symbol, current_price);
            last_print = Instant::now();
        }

        // 模拟延迟
        tokio::time::sleep(Duration::from_micros(100)).await;
    }

    let elapsed = start.elapsed();

    // 4. 最终报告（从 gateway 获取真实账户状态）
    let final_account = system.gateway.get_account().unwrap();
    let equity = final_account.total_equity;
    let unrealized_pnl = final_account.unrealized_pnl;
    let available = final_account.available;

    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║                    模拟交易报告                               ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();
    println!("  运行时长: {:.2}s", elapsed.as_secs_f64());
    println!("  处理Tick: {}", tick_count);
    println!("  处理速率: {:.0} ticks/s", tick_count as f64 / elapsed.as_secs_f64());
    println!();

    println!("  初始资金: {}", initial_balance);
    println!("  最终权益: {}", equity);
    println!("  未实现盈亏: {}", unrealized_pnl);
    println!("  可用余额: {}", available);
    println!("  收益率: {:.2}%", ((equity - initial_balance) / initial_balance) * dec!(100));

    println!("\n✅ 模拟交易完成");
}
