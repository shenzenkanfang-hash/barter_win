//! Mock Trading System - 使用 b_data_mock 数据源 + 策略事件追踪
//!
//! 基于 HOTUSDT 真实历史数据，完整追踪策略行为：
//! - 信号生成
//! - 风控检查
//! - 订单构造
//! - 模拟成交
//! - 仓位变化
//! - PnL 曲线
//!
//! 运行: cargo run --bin mock_trading

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal_macros::dec;

use b_data_mock::{
    OrderInterceptor, OrderInterceptorConfig,
    TickInterceptor, MockApiGateway,
    KlineStreamGenerator, KLine,
    Period,
    StrategyEventTracker, SimpleMatchEngine,
};
use a_common::heartbeat as hb;
use futures_util::{stream, StreamExt};

// ============================================================================
// 常量配置
// ============================================================================

const INITIAL_BALANCE: Decimal = dec!(10000);
const SYMBOL: &str = "HOTUSDT";
const CSV_PATH: &str = "D:/RusProject/barter-rs-main/data/HOTUSDT_1m_20251009_20251011.csv";

// 策略参数
const HISTORY_WINDOW: usize = 20;        // 价格历史窗口
const PINBAR_BODY_RATIO: Decimal = dec!(0.3);  // 实体/整幅 < 30%
const PINBAR_WICK_RATIO: Decimal = dec!(0.6);   // 影线/整幅 > 60%
const POSITION_SIZE: Decimal = dec!(1.0); // 每笔开仓数量 (1 lot)
const MIN_BALANCE: Decimal = dec!(1000);  // 最小余额
const MAX_DAILY_TRADES: u32 = 10;        // 每日最大交易次数

// 心跳测试点ID
const BS_001: &str = "BS-001";
const CP_001: &str = "CP-001";
const DT_001: &str = "DT-001";
const ER_001: &str = "ER-001";
const FE_001: &str = "FE-001";
const EV_001: &str = "EV-001"; // StrategyEventTracker

// ============================================================================
// 简单策略：Pin Bar 检测
// ============================================================================

struct SimpleStrategy {
    price_history: VecDeque<SimulatedKline>,
    position_side: Option<String>,
    balance: Decimal,
    daily_trade_count: u32,
    last_trade_date: Option<DateTime<Utc>>,
    order_id_counter: u64,
}

#[derive(Debug, Clone)]
struct SimulatedKline {
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub timestamp: DateTime<Utc>,
    pub is_closed: bool,
}

impl SimpleStrategy {
    fn new() -> Self {
        Self {
            price_history: VecDeque::with_capacity(HISTORY_WINDOW),
            position_side: None,
            balance: INITIAL_BALANCE,
            daily_trade_count: 0,
            last_trade_date: None,
            order_id_counter: 0,
        }
    }

    /// 更新价格历史
    fn update_price(&mut self, kline: &SimulatedKline) {
        if kline.is_closed {
            self.price_history.push_back(SimulatedKline {
                open: kline.open,
                high: kline.high,
                low: kline.low,
                close: kline.close,
                timestamp: kline.timestamp,
                is_closed: true,
            });

            if self.price_history.len() > HISTORY_WINDOW {
                self.price_history.pop_front();
            }
        }
    }

    /// 生成下一个订单ID
    fn next_order_id(&mut self) -> String {
        self.order_id_counter += 1;
        format!("ORD_{:06}", self.order_id_counter)
    }

    /// 检测 Pin Bar 信号
    /// 返回 (方向, 强度, 信号类型)
    fn detect_signal(&self) -> Option<(String, u8, String)> {
        if self.price_history.len() < 5 {
            return None;
        }

        let history: Vec<_> = self.price_history.iter().rev().collect();

        // 至少需要前4根确认信号
        if history.len() < 5 {
            return None;
        }

        // 检测最近完成的K线
        let current = history[0];
        let prev1 = history[1];
        let prev2 = history[2];

        // 计算当前K线的各项指标
        let body = (current.close - current.open).abs();
        let range = current.high - current.low;
        let upper_wick = current.high - current.close.max(current.open);
        let lower_wick = current.close.min(current.open) - current.low;

        if range.is_zero() {
            return None;
        }

        let body_ratio = body / range;
        let upper_wick_ratio = upper_wick / range;
        let lower_wick_ratio = lower_wick / range;

        // 检测看涨 Pin Bar（下影线长）
        if body_ratio < PINBAR_BODY_RATIO && lower_wick_ratio > PINBAR_WICK_RATIO {
            // 影线长度至少是实体的3倍
            if lower_wick > body * dec!(3) {
                // 确认前两根K线趋势向下（做多信号更有力）
                let prev_trend_down = prev1.close < prev1.open && prev2.close < prev2.open;
                let strength = if prev_trend_down { 8 } else { 5 };

                return Some(("long".to_string(), strength, "pin_bar_bullish".to_string()));
            }
        }

        // 检测看跌 Pin Bar（上影线长）
        if body_ratio < PINBAR_BODY_RATIO && upper_wick_ratio > PINBAR_WICK_RATIO {
            if upper_wick > body * dec!(3) {
                let prev_trend_up = prev1.close > prev1.open && prev2.close > prev2.open;
                let strength = if prev_trend_up { 8 } else { 5 };

                return Some(("short".to_string(), strength, "pin_bar_bearish".to_string()));
            }
        }

        None
    }

    /// 风控检查（入场版本）
    fn risk_check_entry(
        &self,
        signal_type: &str,
        price: Decimal,
    ) -> (bool, Option<String>, std::collections::HashMap<String, bool>) {
        let mut check_items = std::collections::HashMap::new();
        let mut all_passed = true;
        let mut reject_reason = None;

        // 1. 余额检查
        let required_margin = price * POSITION_SIZE * dec!(0.1);
        if self.balance - required_margin < MIN_BALANCE {
            check_items.insert("min_balance".to_string(), false);
            all_passed = false;
            reject_reason = Some("MinBalanceExceeded".to_string());
        } else {
            check_items.insert("min_balance".to_string(), true);
        }

        // 2. 仓位限制检查（无仓位才能入场）
        if self.position_side.is_some() {
            check_items.insert("no_position".to_string(), false);
            all_passed = false;
            if reject_reason.is_none() {
                reject_reason = Some("PositionAlreadyOpen".to_string());
            }
        } else {
            check_items.insert("no_position".to_string(), true);
        }

        // 3. 日交易次数限制
        let today = Utc::now().date_naive();
        let last_date = self.last_trade_date.map(|d| d.date_naive());
        if last_date == Some(today) && self.daily_trade_count >= MAX_DAILY_TRADES {
            check_items.insert("daily_trade_limit".to_string(), false);
            all_passed = false;
            if reject_reason.is_none() {
                reject_reason = Some("DailyTradeLimitExceeded".to_string());
            }
        } else {
            check_items.insert("daily_trade_limit".to_string(), true);
        }

        // 4. 信号强度检查
        if signal_type.contains("pin_bar") {
            check_items.insert("signal_strength".to_string(), true);
        } else {
            check_items.insert("signal_strength".to_string(), false);
            all_passed = false;
        }

        (all_passed, reject_reason, check_items)
    }

    /// 平仓检查（出场版本 - 允许平仓）
    fn risk_check_exit(
        &self,
        signal_type: &str,
        _price: Decimal,
    ) -> (bool, Option<String>, std::collections::HashMap<String, bool>) {
        let mut check_items = std::collections::HashMap::new();

        // 有仓位才能平仓
        if self.position_side.is_none() {
            check_items.insert("has_position".to_string(), false);
            return (false, Some("NoPositionToClose".to_string()), check_items);
        }
        check_items.insert("has_position".to_string(), true);

        // 信号强度检查
        if signal_type.contains("pin_bar") {
            check_items.insert("signal_strength".to_string(), true);
        } else {
            check_items.insert("signal_strength".to_string(), false);
            return (false, Some("WeakSignal".to_string()), check_items);
        }

        (true, None, check_items)
    }
}

// ============================================================================
// 主程序
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("========================================");
    println!("  STRATEGY REPLAY WITH EVENT TRACKING");
    println!("  策略回放 + 事件追踪系统");
    println!("  Symbol: {} | 2025-10-09 ~ 2025-10-11", SYMBOL);
    println!("========================================");
    println!();

    // 2. 初始化心跳报告器
    tracing::info!("Initializing Heartbeat Reporter...");
    hb::init(hb::Config::default());
    println!("  Heartbeat Reporter: OK");
    println!();

    // 3. 初始化组件
    println!("[1] Initializing Components...");
    println!();

    // 3.1 策略事件追踪器
    let tracker = Arc::new(Mutex::new(StrategyEventTracker::new(INITIAL_BALANCE)));
    println!("  - StrategyEventTracker: OK (balance={})", INITIAL_BALANCE);

    // 3.2 撮合引擎
    let match_engine = Arc::new(SimpleMatchEngine::new());
    println!("  - SimpleMatchEngine: OK");

    // 3.3 订单拦截器（回放模式关闭心跳延迟上报）
    let gateway = MockApiGateway::with_default_config(INITIAL_BALANCE);
    let order_config = OrderInterceptorConfig {
        enable_heartbeat: false,
        latency_warning_ms: 100,
        latency_critical_ms: 500,
    };
    let _order_interceptor = OrderInterceptor::new(gateway, order_config);
    println!("  - OrderInterceptor: OK");

    // 3.5 策略
    let strategy = Arc::new(tokio::sync::Mutex::new(SimpleStrategy::new()));
    println!("  - SimpleStrategy (PinBar): OK");
    println!();

    // 4. 从 CSV 加载真实 K 线数据
    println!("[2] Loading K-line Data from CSV...");
    let klines = load_klines_from_csv(CSV_PATH, SYMBOL)
        .expect("Failed to load K-line data from CSV");
    let start_time = klines.first().map(|k| k.timestamp);
    let end_time = klines.last().map(|k| k.timestamp);
    println!("  Loaded {} K-lines ({} sub-ticks)",
             klines.len(), klines.len() * 60);
    println!();

    // 5. 创建 K 线流生成器
    let mut kline_stream = KlineStreamGenerator::new(
        SYMBOL.to_string(),
        Box::new(klines.into_iter()),
    );

    println!("[3] Starting Strategy Replay Loop...");
    println!();

    // 预加载所有子 K 线数据
    let all_sub_klines: Vec<_> = kline_stream.by_ref().collect();
    let replay_start_time = Utc::now();

    println!("  Total sub-ticks: {}", all_sub_klines.len());
    println!();

    // 转换为异步流
    let kline_stream = stream::iter(all_sub_klines);
    let mut kline_stream = Box::pin(kline_stream.fuse());
    let mut stream_exhausted = false;

    // 用于累积 OHLC 的临时变量
    let mut current_ohlc_open: Option<Decimal> = None;
    let mut current_ohlc_high: Decimal = Decimal::ZERO;
    let mut current_ohlc_low: Decimal = Decimal::MAX;
    let mut current_ohlc_close: Option<Decimal> = None;
    let mut current_ohlc_start_seq: u64 = 0;
    let mut current_ohlc_timestamp: Option<DateTime<Utc>> = None;

    // 进度打印计数器
    let mut last_progress_print = 0u64;

    loop {
        tokio::select! {
            kline_opt = kline_stream.next() => {
                match kline_opt {
                    Some(sub_kline) => {
                        // ===== 检测K线闭合 (sub_kline.is_last_in_kline 由生成器标记) =====
                        let is_last = sub_kline.is_last_in_kline;

                        if current_ohlc_start_seq == 0 {
                            // 初始化第一根K线
                            current_ohlc_open = Some(sub_kline.price);
                            current_ohlc_high = sub_kline.price;
                            current_ohlc_low = sub_kline.price;
                            current_ohlc_close = Some(sub_kline.price);
                            current_ohlc_start_seq = sub_kline.sequence_id;
                            current_ohlc_timestamp = Some(sub_kline.timestamp);
                        } else if is_last {
                            // K线闭合：先保存当前累积的OHLC，再处理信号，最后重置
                            let completed_kline = SimulatedKline {
                                open: current_ohlc_open.take().unwrap_or(sub_kline.price),
                                high: current_ohlc_high,
                                low: current_ohlc_low,
                                close: current_ohlc_close.take().unwrap_or(sub_kline.price),
                                timestamp: current_ohlc_timestamp.take().unwrap_or(sub_kline.timestamp),
                                is_closed: true,
                            };
                            let completed_ts = completed_kline.timestamp;

                            // 重置OHLC累加器，开始新的K线
                            current_ohlc_open = Some(sub_kline.price);
                            current_ohlc_high = sub_kline.price;
                            current_ohlc_low = sub_kline.price;
                            current_ohlc_close = Some(sub_kline.price);
                            current_ohlc_start_seq = sub_kline.sequence_id;
                            current_ohlc_timestamp = Some(sub_kline.timestamp);

                            // ===== 策略信号检测 =====
                            // 1. 更新价格历史
                            {
                                let mut sg = strategy.lock().await;
                                sg.update_price(&completed_kline);
                            }

                            // 2. 检测信号
                            let signal_opt = {
                                let sg = strategy.lock().await;
                                sg.detect_signal()
                            };

                            // 3. 心跳报到 - BS-001, CP-001
                            let tick_count = sub_kline.sequence_id / 60;
                            let token1 = hb::Token::with_data_timestamp(tick_count, completed_ts);
                            let latency1 = token1.data_latency_ms().unwrap_or(0);
                            hb::global().report_with_latency(
                                &token1, BS_001, "b_data_mock",
                                "kline_1m_stream", "mock_main.rs", latency1
                            ).await;
                            let token2 = hb::Token::with_data_timestamp(tick_count, completed_ts);
                            let latency2 = token2.data_latency_ms().unwrap_or(0);
                            hb::global().report_with_latency(
                                &token2, CP_001, "c_data_process",
                                "calc_indicators", "mock_main.rs", latency2
                            ).await;

                            if let Some((direction, strength, signal_type)) = signal_opt {
                                let price = completed_kline.close;
                                let current_pos_side = {
                                    let sg = strategy.lock().await;
                                    sg.position_side.clone()
                                };

                                // 记录信号生成
                                {
                                    let t = tracker.lock().unwrap();
                                    t.record_signal(
                                        completed_ts,
                                        &signal_type,
                                        &direction,
                                        price,
                                        strength,
                                        vec!["body_ratio<30%,wick_ratio>60%".to_string()],
                                    );
                                }

                                // ===== DT-001: CheckTable 报到 =====
                                let token3 = hb::Token::with_data_timestamp(tick_count, completed_ts);
                                hb::global().report_with_latency(
                                    &token3, DT_001, "d_checktable",
                                    "check_signals", "mock_main.rs", 0
                                ).await;

                                // ===== 判断是入场还是出场 =====
                                let is_exit = match (&current_pos_side, direction.as_str()) {
                                    // 有仓位且信号相反 → 平仓
                                    (Some(pos), sig) if
                                        (pos == "long" && sig == "short") ||
                                        (pos == "short" && sig == "long") => true,
                                    _ => false,
                                };

                                let (passed, reject_reason, check_items) = if is_exit {
                                    // 平仓检查
                                    let sg = strategy.lock().await;
                                    sg.risk_check_exit(&signal_type, price)
                                } else {
                                    // 入场检查
                                    let sg = strategy.lock().await;
                                    sg.risk_check_entry(&signal_type, price)
                                };

                                {
                                    let mut t = tracker.lock().unwrap();
                                    t.record_risk_check(
                                        completed_ts,
                                        &signal_type,
                                        passed,
                                        reject_reason.clone(),
                                        check_items,
                                    );
                                }

                                if passed {
                                    // ===== ER-001: RiskPreChecker 报到 =====
                                    let token4 = hb::Token::with_data_timestamp(tick_count, completed_ts);
                                    hb::global().report_with_latency(
                                        &token4, ER_001, "e_risk_monitor",
                                        "pre_check", "mock_main.rs", 0
                                    ).await;

                                    // 构造订单
                                    let order_id = {
                                        let mut sg = strategy.lock().await;
                                        sg.next_order_id()
                                    };
                                    let side = if direction == "long" { "buy" } else { "sell" };

                                    {
                                        let t = tracker.lock().unwrap();
                                        t.record_order(
                                            completed_ts,
                                            &order_id,
                                            SYMBOL,
                                            side,
                                            "limit",
                                            price,
                                            POSITION_SIZE,
                                        );
                                    }

                                    // 模拟成交
                                    let (filled_price, slippage, commission) =
                                        match_engine.simulate_fill(
                                            price,
                                            completed_kline.high,
                                            completed_kline.low,
                                            completed_kline.close,
                                            side,
                                        );

                                    {
                                        let t = tracker.lock().unwrap();
                                        t.record_filled(
                                            completed_ts,
                                            &order_id,
                                            filled_price,
                                            POSITION_SIZE,
                                            slippage,
                                            commission,
                                        );
                                    }

                                    // 更新策略状态
                                    {
                                        let mut sg = strategy.lock().await;
                                        sg.balance -= commission;
                                        if is_exit {
                                            // 平仓：清除仓位
                                            sg.position_side = None;
                                            println!(
                                                "  [CLOSE] {} @ {} | filled={} | slippage={} | commission={}",
                                                signal_type, price, filled_price, slippage, commission
                                            );
                                        } else {
                                            // 开仓
                                            sg.position_side = Some(direction.clone());
                                            sg.last_trade_date = Some(completed_ts);
                                            sg.daily_trade_count += 1;
                                            println!(
                                                "  [OPEN] {} @ {} | strength={} | filled={} | slippage={}",
                                                signal_type, price, strength, filled_price, slippage
                                            );
                                        }
                                    }

                                    // ===== FE-001 + EV-001 报到 =====
                                    let token5 = hb::Token::with_data_timestamp(tick_count, completed_ts);
                                    hb::global().report_with_latency(
                                        &token5, FE_001, "f_engine",
                                        "place_order", "mock_main.rs", 0
                                    ).await;
                                    let token6 = hb::Token::with_data_timestamp(tick_count, completed_ts);
                                    hb::global().report_with_latency(
                                        &token6, EV_001, "b_data_mock",
                                        "strategy_event_tracker", "mock_main.rs", 0
                                    ).await;
                                } else {
                                    println!(
                                        "  [BLOCKED] {} @ {} | reason={:?} | {}",
                                        signal_type, price, reject_reason,
                                        if is_exit { "(close)" } else { "(entry)" }
                                    );
                                }
                            } else {
                                // 无信号，仅报到心跳
                                let token3 = hb::Token::with_data_timestamp(tick_count, completed_ts);
                                hb::global().report_with_latency(
                                    &token3, DT_001, "d_checktable",
                                    "check_signals", "mock_main.rs", 0
                                ).await;
                                let token4 = hb::Token::with_data_timestamp(tick_count, completed_ts);
                                hb::global().report_with_latency(
                                    &token4, ER_001, "e_risk_monitor",
                                    "pre_check", "mock_main.rs", 0
                                ).await;
                                let token5 = hb::Token::with_data_timestamp(tick_count, completed_ts);
                                hb::global().report_with_latency(
                                    &token5, FE_001, "f_engine",
                                    "place_order", "mock_main.rs", 0
                                ).await;
                            }

                            // 记录PnL tick
                            {
                                let t = tracker.lock().unwrap();
                                t.tick(
                                    sub_kline.sequence_id / 60,
                                    sub_kline.timestamp,
                                    sub_kline.price,
                                );
                            }
                        } else {
                            // 继续累积当前K线
                            if sub_kline.price > current_ohlc_high {
                                current_ohlc_high = sub_kline.price;
                            }
                            if sub_kline.price < current_ohlc_low {
                                current_ohlc_low = sub_kline.price;
                            }
                            current_ohlc_close = Some(sub_kline.price);
                        }

                        // 进度打印（每1000个子tick）
                        let tick_count = sub_kline.sequence_id;
                        if tick_count - last_progress_print >= 6000 {
                            last_progress_print = tick_count;
                            let elapsed = (Utc::now() - replay_start_time).num_seconds();
                            let position = {
                                let t = tracker.lock().unwrap();
                                t.get_position()
                            };
                            let balance = {
                                let t = tracker.lock().unwrap();
                                t.get_balance()
                            };
                            println!(
                                "  [Progress] Tick#{:>6} | Price: {:.8} | Pos: {:?} | Balance: {} | Elapsed: {}s",
                                tick_count,
                                sub_kline.price,
                                position.side,
                                balance,
                                elapsed
                            );
                        }
                    }
                    None => {
                        if !stream_exhausted {
                            println!("\n[KLine Stream Exhausted]");
                            stream_exhausted = true;
                        }
                    }
                }
            }
        }

        if stream_exhausted {
            break;
        }
    }

    // 6. 生成最终回放报告
    println!();
    println!("========================================");
    println!("  STRATEGY REPLAY REPORT");
    println!("========================================");
    println!();

    let report: b_data_mock::ReplayReport = {
        let t = tracker.lock().unwrap();
        t.generate_report(
            SYMBOL,
            start_time.unwrap_or_else(Utc::now),
            end_time.unwrap_or_else(Utc::now),
        )
    };

    // 打印统计
    println!("  [Event Statistics]");
    println!("  {:<30} {:>10}", "Total Signals:", report.stats.total_signals);
    for (sig_type, count) in &report.stats.signals_by_type {
        println!("    - {}: {}", sig_type, count);
    }
    println!("  {:<30} {:>10}", "Total Risk Checks:", report.stats.total_risk_checks);
    println!("  {:<30} {:>10}", "  Passed:", report.stats.risk_checks_passed);
    println!("  {:<30} {:>10}", "  Rejected:", report.stats.risk_checks_rejected);

    if !report.stats.reject_reasons.is_empty() {
        println!("  [Rejection Reasons]");
        for (reason, count) in &report.stats.reject_reasons {
            println!("    - {}: {}", reason, count);
        }
    }

    println!();
    println!("  {:<30} {:>10}", "Total Orders:", report.stats.total_orders);
    println!("  {:<30} {:>10}", "  Filled:", report.stats.orders_filled);
    println!("  {:<30} {:>10}", "  Rejected:", report.stats.orders_rejected);
    println!("  {:<30} {:>10}", "Total Slippage:", report.stats.total_slippage);
    println!("  {:<30} {:>10}", "Total Commission:", report.stats.total_commission);
    println!();

    println!("  [PnL Summary]");
    println!("  {:<30} {:>10}", "Total Ticks:", report.total_ticks);

    if let Some(ref mp) = report.max_profit {
        println!("  {:<30} {:>10}", "Max Profit:", mp.pnl);
        println!("  {:<30} {:>10}", "  At tick:", mp.tick);
    }

    if let Some(ref md) = report.max_drawdown {
        println!("  {:<30} {:>10}", "Max Drawdown:", md.drawdown);
        println!("  {:<30} {:>10}", "  At tick:", md.tick);
    }

    // 关键统计
    let final_balance = {
        let t = tracker.lock().unwrap();
        t.get_balance()
    };
    let total_pnl = final_balance - INITIAL_BALANCE;
    let roi = if INITIAL_BALANCE > Decimal::ZERO {
        (total_pnl / INITIAL_BALANCE * dec!(100)).to_f64().unwrap_or(0.0)
    } else {
        0.0
    };

    println!();
    println!("  [Final Results]");
    println!("  {:<30} {:>15}", "Initial Balance:", INITIAL_BALANCE);
    println!("  {:<30} {:>15}", "Final Balance:", final_balance);
    println!("  {:<30} {:>15.4}", "Total PnL:", total_pnl);
    println!("  {:<30} {:>15.4}%", "ROI:", roi);

    // 成功率
    if report.stats.total_signals > 0 {
        let win_rate = report.stats.risk_checks_passed as f64
            / report.stats.total_risk_checks as f64 * 100.0;
        println!("  {:<30} {:>15.2}%", "Signal Pass Rate:", win_rate);
    }

    if report.stats.total_orders > 0 {
        let fill_rate = report.stats.orders_filled as f64
            / report.stats.total_orders as f64 * 100.0;
        println!("  {:<30} {:>15.2}%", "Order Fill Rate:", fill_rate);
    }

    // 7. 保存报告
    let report_path = "strategy_replay_report.json";
    let json = serde_json::to_string_pretty(&report)?;
    tokio::fs::write(report_path, &json).await?;
    println!();
    println!("  [Report saved to: {}]", report_path);

    // 8. 保存心跳报告
    let heartbeat_path = "heartbeat_report.json";
    if let Err(e) = hb::global().save_report(heartbeat_path).await {
        tracing::error!("Failed to save heartbeat report: {:?}", e);
    } else {
        println!("  [Heartbeat Report saved to: {}]", heartbeat_path);
    }

    println!();
    println!("========================================");
    println!("  REPLAY COMPLETE");
    println!("========================================");

    Ok(())
}

// ============================================================================
// 辅助函数：从 CSV 文件加载真实 K 线数据
// ============================================================================

fn load_klines_from_csv(path: &str, symbol: &str) -> Result<Vec<KLine>, Box<dyn std::error::Error>> {
    use std::io::BufRead;

    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    let mut lines = reader.lines();

    let mut klines = Vec::new();

    // 跳过表头
    if let Some(header) = lines.next() {
        tracing::info!("CSV header: {}", header?);
    }

    for line in lines {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() < 6 {
            continue;
        }

        // 解析时间戳 (毫秒)
        let ts_ms: i64 = parts[0].trim().parse()?;
        let timestamp = DateTime::from_timestamp(ts_ms / 1000, ((ts_ms % 1000) as u32) * 1_000_000)
            .ok_or_else(|| format!("Invalid timestamp: {}", ts_ms))?;

        // 解析 OHLCV
        let open: Decimal = parts[1].trim().parse()?;
        let high: Decimal = parts[2].trim().parse()?;
        let low: Decimal = parts[3].trim().parse()?;
        let close: Decimal = parts[4].trim().parse()?;
        let volume: Decimal = parts[5].trim().parse()?;

        klines.push(KLine {
            symbol: symbol.to_string(),
            period: Period::Minute(1),
            open,
            high,
            low,
            close,
            volume,
            timestamp,
            is_closed: true,
        });
    }

    tracing::info!("Loaded {} K-lines from {}", klines.len(), path);
    Ok(klines)
}

