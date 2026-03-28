//! mock_ws 同步模式回测示例
//!
//! 特点：
//! - 同步 for 循环处理 Tick，无 async/await 复杂性
//! - 每根 K 线最后一根 Tick 的 is_last_in_kline = true
//! - 用于分钟指标计算和波动率窗口更新
//!
//! 运行:
//! ```bash
//! cargo run --example mock_ws_handshake -- --csv data/btcusdt_1m.csv
//! ```

use std::io::Write;
use parking_lot::Mutex;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

// 从 b_data_mock 导入模拟组件
use b_data_mock::{
    ReplaySource, StreamTickGenerator, SimulatedTick, MockApiGateway,
};
use b_data_mock::api::mock_account::Side;

// ============================================================================
// 简化的策略
// ============================================================================

/// 简单的 EMA 交叉策略
struct SimpleStrategy {
    ema_fast: Option<Decimal>,
    ema_slow: Option<Decimal>,
    ema_period_fast: usize,
    ema_period_slow: usize,
    price_history: Vec<Decimal>,
    order_count: usize,
}

impl SimpleStrategy {
    fn new() -> Self {
        Self {
            ema_fast: None,
            ema_slow: None,
            ema_period_fast: 5,
            ema_period_slow: 20,
            price_history: Vec::new(),
            order_count: 0,
        }
    }

    /// 更新 EMA 并返回信号
    fn update(&mut self, price: Decimal, is_kline_closed: bool) -> Option<Signal> {
        self.price_history.push(price);

        // K 线闭合时检查交叉
        if is_kline_closed {
            if self.price_history.len() >= self.ema_period_slow {
                let fast_ema = self.calc_ema(self.ema_period_fast);
                let slow_ema = self.calc_ema(self.ema_period_slow);
                let prev_fast_ema = self.calc_prev_ema(self.ema_period_fast);
                let prev_slow_ema = self.calc_prev_ema(self.ema_period_slow);

                // 金叉
                if prev_fast_ema <= prev_slow_ema && fast_ema > slow_ema {
                    self.order_count += 1;
                    return Some(Signal::Buy);
                }
                // 死叉
                if prev_fast_ema >= prev_slow_ema && fast_ema < slow_ema {
                    self.order_count += 1;
                    return Some(Signal::Sell);
                }
            }
        }

        None
    }

    fn calc_ema(&self, period: usize) -> Decimal {
        let start = self.price_history.len().saturating_sub(period);
        let slice: Vec<Decimal> = self.price_history[start..].to_vec();
        let sum: Decimal = slice.iter().sum();
        sum / Decimal::from(period)
    }

    fn calc_prev_ema(&self, period: usize) -> Decimal {
        if self.price_history.len() <= period {
            return self.calc_ema(period);
        }
        let start = self.price_history.len().saturating_sub(period + 1);
        let end = self.price_history.len().saturating_sub(1);
        let slice: Vec<Decimal> = self.price_history[start..end].to_vec();
        let sum: Decimal = slice.iter().sum();
        sum / Decimal::from(period)
    }

    fn get_order_count(&self) -> usize {
        self.order_count
    }
}

impl Default for SimpleStrategy {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy)]
enum Signal {
    Buy,
    Sell,
}

// ============================================================================
// 引擎处理函数（同步版本）
// ============================================================================

/// 处理单个 Tick（同步版本）
///
/// 直接处理，无需 async
fn process_tick(
    tick: &SimulatedTick,
    gateway: &MockApiGateway,
    strategy: &Mutex<SimpleStrategy>,
    stats: &Mutex<Stats>,
) {
    {
        let mut s = stats.lock();
        s.tick_count += 1;
    }

    // 更新网关价格
    gateway.update_price(&tick.symbol, tick.price);

    // 策略判断
    let signal = {
        let mut strat = strategy.lock();
        strat.update(tick.price, tick.is_last_in_kline)
    };

    if let Some(sig) = signal {
        match sig {
            Signal::Buy => {
                {
                    let mut s = stats.lock();
                    s.buy_signals += 1;
                }
                let _ = gateway.place_order(&tick.symbol, Side::Buy, dec!(0.01), None);
            }
            Signal::Sell => {
                {
                    let mut s = stats.lock();
                    s.sell_signals += 1;
                }
                let _ = gateway.place_order(&tick.symbol, Side::Sell, dec!(0.01), None);
            }
        }
    }

    // K 线闭合时更新统计
    if tick.is_last_in_kline {
        let mut s = stats.lock();
        s.kline_count += 1;
    }
}

#[derive(Default)]
struct Stats {
    tick_count: u64,
    kline_count: u32,
    buy_signals: u32,
    sell_signals: u32,
}

// ============================================================================
// 主函数
// ============================================================================

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 解析命令行参数
    let args: Vec<String> = std::env::args().collect();
    let csv_path = args.get(1).cloned().unwrap_or_else(|| "data/btcusdt_1m.csv".to_string());

    println!("=== Mock WS 同步模式回测 ===");
    println!("数据源: {}", csv_path);

    // =========================================================================
    // 1. 初始化组件（使用单线程 runtime 同步加载数据）
    // =========================================================================

    // 使用单线程 runtime 同步加载数据
    // ReplaySource::from_csv 是 async 函数，需要 runtime 来执行
    let replay = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async { ReplaySource::from_csv(&csv_path).await })?;

    let total_klines = replay.len();
    println!("加载 K 线数量: {}", total_klines);

    if total_klines == 0 {
        println!("错误: 没有加载到 K 线数据");
        return Ok(());
    }

    // 创建模拟网关
    let gateway = MockApiGateway::with_default_config(dec!(10000));
    println!("初始余额: 10000 USDT");

    // 创建策略
    let strategy = Mutex::new(SimpleStrategy::new());

    // 创建统计
    let stats = Mutex::new(Stats::default());

    // =========================================================================
    // 2. 创建 Tick 生成器
    // =========================================================================

    // StreamTickGenerator 实现了 Iterator trait，直接使用 replay 作为数据源
    let generator = StreamTickGenerator::new("BTCUSDT".to_string(), Box::new(replay));

    // =========================================================================
    // 3. 同步回测循环
    // =========================================================================

    let start_time = std::time::Instant::now();
    println!("\n开始回测（同步模式）...");

    // 同步 for 循环：直接遍历 Tick，无 async/await 复杂性
    for tick in generator {
        // 处理 Tick
        process_tick(&tick, &gateway, &strategy, &stats);

        // 进度打印（每 100 根 Tick）
        let s = stats.lock();
        if s.tick_count % 100 == 0 {
            drop(s);
            let elapsed = start_time.elapsed();
            let s = stats.lock();
            let rate = if elapsed.as_secs() > 0 {
                s.tick_count as f64 / elapsed.as_secs_f64()
            } else {
                0.0
            };
            println!(
                "[进度] K线: {}/{} | Ticks: {} | 速度: {:.0}/s | 买单: {} | 卖单: {}",
                s.kline_count,
                total_klines,
                s.tick_count,
                rate,
                s.buy_signals,
                s.sell_signals,
            );
            print!("\r");
            std::io::stdout().flush().ok();
        }
    }

    // =========================================================================
    // 4. 回测结果
    // =========================================================================

    let elapsed = start_time.elapsed();
    let s = stats.lock();
    let order_count = strategy.lock().get_order_count();
    let account = gateway.get_account()?;
    let positions = gateway.get_position("BTCUSDT")?;

    println!("\n{}", "=".repeat(60));
    println!("回测完成");
    println!("{}", "=".repeat(60));
    println!("总耗时: {:?}", elapsed);
    println!("总 Ticks: {}", s.tick_count);
    println!("总 K 线: {}", s.kline_count);
    println!("买入信号: {}", s.buy_signals);
    println!("卖出信号: {}", s.sell_signals);
    println!("策略订单数: {}", order_count);
    println!("{}", "-".repeat(60));
    println!("初始余额: 10000 USDT");
    println!("最终余额: {} USDT", account.available);
    println!("冻结保证金: {} USDT", account.frozen_margin);
    if let Some(pos) = positions {
        println!("多仓数量: {}", pos.long_qty);
        println!("空仓数量: {}", pos.short_qty);
    }

    // 计算收益率
    let initial = dec!(10000);
    let final_balance = account.available + account.frozen_margin;
    let pnl = (final_balance - initial) / initial * dec!(100);
    println!("收益率: {:.2}%", pnl);
    println!("{}", "=".repeat(60));

    Ok(())
}
