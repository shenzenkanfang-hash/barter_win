//! 完整的 Engine 架构演示
//!
//! 展示 barter-rs 启发的全部新组件:
//! - HistoricalClock: 回测时钟，基于事件时间戳推进（真实墙钟流逝）
//! - SyncRunner: 实现了 Auditor<EngineOutput> trait，生成递增序列号
//! - AuditTick: 每个事件的完整审计记录（event + context）
//! - Processor trait: 统一事件处理接口
//!
//! 运行:
//! ```bash
//! cargo run -p b_data_source --example mock_ws_handshake -- --csv data/btcusdt_1m.csv
//! ```

use std::io::Write;
use parking_lot::Mutex;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use chrono::Utc;

// 从 b_data_source 导入引擎组件（barter-rs 启发）
// 注意：EngineClock trait 与 HistoricalClock 结构体同名，不能同时从同一路径导入
use b_data_source::engine::clock::EngineClock; // trait 用于调用 .time()
use b_data_source::engine::{
    HistoricalClock,    // 回测时钟：基于事件时间 + 真实流逝
    SyncRunner,         // 同步运行器：实现 Auditor<EngineOutput>
    Auditor,             // 审计器 trait：生成 AuditTick<EngineOutput>
    AuditTick,           // 审计标记：事件 + 上下文，用于完整回放
    EngineOutput,        // 引擎输出：sequence + time
};
use b_data_mock::SimulatedKline; // KlineStreamGenerator 产生的类型，与 SimulatedTick 字段相同

// 从 b_data_mock 导入模拟组件
use b_data_mock::{
    ReplaySource, KlineStreamGenerator,
    MockApiGateway, MockExecutionConfig,
};
use b_data_mock::api::mock_account::Side;

// ============================================================================
// 交易信号
// ============================================================================

#[derive(Debug, Clone, Copy)]
enum Signal {
    Buy,
    Sell,
}

// ============================================================================
// EMA 交叉策略
// ============================================================================

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

    /// K 线闭合时检查 EMA 交叉
    fn update(&mut self, price: Decimal, is_kline_closed: bool) -> Option<Signal> {
        self.price_history.push(price);

        if is_kline_closed && self.price_history.len() >= self.ema_period_slow {
            let fast_ema = self.calc_ema(self.ema_period_fast);
            let slow_ema = self.calc_ema(self.ema_period_slow);
            let prev_fast_ema = self.calc_prev_ema(self.ema_period_fast);
            let prev_slow_ema = self.calc_prev_ema(self.ema_period_slow);

            // 金叉：快线上穿慢线
            if prev_fast_ema <= prev_slow_ema && fast_ema > slow_ema {
                self.order_count += 1;
                return Some(Signal::Buy);
            }
            // 死叉：快线下穿慢线
            if prev_fast_ema >= prev_slow_ema && fast_ema < slow_ema {
                self.order_count += 1;
                return Some(Signal::Sell);
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

// ============================================================================
// 回测统计
// ============================================================================

#[derive(Default, Clone)]
struct BacktestStats {
    tick_count: u64,
    kline_count: u32,
    buy_signals: u32,
    sell_signals: u32,
    audit_count: u64,
    last_sequence: u64,
}

// ============================================================================
// 回测引擎（展示完整架构）
// ============================================================================

/// 回测引擎
///
/// 整合所有 barter-rs 启发的组件：
/// - HistoricalClock：时间管理
/// - SyncRunner：序列号生成 + 审计
/// - MockApiGateway：模拟账户/下单
/// - SimpleStrategy：策略逻辑
struct BacktestEngine {
    /// 同步运行器（实现 Auditor trait）
    runner: SyncRunner,
    /// 回测时钟
    clock: HistoricalClock,
    /// 模拟网关
    gateway: MockApiGateway,
    /// 策略
    strategy: Mutex<SimpleStrategy>,
    /// 统计
    stats: Mutex<BacktestStats>,
}

impl BacktestEngine {
    /// 创建新的回测引擎
    ///
    /// 使用首根 K 线时间戳初始化 HistoricalClock
    fn new(first_kline_time: chrono::DateTime<Utc>) -> Self {
        Self {
            // SyncRunner: 初始 sequence=0，每次 audit() 递增
            runner: SyncRunner::new(),
            // HistoricalClock: 基于事件时间戳推进，保留真实流逝
            clock: HistoricalClock::new(first_kline_time),
            // MockApiGateway: 模拟账户，初始余额 10000 USDT
            gateway: MockApiGateway::with_default_config(dec!(10000)),
            strategy: Mutex::new(SimpleStrategy::new()),
            stats: Mutex::new(BacktestStats::default()),
        }
    }

    /// 创建引擎（使用 MockExecutionConfig 精细配置）
    fn with_config(config: MockExecutionConfig) -> Self {
        Self {
            runner: SyncRunner::new(),
            clock: HistoricalClock::from_datetime(Utc::now()),
            gateway: MockApiGateway::with_execution_config(config),
            strategy: Mutex::new(SimpleStrategy::new()),
            stats: Mutex::new(BacktestStats::default()),
        }
    }

    /// 处理单个 Tick（核心流程）
    ///
    /// 流程:
    /// 1. 更新 HistoricalClock（基于事件时间戳）
    /// 2. 更新 MockApiGateway 价格（计算未实现盈亏）
    /// 3. 策略判断（K 线闭合时检查 EMA 交叉）
    /// 4. 执行交易（如有信号）
    /// 5. SyncRunner.audit() 生成 AuditTick<EngineOutput>
    ///
    /// 返回: AuditTick<EngineOutput>（用于完整事件回放）
    fn process_tick(&mut self, tick: &SimulatedKline) -> AuditTick<EngineOutput> {
        // 1. 推进 HistoricalClock（基于事件时间戳）
        self.clock.update(tick.timestamp);

        // 2. 更新网关价格（用于计算持仓盈亏）
        self.gateway.update_price(&tick.symbol, tick.price);

        // 3. 策略判断
        let signal = {
            let mut strat = self.strategy.lock();
            strat.update(tick.price, tick.is_last_in_kline)
        };

        // 4. 执行交易（如有信号）
        if let Some(sig) = signal {
            let mut stats = self.stats.lock();
            match sig {
                Signal::Buy => {
                    stats.buy_signals += 1;
                    drop(stats);
                    let _ = self.gateway.place_order(&tick.symbol, Side::Buy, dec!(0.01), None);
                }
                Signal::Sell => {
                    stats.sell_signals += 1;
                    drop(stats);
                    let _ = self.gateway.place_order(&tick.symbol, Side::Sell, dec!(0.01), None);
                }
            }
        }

        // 5. 更新统计
        {
            let mut stats = self.stats.lock();
            stats.tick_count += 1;
            if tick.is_last_in_kline {
                stats.kline_count += 1;
            }
        }

        // 6. 生成引擎输出（当前状态快照）
        let output = EngineOutput {
            sequence: self.stats.lock().last_sequence,
            time: self.clock.time(),
        };

        // 7. SyncRunner.audit() → AuditTick<EngineOutput>
        //    这是 barter-rs 核心理念：每个事件都有唯一的 sequence + time
        let audit_tick = self.runner.audit(output);

        // 更新统计中的序列号
        {
            let mut stats = self.stats.lock();
            stats.audit_count += 1;
            stats.last_sequence = audit_tick.context.sequence;
        }

        audit_tick
    }

    /// 获取统计
    fn get_stats(&self) -> BacktestStats {
        self.stats.lock().clone()
    }

    /// 获取策略订单数
    fn get_order_count(&self) -> usize {
        self.strategy.lock().get_order_count()
    }

    /// 获取时钟（用于调试）
    fn clock(&self) -> &HistoricalClock {
        &self.clock
    }
}

// ============================================================================
// 主函数
// ============================================================================

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 解析命令行参数
    let args: Vec<String> = std::env::args().collect();
    let csv_path = args.get(1).cloned().unwrap_or_else(|| "data/btcusdt_1m.csv".to_string());

    println!("{}", "=".repeat(70));
    println!("  mock_ws 完整 Engine 架构演示 (barter-rs 启发)");
    println!("{}", "=".repeat(70));
    println!();
    println!("  架构组件:");
    println!("    - HistoricalClock  回测时钟（事件时间 + 真实流逝）");
    println!("    - SyncRunner       Auditor<EngineOutput> 实现");
    println!("    - AuditTick        完整事件审计记录");
    println!("    - MockApiGateway   模拟账户/下单");
    println!("    - EMA 交叉策略      K 线闭合时信号判断");
    println!("{}", "=".repeat(70));
    println!();

    // =========================================================================
    // 1. 加载历史数据（单线程 runtime 同步加载）
    // =========================================================================

    let replay = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async { ReplaySource::from_csv(&csv_path).await })?;

    let total_klines = replay.len();
    let first_time = replay.first_timestamp();

    println!("数据源: {}", csv_path);
    println!("K 线数量: {}", total_klines);

    if total_klines == 0 {
        println!("错误: 没有加载到 K 线数据");
        return Ok(());
    }

    if let Some(t) = first_time {
        println!("首根 K 线时间: {}", t.format("%Y-%m-%d %H:%M:%S UTC"));
    }
    println!();

    // =========================================================================
    // 2. 创建回测引擎（展示完整架构初始化）
    // =========================================================================

    // 使用 MockExecutionConfig 进行精细配置
    let exec_config = MockExecutionConfig::default()
        .with_balance(dec!(10000))
        .with_latency(0);  // 回测无延迟

    // 创建引擎：使用首根 K 线时间戳初始化 HistoricalClock
    let mut engine = if let Some(first_ts) = first_time {
        let mut e = BacktestEngine::new(first_ts);
        e.gateway = MockApiGateway::with_execution_config(exec_config);
        e
    } else {
        BacktestEngine::with_config(exec_config)
    };

    // 打印时钟初始状态
    println!("HistoricalClock 初始时间: {}", engine.clock.time().format("%Y-%m-%d %H:%M:%S UTC"));
    println!("SyncRunner 初始序列号: {}", engine.runner.sequence());
    println!();

    // =========================================================================
    // 3. 创建 Tick 生成器
    // =========================================================================

    let generator = KlineStreamGenerator::new("BTCUSDT".to_string(), Box::new(replay));

    // =========================================================================
    // 4. 同步回测循环（核心演示）
    // =========================================================================

    println!("开始回测（同步 for 循环，无 async/await 死锁风险）...");
    println!();

    let start_time = std::time::Instant::now();

    // 收集前 3 个 AuditTick 用于演示
    let mut demo_ticks: Vec<AuditTick<EngineOutput>> = Vec::with_capacity(3);

    for tick in generator {
        // process_tick() 返回 AuditTick<EngineOutput>
        // 这是 barter-rs 的核心：每个事件都有完整的审计上下文
        let audit_tick = engine.process_tick(&tick);

        // 收集前 3 个用于演示
        if demo_ticks.len() < 3 {
            demo_ticks.push(audit_tick);
        }

        // 进度报告（每 500 根 K 线）
        let stats = engine.get_stats();
        if stats.kline_count > 0 && stats.kline_count % 500 == 0 {
            let elapsed = start_time.elapsed();
            let rate = if elapsed.as_secs() > 0 {
                stats.tick_count as f64 / elapsed.as_secs_f64()
            } else {
                0.0
            };
            print!("\r");
            print!(
                "  [进度] K线: {}/{} | Ticks: {} | 速度: {:.0}/s | 买单: {} | 卖单: {}",
                stats.kline_count,
                total_klines,
                stats.tick_count,
                rate,
                stats.buy_signals,
                stats.sell_signals,
            );
            std::io::stdout().flush().ok();
        }
    }

    // =========================================================================
    // 5. 回测结果
    // =========================================================================

    let elapsed = start_time.elapsed();
    let stats = engine.get_stats();
    let order_count = engine.get_order_count();
    let account = engine.gateway.get_account()?;
    let positions = engine.gateway.get_position("BTCUSDT")?;

    println!("\n");
    println!("{}", "=".repeat(70));
    println!("  回测完成");
    println!("{}", "=".repeat(70));

    // 性能统计
    println!();
    println!("【性能统计】");
    println!("  总耗时:           {:?}", elapsed);
    println!("  总 Ticks:         {}", stats.tick_count);
    println!("  总 K 线:          {}", stats.kline_count);
    let rate = stats.tick_count as f64 / elapsed.as_secs_f64();
    println!("  处理速度:         {:.0} ticks/s", rate);

    // 交易统计
    println!();
    println!("【交易统计】");
    println!("  买入信号:         {}", stats.buy_signals);
    println!("  卖出信号:         {}", stats.sell_signals);
    println!("  策略订单数:       {}", order_count);

    // 审计统计（展示新架构）
    println!();
    println!("【审计记录（barter-rs 核心）】");
    println!("  审计事件数:       {}", stats.audit_count);
    println!("  最终序列号:       {}", stats.last_sequence);

    // 展示前 3 个 AuditTick（证明每个事件都有唯一 context）
    if !demo_ticks.is_empty() {
        println!();
        println!("  前 3 个 AuditTick 示例:");
        for (i, at) in demo_ticks.iter().enumerate() {
            println!(
                "    [{:3}] sequence={:>6}  time={}",
                i + 1,
                at.context.sequence,
                at.context.time.format("%Y-%m-%d %H:%M:%S%.3f")
            );
        }
    }

    // 账户结果
    println!();
    println!("【账户结果】");
    println!("  {}", "-".repeat(40));
    println!("  初始余额:         10000.00 USDT");
    println!("  最终可用余额:     {} USDT", account.available);
    println!("  冻结保证金:       {} USDT", account.frozen_margin);

    if let Some(pos) = positions {
        println!("  多仓数量:          {}", pos.long_qty);
        println!("  多仓均价:         {}", pos.long_avg_price);
        println!("  空仓数量:          {}", pos.short_qty);
        println!("  空仓均价:          {}", pos.short_avg_price);
    }

    // 收益率
    let initial = dec!(10000);
    let final_balance = account.available + account.frozen_margin;
    let pnl = (final_balance - initial) / initial * dec!(100);
    println!();
    println!("  最终权益:         {} USDT", final_balance);
    println!("  收益率:           {:.2}%", pnl);
    if pnl >= Decimal::ZERO {
        println!("  状态:             [盈利]");
    } else {
        println!("  状态:             [亏损]");
    }

    // 时钟状态（证明 HistoricalClock 正确推进）
    println!();
    println!("【时钟状态】");
    println!("  最终时钟时间:     {}", engine.clock.time().format("%Y-%m-%d %H:%M:%S UTC"));

    println!("{}", "=".repeat(70));
    println!();
    println!("  架构演示完成！");
    println!("  - 每个 Tick 都由 SyncRunner 生成唯一的 sequence + time");
    println!("  - AuditTick<EngineOutput> 可用于完整事件回放");
    println!("  - HistoricalClock 基于事件时间戳 + 真实流逝推进");
    println!("  - 同步 for 循环，无 async/await 复杂性，无死锁风险");
    println!();

    Ok(())
}
