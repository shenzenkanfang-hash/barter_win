//! Mock Trading System - 使用 d_checktable 现有 Pin 策略 + 事件追踪
//!
//! 集成现有 d_checktable/h_15m 策略模块:
//! - MinSignalGenerator: 7条件Pin模式 + 双通道信号生成
//! - PinStatusMachine:   仓位状态机 (Initial/LongInitial/LongFirstOpen/...)
//!
//! 策略回放完整追踪:
//! - 信号生成（含所有条件满足情况）
//! - 状态机状态转换
//! - 风控检查
//! - 订单构造 + 模拟成交
//! - 仓位变化
//! - PnL 曲线
//!
//! 运行: cargo run --bin mock-trading

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::MathematicalOps;
use rust_decimal_macros::dec;

use b_data_mock::{
    OrderInterceptor, OrderInterceptorConfig,
    MockApiGateway,
    KlineStreamGenerator, KLine,
    Period,
    StrategyEventTracker, SimpleMatchEngine,
};
use a_common::heartbeat as hb;
use d_checktable::h_15m::{
    MinSignalGenerator, PinStatusMachine,
};
use d_checktable::types::{MinSignalInput, MinSignalOutput, VolatilityTier};
use futures_util::{stream, StreamExt};

// ============================================================================
// 常量配置
// ============================================================================

const INITIAL_BALANCE: Decimal = dec!(10000);
const SYMBOL: &str = "HOTUSDT";
const CSV_PATH: &str = "D:/RusProject/barter-rs-main/data/HOTUSDT_1m_20251009_20251011.csv";

// 策略参数（与 d_checktable 保持一致）
const POSITION_SIZE: Decimal = dec!(1.0);  // 每笔开仓数量
const MIN_BALANCE: Decimal = dec!(1000);   // 最小余额
const MAX_DAILY_TRADES: u32 = 10;         // 每日最大交易次数

// 指标计算窗口
const ZSCORE_WINDOW: usize = 14;          // Z-score 窗口
const POS_WINDOW: usize = 60;              // 位置标准化窗口
const PRICE_HISTORY_WINDOW: usize = 200;   // 价格历史（留足够空间）

// 心跳测试点ID
const BS_001: &str = "BS-001";
const CP_001: &str = "CP-001";
const DT_001: &str = "DT-001";
const ER_001: &str = "ER-001";
const FE_001: &str = "FE-001";
const EV_001: &str = "EV-001";

// ============================================================================
// Pin 策略：集成 d_checktable 现有策略
// ============================================================================

/// Pin 策略状态追踪器
///
/// 集成:
/// - MinSignalGenerator: 7条件Pin信号生成
/// - PinStatusMachine:   仓位状态机
/// - MinSignalInput:      从OHLCV数据计算代理指标
struct PinStrategy {
    /// 7条件Pin信号生成器
    signal_gen: MinSignalGenerator,
    /// 仓位状态机
    status_machine: PinStatusMachine,
    /// 收盘价历史（用于计算指标）
    close_history: VecDeque<Decimal>,
    /// 高价历史（用于计算TR）
    high_history: VecDeque<Decimal>,
    /// 低价历史（用于计算TR）
    low_history: VecDeque<Decimal>,
    /// TR历史
    tr_history: VecDeque<Decimal>,
    /// 当前仓位方向
    position_side: Option<String>,
    /// 账户余额
    balance: Decimal,
    /// 当日交易次数
    daily_trade_count: u32,
    /// 上次交易日期
    last_trade_date: Option<DateTime<Utc>>,
    /// 订单ID计数器
    order_id_counter: u64,
    /// 最近一次的信号输出（用于交易日志展示）
    last_signal_output: Option<MinSignalOutput>,
    /// 当前波动率等级
    current_vol_tier: VolatilityTier,
    /// 当前的 MinSignalInput（用于日志展示）
    current_signal_input: MinSignalInput,
    /// 满足的条件计数（用于日志展示）
    last_pin_conditions_met: u8,
}

impl PinStrategy {
    fn new() -> Self {
        Self {
            signal_gen: MinSignalGenerator::new(),
            status_machine: PinStatusMachine::new(),
            close_history: VecDeque::with_capacity(PRICE_HISTORY_WINDOW),
            high_history: VecDeque::with_capacity(PRICE_HISTORY_WINDOW),
            low_history: VecDeque::with_capacity(PRICE_HISTORY_WINDOW),
            tr_history: VecDeque::with_capacity(PRICE_HISTORY_WINDOW),
            position_side: None,
            balance: INITIAL_BALANCE,
            daily_trade_count: 0,
            last_trade_date: None,
            order_id_counter: 0,
            last_signal_output: None,
            current_vol_tier: VolatilityTier::Low,
            current_signal_input: MinSignalInput::default(),
            last_pin_conditions_met: 0,
        }
    }

    /// 更新价格历史并计算指标
    fn update_price(&mut self, open: Decimal, high: Decimal, low: Decimal, close: Decimal) {
        // 更新收盘价历史
        self.close_history.push_back(close);
        self.high_history.push_back(high);
        self.low_history.push_back(low);

        if self.close_history.len() > PRICE_HISTORY_WINDOW {
            self.close_history.pop_front();
        }
        if self.high_history.len() > PRICE_HISTORY_WINDOW {
            self.high_history.pop_front();
        }
        if self.low_history.len() > PRICE_HISTORY_WINDOW {
            self.low_history.pop_front();
        }

        // 计算 True Range
        let prev_close = self.close_history.iter().rev().nth(1).copied().unwrap_or(close);
        let tr = (high - low).max((high - prev_close).abs()).max((low - prev_close).abs());
        self.tr_history.push_back(tr);
        if self.tr_history.len() > PRICE_HISTORY_WINDOW {
            self.tr_history.pop_front();
        }

        // 计算 MinSignalInput
        self.current_signal_input = self.compute_signal_input(open, close);
    }

    /// 计算 MinSignalInput（从OHLCV数据推导代理指标）
    fn compute_signal_input(&self, current_open: Decimal, current_close: Decimal) -> MinSignalInput {
        let mut input = MinSignalInput::new();

        // === 可从OHLCV直接计算的指标 ===

        // 1. Z-score 14 (从收盘价历史)
        if self.close_history.len() >= ZSCORE_WINDOW {
            let window: Vec<_> = self.close_history.iter().rev().take(ZSCORE_WINDOW).collect();
            if window.len() == ZSCORE_WINDOW {
                let mean: Decimal = window.iter().copied().sum::<Decimal>() / Decimal::from(ZSCORE_WINDOW);
                let variance: Decimal = window.iter()
                    .map(|p| (*p - mean) * (*p - mean))
                    .sum::<Decimal>() / Decimal::from(ZSCORE_WINDOW);
                let std = variance.sqrt().unwrap_or(Decimal::ZERO);
                if !std.is_zero() {
                    let latest = window[0];
                    input.zscore_14_1m = (latest - mean) / std;
                }
            }
        }

        // 2. Z-score 1h (代理：使用更长期的历史窗口模拟)
        if self.close_history.len() >= 60 {
            let window: Vec<_> = self.close_history.iter().rev().take(60).collect();
            if window.len() == 60 {
                let mean: Decimal = window.iter().copied().sum::<Decimal>() / dec!(60);
                let variance: Decimal = window.iter()
                    .map(|p| (*p - mean) * (*p - mean))
                    .sum::<Decimal>() / dec!(60);
                let std = variance.sqrt().unwrap_or(Decimal::ZERO);
                if !std.is_zero() {
                    input.zscore_1h_1m = (window[0] - mean) / std;
                }
            }
        }

        // 3. TR base 60min（滚动60期TR均值 / 收盘价的百分比）
        if self.tr_history.len() >= 60 && !current_close.is_zero() {
            let window: Vec<_> = self.tr_history.iter().rev().take(60).collect();
            let avg_tr: Decimal = window.iter().copied().sum::<Decimal>() / dec!(60);
            input.tr_base_60min = (avg_tr / current_close * dec!(100)).min(dec!(100)); // 百分比，限制在100%以内
        }

        // 4. TR ratio 60min/5h（使用TR的300期均值 / 60期均值）
        // 注意：价格历史最多保留200期，因此使用 min(200, 300) = 200 作为长期窗口
        if self.tr_history.len() >= 60 {
            let avg_60: Decimal = self.tr_history.iter().rev().take(60).copied().sum::<Decimal>() / dec!(60);
            let long_window = std::cmp::min(200, self.tr_history.len());
            let avg_long: Decimal = self.tr_history.iter().copied().sum::<Decimal>() / Decimal::from(u32::try_from(long_window).unwrap_or(200));
            if !avg_60.is_zero() {
                input.tr_ratio_60min_5h = avg_long / avg_60;
            }
        }

        // 5. TR ratio 10min/1h（10期TR均值 / 60期TR均值）
        if self.tr_history.len() >= 10 && self.tr_history.len() >= 60 {
            let avg_10: Decimal = self.tr_history.iter().rev().take(10).copied().sum::<Decimal>() / dec!(10);
            let avg_60: Decimal = self.tr_history.iter().rev().take(60).copied().sum::<Decimal>() / dec!(60);
            if !avg_60.is_zero() {
                input.tr_ratio_10min_1h = avg_10 / avg_60;
            }
        }

        // 6. Pos norm 60（价格当前位置：close在[low_N, high_N]区间内的百分比位置）
        if self.close_history.len() >= POS_WINDOW {
            let window: Vec<_> = self.close_history.iter().rev().take(POS_WINDOW).collect();
            let lo = *window.iter().min().unwrap();
            let hi = *window.iter().max().unwrap();
            let range = hi - lo;
            if !range.is_zero() {
                let pos = (current_close - lo) / range * dec!(100);
                input.pos_norm_60 = pos.max(dec!(0)).min(dec!(100));
            } else {
                input.pos_norm_60 = dec!(50);
            }
        }

        // 7. Price deviation（当前价格偏离均线百分比）
        if self.close_history.len() >= 20 {
            let window: Vec<_> = self.close_history.iter().rev().take(20).collect();
            let ma: Decimal = window.iter().copied().sum::<Decimal>() / dec!(20);
            if !ma.is_zero() {
                input.price_deviation = (current_close - ma) / ma * dec!(100); // 百分比
            }
        }

        // 8. Price deviation horizontal position（同6，但是用不同窗口）
        if self.close_history.len() >= POS_WINDOW {
            let window: Vec<_> = self.close_history.iter().rev().take(POS_WINDOW).collect();
            let lo = *window.iter().min().unwrap();
            let hi = *window.iter().max().unwrap();
            let range = hi - lo;
            if !range.is_zero() {
                input.price_deviation_horizontal_position =
                    ((current_close - lo) / range * dec!(100)).max(dec!(0)).min(dec!(100));
            } else {
                input.price_deviation_horizontal_position = dec!(50);
            }
        }

        // === 需要Pine Script / 复杂计算模拟的指标（使用合理的代理默认值）===

        // 9. Acceleration percentile 1h（价格加速度百分位 - 代理：使用价格变化率的滚动标准差）
        if self.close_history.len() >= 30 {
            let changes: Vec<Decimal> = self.close_history.iter()
                .rev()
                .take(30)
                .zip(self.close_history.iter().rev().skip(1).take(30))
                .map(|(c1, c2)| *c1 - *c2)
                .collect();
            let n = Decimal::from(u32::try_from(changes.len()).unwrap_or(30));
            if changes.len() >= 15 {
                let mean_change: Decimal = changes.iter().copied().sum::<Decimal>() / n;
                let variance: Decimal = changes.iter()
                    .map(|c| (*c - mean_change) * (*c - mean_change))
                    .sum::<Decimal>() / n;
                let std = variance.sqrt().unwrap_or(Decimal::ZERO);
                if !std.is_zero() && !mean_change.is_zero() {
                    let recent_change = changes[0];
                    let z = (recent_change - mean_change) / std;
                    // 映射到0-100百分位
                    let percentile = ((z + dec!(3)) / dec!(6) * dec!(100)).max(dec!(0)).min(dec!(100));
                    input.acc_percentile_1h = percentile;
                }
            }
        }

        // 10. Velocity percentile 1h（代理：使用更短期的动量指标）
        if self.close_history.len() >= 10 {
            let recent_return = if let (Some(c1), Some(c2)) = (self.close_history.get(0), self.close_history.get(9)) {
                if !c2.is_zero() {
                    (*c1 - *c2) / *c2 * dec!(100)
                } else {
                    Decimal::ZERO
                }
            } else {
                Decimal::ZERO
            };
            // 映射到0-100
            let vel = (recent_return.abs() * dec!(100)).min(dec!(100));
            input.velocity_percentile_1h = vel;
        }

        // 11. Pine BG color（背景颜色 - 从近3根K线判断趋势方向）
        if self.close_history.len() >= 3 {
            let c0 = self.close_history[0];
            let c1 = self.close_history[1];
            let c2 = self.close_history[2];
            // 连续上涨 = 纯绿， 连续下跌 = 纯红
            if c0 > c1 && c1 > c2 {
                input.pine_bg_color = "纯绿".to_string();
            } else if c0 < c1 && c1 < c2 {
                input.pine_bg_color = "纯红".to_string();
            } else {
                input.pine_bg_color = "混色".to_string();
            }
        }

        // 12. Pine Bar color（当前K线颜色）
        if current_close > current_open {
            input.pine_bar_color = "纯绿".to_string();
        } else if current_close < current_open {
            input.pine_bar_color = "纯红".to_string();
        } else {
            input.pine_bar_color = "混色".to_string();
        }

        input
    }

    /// 计算波动率等级（从TR历史推导）
    fn compute_volatility_tier(&self, current_close: Decimal) -> VolatilityTier {
        if self.tr_history.len() >= 15 && !current_close.is_zero() {
            let avg_tr_15: Decimal = self.tr_history.iter().rev().take(15).copied().sum::<Decimal>() / dec!(15);
            let vol_ratio = avg_tr_15 / current_close * dec!(100);
            if vol_ratio > dec!(1.5) {
                return VolatilityTier::High;
            } else if vol_ratio > dec!(0.8) {
                return VolatilityTier::Medium;
            }
        }
        VolatilityTier::Low
    }

    /// 生成信号
    fn generate_signal(&mut self) -> Option<(String, MinSignalOutput, u8)> {
        let input = &self.current_signal_input;
        let vol_tier = self.current_vol_tier;

        // 7条件Pin计数（从 signal.rs 提取的逻辑）
        let pin_conditions_met = self.count_pin_conditions(input);

        // 使用 d_checktable 的 MinSignalGenerator 生成信号
        // 回测模式：无日线方向，使用 None
        let output = self.signal_gen.generate(input, &vol_tier, None);

        self.last_signal_output = Some(output.clone());
        self.last_pin_conditions_met = pin_conditions_met;

        // 判断方向
        if output.long_entry {
            Some(("long".to_string(), output, pin_conditions_met))
        } else if output.short_entry {
            Some(("short".to_string(), output, pin_conditions_met))
        } else {
            None
        }
    }

    /// 统计满足的Pin条件数量（暴露给交易日志）
    fn count_pin_conditions(&self, input: &MinSignalInput) -> u8 {
        let mut count: u8 = 0;

        if input.zscore_14_1m.abs() > dec!(2) || input.zscore_1h_1m.abs() > dec!(2) { count += 1; }
        if input.tr_ratio_60min_5h > dec!(1) || input.tr_ratio_10min_1h > dec!(1) { count += 1; }
        if input.pos_norm_60 > dec!(80) || input.pos_norm_60 < dec!(20) { count += 1; }
        if input.acc_percentile_1h > dec!(90) { count += 1; }
        if input.pine_bg_color == "纯绿" || input.pine_bg_color == "纯红" { count += 1; }
        if input.pine_bar_color == "纯绿" || input.pine_bar_color == "纯红" { count += 1; }
        if input.price_deviation_horizontal_position.abs() == dec!(100) { count += 1; }

        count
    }

    /// 生成订单ID
    fn next_order_id(&mut self) -> String {
        self.order_id_counter += 1;
        format!("ORD_{:06}", self.order_id_counter)
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

        // 2. 仓位限制（已有仓位不允许同向开仓）
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

        // 4. Pin条件强度（至少满足4个条件）
        if signal_type.contains("long_entry") || signal_type.contains("short_entry") {
            check_items.insert("pin_condition".to_string(), true);
        } else {
            check_items.insert("pin_condition".to_string(), false);
            all_passed = false;
        }

        (all_passed, reject_reason, check_items)
    }

    /// 平仓检查（出场版本）
    fn risk_check_exit(
        &self,
        _signal_type: &str,
        _price: Decimal,
    ) -> (bool, Option<String>, std::collections::HashMap<String, bool>) {
        let mut check_items = std::collections::HashMap::new();

        // 有仓位才能平仓
        if self.position_side.is_none() {
            check_items.insert("has_position".to_string(), false);
            return (false, Some("NoPositionToClose".to_string()), check_items);
        }
        check_items.insert("has_position".to_string(), true);

        (true, None, check_items)
    }

    /// 获取当前状态描述（用于交易日志）
    fn get_state_summary(&self) -> String {
        let status = self.status_machine.current_status();
        let vol = match self.current_vol_tier {
            VolatilityTier::High => "High",
            VolatilityTier::Medium => "Medium",
            VolatilityTier::Low => "Low",
        };
        let pin = self.last_pin_conditions_met;
        let input = &self.current_signal_input;

        let tr = (&input.tr_base_60min * dec!(100)).round_dp(2);
        let zscore = input.zscore_14_1m.round_dp(3);
        let pos = input.pos_norm_60.round_dp(1);
        let bg = &input.pine_bg_color;
        let bar = &input.pine_bar_color;
        let pdh = input.price_deviation_horizontal_position.round_dp(1);

        format!(
            "PinStatus={:?} | VolTier={} | PinCond={}/7 | TR={}% | Zs={} | Pos={} | BG={} | Bar={} | PDH={}",
            status, vol, pin, tr, zscore, pos, bg, bar, pdh
        )
    }
}

// ============================================================================
// 交易日志条目（带策略状态）
// ============================================================================

#[derive(Debug, Clone, serde::Serialize)]
struct TradeLogEntry {
    timestamp: String,
    action: String,       // OPEN / CLOSE
    direction: String,    // long / short
    signal_type: String,  // long_entry / short_entry / ...
    price: Decimal,
    filled_price: Decimal,
    qty: Decimal,
    slippage: Decimal,
    commission: Decimal,
    pnl: Option<Decimal>,
    // === 策略运行状态（核心需求）===
    pin_status: String,
    volatility_tier: String,
    pin_conditions_met: u8,
    // 7个条件各自是否满足
    cond1_zscore: bool,
    cond2_tr_ratio: bool,
    cond3_pos_norm: bool,
    cond4_acc_pct: bool,
    cond5_bg_color: bool,
    cond6_bar_color: bool,
    cond7_pdh: bool,
    // 信号输入详情
    tr_base_60min: String,
    zscore_14_1m: String,
    zscore_1h_1m: String,
    pos_norm_60: String,
    acc_percentile_1h: String,
    pine_bg_color: String,
    pine_bar_color: String,
    price_deviation_horizontal_position: String,
}

impl TradeLogEntry {
    fn new(
        timestamp: DateTime<Utc>,
        action: &str,
        direction: &str,
        signal_type: &str,
        price: Decimal,
        filled_price: Decimal,
        qty: Decimal,
        slippage: Decimal,
        commission: Decimal,
        pnl: Option<Decimal>,
        strategy: &PinStrategy,
    ) -> Self {
        let input = &strategy.current_signal_input;
        let pin = strategy.last_pin_conditions_met;

        Self {
            timestamp: timestamp.to_rfc3339(),
            action: action.to_string(),
            direction: direction.to_string(),
            signal_type: signal_type.to_string(),
            price,
            filled_price,
            qty,
            slippage,
            commission,
            pnl,
            pin_status: format!("{:?}", strategy.status_machine.current_status()),
            volatility_tier: match strategy.current_vol_tier {
                VolatilityTier::High => "High".to_string(),
                VolatilityTier::Medium => "Medium".to_string(),
                VolatilityTier::Low => "Low".to_string(),
            },
            pin_conditions_met: pin,
            cond1_zscore: input.zscore_14_1m.abs() > dec!(2) || input.zscore_1h_1m.abs() > dec!(2),
            cond2_tr_ratio: input.tr_ratio_60min_5h > dec!(1) || input.tr_ratio_10min_1h > dec!(1),
            cond3_pos_norm: input.pos_norm_60 > dec!(80) || input.pos_norm_60 < dec!(20),
            cond4_acc_pct: input.acc_percentile_1h > dec!(90),
            cond5_bg_color: input.pine_bg_color == "纯绿" || input.pine_bg_color == "纯红",
            cond6_bar_color: input.pine_bar_color == "纯绿" || input.pine_bar_color == "纯红",
            cond7_pdh: input.price_deviation_horizontal_position.abs() == dec!(100),
            tr_base_60min: format!("{:.2}%", input.tr_base_60min * dec!(100)),
            zscore_14_1m: format!("{:.3}", input.zscore_14_1m),
            zscore_1h_1m: format!("{:.3}", input.zscore_1h_1m),
            pos_norm_60: format!("{:.1}", input.pos_norm_60),
            acc_percentile_1h: format!("{:.1}", input.acc_percentile_1h),
            pine_bg_color: input.pine_bg_color.clone(),
            pine_bar_color: input.pine_bar_color.clone(),
            price_deviation_horizontal_position: format!("{:.1}", input.price_deviation_horizontal_position),
        }
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
    println!("  STRATEGY REPLAY WITH EXISTING PIN STRATEGY");
    println!("  策略回放 - d_checktable/h_15m Pin策略");
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

    // 3.4 Pin策略（集成 d_checktable）
    let strategy = Arc::new(tokio::sync::Mutex::new(PinStrategy::new()));
    println!("  - PinStrategy (d_checktable::h_15m): OK");
    println!("    - MinSignalGenerator: 7条件Pin模式");
    println!("    - PinStatusMachine: 仓位状态机");
    println!("    - ProxyIndicators: 从OHLCV计算");
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
    let mut current_ohlc_timestamp: Option<DateTime<Utc>> = None;

    // 交易日志
    let trade_log: Arc<Mutex<Vec<TradeLogEntry>>> = Arc::new(Mutex::new(Vec::new()));

    // 进度打印计数器
    let mut last_progress_print = 0u64;

    // 已实现PnL累计（用于计算每笔交易的盈亏）
    let mut realized_pnl: Decimal = Decimal::ZERO;

    // 入场价格记录（用于计算每笔交易的PnL）
    let entry_price: Arc<Mutex<Option<Decimal>>> = Arc::new(Mutex::new(None));
    let entry_direction: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    loop {
        tokio::select! {
            kline_opt = kline_stream.next() => {
                match kline_opt {
                    Some(sub_kline) => {
                        let is_last = sub_kline.is_last_in_kline;

                        if current_ohlc_open.is_none() {
                            // 初始化第一根K线
                            current_ohlc_open = Some(sub_kline.price);
                            current_ohlc_high = sub_kline.price;
                            current_ohlc_low = sub_kline.price;
                            current_ohlc_close = Some(sub_kline.price);
                            current_ohlc_timestamp = Some(sub_kline.timestamp);
                        } else if is_last {
                            // ===== K线闭合 =====
                            let completed_kline = SimulatedKline {
                                open: current_ohlc_open.take().unwrap_or(sub_kline.price),
                                high: current_ohlc_high,
                                low: current_ohlc_low,
                                close: current_ohlc_close.take().unwrap_or(sub_kline.price),
                                timestamp: current_ohlc_timestamp.take().unwrap_or(sub_kline.timestamp),
                            };
                            let completed_ts = completed_kline.timestamp;

                            // 重置OHLC累加器
                            current_ohlc_open = Some(sub_kline.price);
                            current_ohlc_high = sub_kline.price;
                            current_ohlc_low = sub_kline.price;
                            current_ohlc_close = Some(sub_kline.price);
                            current_ohlc_timestamp = Some(sub_kline.timestamp);

                            // ===== 策略指标更新 =====
                            {
                                let mut sg = strategy.lock().await;
                                sg.update_price(
                                    completed_kline.open,
                                    completed_kline.high,
                                    completed_kline.low,
                                    completed_kline.close,
                                );
                                sg.current_vol_tier = sg.compute_volatility_tier(completed_kline.close);
                            }

                            // ===== 心跳报到 - BS-001, CP-001 =====
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

                            // ===== 信号生成 =====
                            let signal_opt = {
                                let mut sg = strategy.lock().await;
                                sg.generate_signal()
                            };

                            // ===== DT-001: CheckTable 报到 =====
                            let token3 = hb::Token::with_data_timestamp(tick_count, completed_ts);
                            hb::global().report_with_latency(
                                &token3, DT_001, "d_checktable",
                                "check_signals", "mock_main.rs", 0
                            ).await;

                            if let Some((direction, _signal_output, pin_conditions)) = signal_opt {
                                let price = completed_kline.close;
                                let current_pos_side = {
                                    let sg = strategy.lock().await;
                                    sg.position_side.clone()
                                };

                                // === 信号类型 ===
                                let signal_type = if direction == "long" { "long_entry" } else { "short_entry" };

                                // === 记录信号生成 ===
                                let conditions_met = vec![
                                    format!("pin_conditions={}", pin_conditions),
                                    format!("vol_tier={:?}", {
                                        let sg = strategy.lock().await;
                                        sg.current_vol_tier
                                    }),
                                ];
                                {
                                    let t = tracker.lock().unwrap();
                                    t.record_signal(
                                        completed_ts,
                                        signal_type,
                                        &direction,
                                        price,
                                        pin_conditions,
                                        conditions_met,
                                    );
                                }

                                // === 判断是入场还是出场 ===
                                let is_exit = match (&current_pos_side, direction.as_str()) {
                                    (Some(pos), sig) if
                                        (pos == "long" && sig == "short") ||
                                        (pos == "short" && sig == "long") => true,
                                    _ => false,
                                };

                                let (passed, reject_reason, check_items) = if is_exit {
                                    let sg = strategy.lock().await;
                                    sg.risk_check_exit(signal_type, price)
                                } else {
                                    let sg = strategy.lock().await;
                                    sg.risk_check_entry(signal_type, price)
                                };

                                // === 记录风控检查 ===
                                {
                                    let t = tracker.lock().unwrap();
                                    t.record_risk_check(
                                        completed_ts,
                                        signal_type,
                                        passed,
                                        reject_reason.clone(),
                                        check_items,
                                    );
                                }

                                if passed {
                                    // === ER-001: RiskPreChecker 报到 ===
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

                                    // 计算本笔PnL（平仓时）
                                    let trade_pnl = if is_exit {
                                        let ep = entry_price.lock().unwrap();
                                        let ed = entry_direction.lock().unwrap();
                                        if let (Some(entry_p), Some(entry_d)) = (*ep, ed.as_deref()) {
                                            let pnl = if entry_d == "long" {
                                                filled_price - entry_p
                                            } else {
                                                entry_p - filled_price
                                            };
                                            Some(pnl * POSITION_SIZE - commission)
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    };

                                    // 累计已实现PnL
                                    if let Some(p) = trade_pnl {
                                        realized_pnl += p;
                                    }

                                    // === 记录成交 ===
                                    {
                                        let mut t = tracker.lock().unwrap();
                                        t.record_filled(
                                            completed_ts,
                                            &order_id,
                                            filled_price,
                                            POSITION_SIZE,
                                            slippage,
                                            commission,
                                        );
                                    }

                                    // === 更新策略状态 ===
                                    let state_summary = {
                                        let mut sg = strategy.lock().await;
                                        sg.balance -= commission;

                                        if is_exit {
                                            sg.position_side = None;
                                            *entry_price.lock().unwrap() = None;
                                            *entry_direction.lock().unwrap() = None;
                                        } else {
                                            sg.position_side = Some(direction.clone());
                                            sg.last_trade_date = Some(completed_ts);
                                            sg.daily_trade_count += 1;
                                            *entry_price.lock().unwrap() = Some(filled_price);
                                            *entry_direction.lock().unwrap() = Some(direction.clone());
                                        }

                                        sg.get_state_summary()
                                    };

                                    // === 构造交易日志条目（带策略状态）===
                                    let log_entry = {
                                        let sg = strategy.lock().await;
                                        TradeLogEntry::new(
                                            completed_ts,
                                            if is_exit { "CLOSE" } else { "OPEN" },
                                            &direction,
                                            signal_type,
                                            price,
                                            filled_price,
                                            POSITION_SIZE,
                                            slippage,
                                            commission,
                                            trade_pnl,
                                            &sg,
                                        )
                                    };
                                    trade_log.lock().unwrap().push(log_entry.clone());

                                    // === 控制台输出（含策略状态）===
                                    if is_exit {
                                        let pnl_str = trade_pnl.map(|p| format!("PnL={:.6}", p)).unwrap_or_default();
                                        println!(
                                            "  [CLOSE] {} @ {} | filled={} | {} | {}",
                                            signal_type, price, filled_price, state_summary, pnl_str
                                        );
                                    } else {
                                        println!(
                                            "  [OPEN] {} @ {} | filled={} | {}",
                                            signal_type, price, filled_price, state_summary
                                        );
                                    }

                                    // === FE-001 + EV-001 报到 ===
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
                                    let state_summary = {
                                        let sg = strategy.lock().await;
                                        sg.get_state_summary()
                                    };
                                    println!(
                                        "  [BLOCKED] {} @ {} | reason={:?} | {} | {}",
                                        signal_type, price, reject_reason,
                                        if is_exit { "(close)" } else { "(entry)" },
                                        state_summary
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

                            // === 记录PnL tick ===
                            {
                                let mut t = tracker.lock().unwrap();
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

                        // 进度打印（每6000个子tick = 100根K线）
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
                            let state_summary = {
                                let sg = strategy.lock().await;
                                sg.get_state_summary()
                            };
                            println!(
                                "  [Progress] Tick#{:>6} | Price: {:.8} | Pos: {:?} | Balance: {:.4} | {} | {}s",
                                tick_count,
                                sub_kline.price,
                                position.side,
                                balance,
                                state_summary,
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
    println!("  d_checktable/h_15m Pin Strategy");
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
    println!("  {:<35} {:>10}", "Total Signals:", report.stats.total_signals);
    for (sig_type, count) in &report.stats.signals_by_type {
        println!("    - {}: {}", sig_type, count);
    }
    println!("  {:<35} {:>10}", "Total Risk Checks:", report.stats.total_risk_checks);
    println!("  {:<35} {:>10}", "  Passed:", report.stats.risk_checks_passed);
    println!("  {:<35} {:>10}", "  Rejected:", report.stats.risk_checks_rejected);

    if !report.stats.reject_reasons.is_empty() {
        println!("  [Rejection Reasons]");
        for (reason, count) in &report.stats.reject_reasons {
            println!("    - {}: {}", reason, count);
        }
    }

    println!();
    println!("  {:<35} {:>10}", "Total Orders:", report.stats.total_orders);
    println!("  {:<35} {:>10}", "  Filled:", report.stats.orders_filled);
    println!("  {:<35} {:>10}", "  Rejected:", report.stats.orders_rejected);
    println!("  {:<35} {:>10}", "Total Slippage:", report.stats.total_slippage);
    println!("  {:<35} {:>10}", "Total Commission:", report.stats.total_commission);
    println!();

    println!("  [PnL Summary]");
    println!("  {:<35} {:>10}", "Total Ticks:", report.total_ticks);

    if let Some(ref mp) = report.max_profit {
        println!("  {:<35} {:>10}", "Max Profit:", mp.pnl);
        println!("  {:<35} {:>10}", "  At tick:", mp.tick);
    }

    if let Some(ref md) = report.max_drawdown {
        println!("  {:<35} {:>10}", "Max Drawdown:", md.drawdown);
        println!("  {:<35} {:>10}", "  At tick:", md.tick);
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
    println!("  {:<35} {:>15}", "Initial Balance:", INITIAL_BALANCE);
    println!("  {:<35} {:>15}", "Final Balance:", final_balance);
    println!("  {:<35} {:>15.4}", "Total PnL:", total_pnl);
    println!("  {:<35} {:>15.4}%", "ROI:", roi);

    // 成功率
    if report.stats.total_signals > 0 {
        let win_rate = report.stats.risk_checks_passed as f64
            / report.stats.total_risk_checks as f64 * 100.0;
        println!("  {:<35} {:>15.2}%", "Signal Pass Rate:", win_rate);
    }

    if report.stats.total_orders > 0 {
        let fill_rate = report.stats.orders_filled as f64
            / report.stats.total_orders as f64 * 100.0;
        println!("  {:<35} {:>15.2}%", "Order Fill Rate:", fill_rate);
    }

    // 7. 保存策略回放报告
    let report_path = "strategy_replay_report.json";
    let json = serde_json::to_string_pretty(&report)?;
    tokio::fs::write(report_path, &json).await?;
    println!();
    println!("  [Report saved to: {}]", report_path);

    // 8. 保存交易日志（带策略状态）
    let trade_log_path = "pin_strategy_trade_log.json";
    {
        let log = trade_log.lock().unwrap();
        let json = serde_json::to_string_pretty(&*log)?;
        tokio::fs::write(trade_log_path, &json).await?;
        println!("  [Trade Log (with strategy state) saved to: {}]", trade_log_path);
        println!("  Total trade entries: {}", log.len());

        // 统计日志中各状态分布
        let mut status_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for entry in log.iter() {
            *status_counts.entry(entry.pin_status.clone()).or_insert(0) += 1;
        }
        println!();
        println!("  [PinStatus Distribution]");
        for (status, count) in status_counts.iter() {
            println!("    {:?}: {} trades", status, count);
        }
    }

    // 9. 保存心跳报告
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

// ============================================================================
// 内部类型
// ============================================================================

#[derive(Debug, Clone)]
struct SimulatedKline {
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub timestamp: DateTime<Utc>,
}
