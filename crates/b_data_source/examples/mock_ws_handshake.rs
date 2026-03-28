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
use std::io::Write;
use parking_lot::Mutex;
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
    strategy: &Arc<Mutex<SimpleStrategy>>,
    stats: &Arc<Mutex<Stats>>,
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
                let _ = gateway.place_order(EngineOrderRequest {
                    symbol: tick.symbol.clone(),
                    side: Side::Buy,
                    qty: dec!(0.01),
                    price: None,
                });
            }
            Signal::Sell => {
                {
                    let mut s = stats.lock();
                    s.sell_signals += 1;
                }
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 使用 LocalSet 让 spawn_local 能工作
    let result = tokio::task::LocalSet::new()
        .run_until(async_main())
        .await;
    result
}

async fn async_main() -> Result<(), Box<dyn std::error::Error>> {
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
    let strategy = Arc::new(Mutex::new(SimpleStrategy::new()));

    // 创建统计（engine 更新，主循环读取）
    let stats = Arc::new(Mutex::new(Stats::default()));

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

    // =========================================================================
    // 3. 握手回测循环
    // =========================================================================

    let start_time = Instant::now();
    println!("\n开始回测（握手模式）...");

    // =========================================================================
    // 3. 回测循环（Generator 推送 + Engine 处理）
    // =========================================================================
    //
    // Generator: 推送 tick 并等 done（通过 spawn_local 在单线程执行）
    // Engine: 从 channel 收 tick 并处理
    //
    // 使用 tokio::task::spawn_local 确保 engine 在主线程运行，
    // 从而保证 yield_now() 能让出给 engine

    let gateway2 = gateway.clone();
    let strategy2 = strategy.clone();
    let stats2 = stats.clone();

    // 使用 spawn_local：engine 在主线程运行，yield_now 能切换给它
    let engine_handle = tokio::task::spawn_local(async move {
        let mut engine_receiver = receiver;
        while let Some(tick) = engine_receiver.recv_and_ack().await {
            // Engine：用 tick 处理
            process_tick(&tick, &gateway2, &strategy2, &stats2).await;
            // 发 ack 唤醒 generator
            engine_receiver.ack().await.ok();
        }
    });

    // Generator 循环：推送 tick，yield 让 engine 执行
    loop {
        let _tick = match handshake_gen.next().await {
            Some(t) => t,
            None => break,
        };

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

    // Generator 结束，drop sender 让 engine 的 receiver 收到 None
    drop(handshake_gen);

    // 等待 engine 完成
    engine_handle.await.ok();

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
