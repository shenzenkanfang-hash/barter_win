//! Sandbox Production Mode - 生产级沙盒运行
//!
//! 功能：
//! - 使用 StreamTickGenerator 回放历史数据
//! - 使用 DataFeeder 推送 Tick 数据
//! - 使用 ShadowBinanceGateway 拦截订单
//! - 基于波动率的简单策略
//!
//! 运行:
//!   cargo run --bin sandbox_prod -- --symbol BTCUSDT --fund 10000

#![forbid(unsafe_code)]

use std::sync::Arc;
use std::time::Duration;

use chrono::{TimeZone, Utc};
use parking_lot::RwLock;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tokio::time::sleep;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, filter::LevelFilter};

use b_data_source::{DataFeeder, KLine, Period};
use h_sandbox::{
    ShadowBinanceGateway,
    historical_replay::{StreamTickGenerator, SimulatedTick},
};

// ==================== 常量 ====================

const DEFAULT_SYMBOL: &str = "BTCUSDT";
const DEFAULT_FUND: f64 = 10000.0;

// ==================== 配置 ====================

#[derive(Debug, Clone)]
struct SandboxConfig {
    symbol: String,
    initial_fund: Decimal,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            symbol: DEFAULT_SYMBOL.to_string(),
            initial_fund: dec!(10000),
        }
    }
}

// ==================== 简化策略引擎 ====================

#[derive(Debug, Clone)]
enum Signal {
    Buy,
    Sell,
    Hold,
}

#[derive(Debug, Clone)]
struct StrategyConfig {
    volatility_threshold: f64,
    lookback: usize,
    position_ratio: Decimal,
}

struct SimpleStrategy {
    config: StrategyConfig,
    history: RwLock<Vec<KLine>>,
    position: RwLock<Option<Decimal>>,
    last_signal: RwLock<Signal>,
}

impl SimpleStrategy {
    fn new(_symbol: String, config: StrategyConfig) -> Self {
        Self {
            config,
            history: RwLock::new(Vec::new()),
            position: RwLock::new(None),
            last_signal: RwLock::new(Signal::Hold),
        }
    }

    fn push_kline(&self, kline: KLine) {
        let mut history = self.history.write();
        if history.iter().all(|h| h.timestamp != kline.timestamp) {
            history.push(kline);
            if history.len() > 100 {
                history.remove(0);
            }
        }
    }

    fn calculate_volatility(&self) -> f64 {
        let history = self.history.read();
        if history.len() < self.config.lookback {
            return 0.0;
        }

        let recent = &history[history.len().saturating_sub(self.config.lookback)..];
        let mut total_range = 0.0;
        let mut count = 0.0;

        for k in recent {
            let open = k.open.to_string().parse::<f64>().unwrap_or(0.0);
            let close = k.close.to_string().parse::<f64>().unwrap_or(0.0);
            let high = k.high.to_string().parse::<f64>().unwrap_or(0.0);
            let low = k.low.to_string().parse::<f64>().unwrap_or(0.0);
            
            if open > 0.0 {
                total_range += (high - low) / open * 100.0;
                count += 1.0;
            }
        }

        if count > 0.0 {
            total_range / count
        } else {
            0.0
        }
    }

    fn generate_signal(&self) -> Signal {
        let volatility = self.calculate_volatility();
        
        if volatility > self.config.volatility_threshold {
            *self.last_signal.write() = Signal::Buy;
            Signal::Buy
        } else if volatility < self.config.volatility_threshold * 0.5 {
            *self.last_signal.write() = Signal::Sell;
            Signal::Sell
        } else {
            *self.last_signal.write() = Signal::Hold;
            Signal::Hold
        }
    }

    fn get_position(&self) -> Option<Decimal> {
        *self.position.read()
    }

    fn set_position(&self, qty: Option<Decimal>) {
        *self.position.write() = qty;
    }
}

// ==================== 账户状态追踪 ====================

struct AccountTracker {
    initial_fund: Decimal,
    gateway: Arc<ShadowBinanceGateway>,
}

impl AccountTracker {
    fn new(initial_fund: Decimal, gateway: Arc<ShadowBinanceGateway>) -> Self {
        Self { initial_fund, gateway }
    }

    fn get_equity(&self) -> Decimal {
        self.gateway.get_account().map(|a| a.total_equity).unwrap_or(self.initial_fund)
    }

    fn get_pnl(&self) -> Decimal {
        self.get_equity() - self.initial_fund
    }

    fn get_pnl_percent(&self) -> Decimal {
        if self.initial_fund > Decimal::ZERO {
            (self.get_pnl() / self.initial_fund * dec!(100)).round_dp(2)
        } else {
            dec!(0)
        }
    }
}

// ==================== 模拟 K线迭代器 ====================

struct MockKlineIter {
    current_time: i64,
    end_time: i64,
    base_price: Decimal,
    symbol: String,
}

impl Iterator for MockKlineIter {
    type Item = KLine;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_time >= self.end_time {
            return None;
        }

        let open = self.base_price;
        let rand_val = rand_simple(self.current_time);
        let close_offset = Decimal::try_from((rand_val - 0.5) * 100.0).unwrap_or(dec!(0));
        let close = open + close_offset;
        
        let high = if close > open { close * dec!(1.001) } else { open * dec!(1.001) };
        let low = if close < open { close * dec!(0.999) } else { open * dec!(0.999) };

        let kline = KLine {
            symbol: self.symbol.clone(),
            period: Period::Minute(1),
            open,
            high,
            low,
            close,
            volume: dec!(100),
            timestamp: Utc.timestamp_millis_opt(self.current_time).unwrap(),
        };

        self.current_time += 60_000;
        self.base_price = close;

        Some(kline)
    }
}

fn rand_simple(seed: i64) -> f64 {
    let x = ((seed.wrapping_mul(1103515245).wrapping_add(12345)) % (1 << 31)) as f64;
    x / (1 << 31) as f64
}

// ==================== 主函数 ====================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(true)
                .with_level(true)
                .with_thread_ids(false)
        )
        .with(LevelFilter::INFO)
        .init();

    let config = parse_args();

    tracing::info!("========================================");
    tracing::info!("  生产级沙盒模式启动器");
    tracing::info!("========================================");
    tracing::info!("配置:");
    tracing::info!("  品种: {}", config.symbol);
    tracing::info!("  初始资金: {}", config.initial_fund);
    tracing::info!("========================================\n");

    // 创建组件
    let feeder = Arc::new(DataFeeder::new());
    tracing::info!("✅ 1. DataFeeder 创建成功");

    let gateway = Arc::new(ShadowBinanceGateway::with_default_config(config.initial_fund));
    tracing::info!("✅ 2. ShadowGateway 创建成功");

    let strategy = Arc::new(SimpleStrategy::new(
        config.symbol.clone(),
        StrategyConfig {
            volatility_threshold: 0.5,
            lookback: 20,
            position_ratio: dec!(0.1),
        },
    ));
    tracing::info!("✅ 3. 策略引擎创建成功");

    let account = Arc::new(AccountTracker::new(config.initial_fund, Arc::clone(&gateway)));
    tracing::info!("✅ 4. 账户追踪器创建成功\n");

    // 创建生成器
    let mock_iter = MockKlineIter {
        current_time: Utc::now().timestamp_millis() - 24 * 60 * 60 * 1000,
        end_time: Utc::now().timestamp_millis(),
        base_price: dec!(50000),
        symbol: config.symbol.clone(),
    };

    let mut generator = StreamTickGenerator::from_loader(
        config.symbol.clone(),
        mock_iter,
    );
    tracing::info!("✅ 5. Tick 生成器创建成功\n");

    // 主循环
    tracing::info!("开始回放...");
    tracing::info!("========================================");

    let mut tick_count = 0u64;
    let mut kline_count = 0u64;
    let mut trade_count = 0u64;
    let mut last_report_time = std::time::Instant::now();
    let report_interval = Duration::from_secs(10);

    loop {
        let tick = match generator.next() {
            Some(t) => t,
            None => {
                tracing::info!("数据回放结束");
                break;
            }
        };

        tick_count += 1;
        process_tick(&tick, feeder.as_ref(), gateway.as_ref(), strategy.as_ref(), &mut trade_count, &mut kline_count);

        if tick_count % 60 == 0 {
            kline_count += 1;
            
            if kline_count % 10 == 0 {
                let signal = strategy.generate_signal();
                execute_signal(&signal, gateway.as_ref(), strategy.as_ref(), &config.symbol, &mut trade_count);
            }
        }

        if last_report_time.elapsed() >= report_interval {
            print_report(tick_count, kline_count, trade_count, account.as_ref(), strategy.as_ref());
            last_report_time = std::time::Instant::now();
        }

        sleep(Duration::from_micros(100)).await;
    }

    // 最终报告
    tracing::info!("\n========================================");
    tracing::info!("  回放完成 - 最终报告");
    tracing::info!("========================================");

    print_report(tick_count, kline_count, trade_count, account.as_ref(), strategy.as_ref());

    tracing::info!("\n========================================");
    tracing::info!("  测试完成");
    tracing::info!("========================================\n");

    Ok(())
}

// ==================== 辅助函数 ====================

fn process_tick(
    tick: &SimulatedTick,
    feeder: &DataFeeder,
    gateway: &ShadowBinanceGateway,
    strategy: &SimpleStrategy,
    trade_count: &mut u64,
    kline_count: &mut u64,
) {
    gateway.update_price(&tick.symbol, tick.price);

    let kline_data = KLine {
        symbol: tick.symbol.clone(),
        period: Period::Minute(1),
        open: tick.open,
        high: tick.high,
        low: tick.low,
        close: tick.price,
        volume: tick.qty,
        timestamp: tick.timestamp,
    };

    let tick_model = b_data_source::Tick {
        symbol: tick.symbol.clone(),
        price: tick.price,
        qty: tick.qty,
        timestamp: tick.timestamp,
        kline_1m: Some(kline_data.clone()),
        kline_15m: None,
        kline_1d: None,
    };
    feeder.push_tick(tick_model);
    strategy.push_kline(kline_data);

    *kline_count += 1;
}

fn execute_signal(
    signal: &Signal,
    gateway: &ShadowBinanceGateway,
    strategy: &SimpleStrategy,
    symbol: &str,
    trade_count: &mut u64,
) {
    let current_price = gateway.get_current_price(symbol);
    let position = strategy.get_position();

    match (signal, position) {
        (Signal::Buy, None) => {
            let qty = current_price * dec!(0.001);
            let req = f_engine::types::OrderRequest {
                symbol: symbol.to_string(),
                side: f_engine::types::Side::Buy,
                qty,
                price: None,
                order_type: f_engine::types::OrderType::Market,
            };

            match gateway.place_order(req) {
                Ok(result) => {
                    if !result.order_id.is_empty() {
                        strategy.set_position(Some(qty));
                        *trade_count += 1;
                        tracing::info!(
                            "[BUY] 开仓: 价格={}, 数量={}, 订单ID={}",
                            current_price, qty, result.order_id
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!("[BUY] 开仓失败: {:?}", e);
                }
            }
        }
        (Signal::Sell, Some(qty)) => {
            let req = f_engine::types::OrderRequest {
                symbol: symbol.to_string(),
                side: f_engine::types::Side::Sell,
                qty,
                price: None,
                order_type: f_engine::types::OrderType::Market,
            };

            match gateway.place_order(req) {
                Ok(result) => {
                    if !result.order_id.is_empty() {
                        strategy.set_position(None);
                        *trade_count += 1;
                        tracing::info!(
                            "[SELL] 平仓: 价格={}, 数量={}, 订单ID={}",
                            current_price, qty, result.order_id
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!("[SELL] 平仓失败: {:?}", e);
                }
            }
        }
        _ => {}
    }
}

fn print_report(
    tick_count: u64,
    kline_count: u64,
    trade_count: u64,
    account: &AccountTracker,
    strategy: &SimpleStrategy,
) {
    let volatility = strategy.calculate_volatility();
    let equity = account.get_equity();
    let pnl = account.get_pnl();
    let pnl_pct = account.get_pnl_percent();
    let position = strategy.get_position()
        .map(|q| format!("{}", q))
        .unwrap_or_else(|| "空仓".to_string());

    tracing::info!("----------------------------------------");
    tracing::info!("统计: Ticks={}, K线={}, 交易={}", tick_count, kline_count, trade_count);
    tracing::info!("市场: 波动率={:.2}%, 价格={}", volatility, latest_price(strategy));
    tracing::info!("账户: 权益={}, 盈亏={} ({:.2}%), 持仓={}", equity, pnl, pnl_pct, position);
    tracing::info!("----------------------------------------");
}

fn latest_price(strategy: &SimpleStrategy) -> Decimal {
    let history = strategy.history.read();
    history.last().map(|k| k.close).unwrap_or(Decimal::ZERO)
}

fn parse_args() -> SandboxConfig {
    let mut config = SandboxConfig::default();

    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--symbol" if i + 1 < args.len() => {
                config.symbol = args[i + 1].clone();
                i += 2;
            }
            "--fund" if i + 1 < args.len() => {
                if let Ok(fund) = args[i + 1].parse::<f64>() {
                    config.initial_fund = Decimal::try_from(fund).unwrap_or(dec!(10000));
                }
                i += 2;
            }
            _ => i += 1,
        }
    }

    config
}
