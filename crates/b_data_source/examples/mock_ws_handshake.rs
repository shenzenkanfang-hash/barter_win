//! mock_ws 握手模式回测示例
//!
//! 特点：
//! - 等引擎处理完才推送下一个 Tick
//! - 每根 K 线最后一根 Tick 的 is_last_in_kline = true
//! - 用于分钟指标计算和波动率窗口更新
//!
//! 运行:
//! ```bash
//! cargo run --example mock_ws_handshake -- --csv data/btcusdt_1m.csv
//! ```

use std::sync::Arc;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tokio::time::Instant;

// 导入 b_data_source 组件
use b_data_source::{
    ReplaySource,
    ws::{StreamTickGenerator, SimulatedTick},
    api::mock_api::MockApiGateway,
    api::mock_api::gateway::EngineOrderRequest,
    api::mock_api::account::Side,
    create_handshake_channel,
    HandshakeGenerator,
};

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
// 引擎处理函数
// ============================================================================

async fn process_tick(
    tick: &SimulatedTick,
    gateway: &Arc<MockApiGateway>,
    strategy: &mut SimpleStrategy,
    stats: &mut Stats,
) {
    stats.tick_count += 1;

    // 更新网关价格
    gateway.update_price(&tick.symbol, tick.price);

    // 策略判断
    if let Some(signal) = strategy.update(tick.price, tick.is_last_in_kline) {
        match signal {
            Signal::Buy => {
                stats.buy_signals += 1;
                let _ = gateway.place_order(EngineOrderRequest {
                    symbol: tick.symbol.clone(),
                    side: Side::Buy,
                    qty: dec!(0.01),
                    price: None,
                });
            }
            Signal::Sell => {
                stats.sell_signals += 1;
                let _ = gateway.place_order(EngineOrderRequest {
                    symbol: tick.symbol.clone(),
                    side: Side::Sell,
                    qty: dec!(0.01),
                    price: None,
                });
            }
        }
    }

    // K 线闭合时更新统计
    if tick.is_last_in_kline {
        stats.kline_count += 1;
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 解析命令行参数
    let args: Vec<String> = std::env::args().collect();
    let csv_path = args.get(1).cloned().unwrap_or_else(|| "data/btcusdt_1m.csv".to_string());

    println!("=== Mock WS 握手模式回测 ===");
    println!("数据源: {}", csv_path);

    // =========================================================================
    // 1. 初始化组件
    // =========================================================================

    // 加载历史数据
    let replay = ReplaySource::from_csv(&csv_path).await?;
    let total_klines = replay.len();
    println!("加载 K 线数量: {}", total_klines);

    if total_klines == 0 {
        println!("错误: 没有加载到 K 线数据");
        return Ok(());
    }

    // 创建模拟网关
    let gateway = Arc::new(MockApiGateway::with_default_config(dec!(10000)));
    println!("初始余额: 10000 USDT");

    // 创建策略
    let mut strategy = SimpleStrategy::new();

    // =========================================================================
    // 2. 创建握手通道
    // =========================================================================

    // buffer=1 确保严格握手：生成器发送 -> 引擎处理 -> 完成 -> 下一根
    let (sender, receiver) = create_handshake_channel(1, 1);

    // 创建 Tick 生成器
    let klines = replay; // ReplaySource 现在实现了 Iterator
    let generator = StreamTickGenerator::new("BTCUSDT".to_string(), Box::new(klines));

    // 创建握手生成器
    let mut handshake_gen = HandshakeGenerator::new(generator, sender);
    let tick_receiver = receiver;

    // =========================================================================
    // 3. 握手回测循环
    // =========================================================================

    let mut stats = Stats::default();
    let start_time = Instant::now();

    println!("\n开始回测（握手模式）...");

    loop {
        // 生成器：等待上一个完成，获取下一个 Tick
        let tick = match handshake_gen.next().await {
            Some(t) => t,
            None => break, // 数据耗尽
        };

        // 引擎：处理 Tick
        process_tick(&tick, &gateway, &mut strategy, &mut stats).await;

        // 引擎：发送完成信号
        tick_receiver.try_ack();

        // 进度打印（每 10 根 K 线）
        if stats.kline_count > 0 && stats.kline_count % 10 == 0 {
            let elapsed = start_time.elapsed();
            let rate = stats.tick_count as f64 / elapsed.as_secs_f64();

            let account = gateway.get_account()?;
            println!(
                "[进度] K线: {}/{} ({:.1}%) | Ticks: {} | 速度: {:.0}/s | 余额: {} | 买单: {} | 卖单: {}",
                stats.kline_count,
                total_klines,
                stats.kline_count as f64 / total_klines as f64 * 100.0,
                stats.tick_count,
                rate,
                account.available,
                stats.buy_signals,
                stats.sell_signals,
            );
        }
    }

    // =========================================================================
    // 4. 回测结果
    // =========================================================================

    let elapsed = start_time.elapsed();
    let account = gateway.get_account()?;
    let positions = gateway.get_position("BTCUSDT")?;

    println!("\n{}", "=".repeat(60));
    println!("回测完成");
    println!("{}", "=".repeat(60));
    println!("总耗时: {:?}", elapsed);
    println!("总 Ticks: {}", stats.tick_count);
    println!("总 K 线: {}", stats.kline_count);
    println!("买入信号: {}", stats.buy_signals);
    println!("卖出信号: {}", stats.sell_signals);
    println!("策略订单数: {}", strategy.get_order_count());
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
