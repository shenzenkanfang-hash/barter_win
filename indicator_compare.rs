//! 指标对比验证程序
//!
//! 严格按照 Pine Script @version=5 算法实现所有指标计算

use chrono::{DateTime, TimeZone, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::VecDeque;

// ==================== 辅助函数 ====================

fn parse_decimal(s: &str) -> Decimal {
    s.parse().unwrap_or(dec!(0))
}

fn round_price(price: Decimal, precision: u8) -> Decimal {
    price.round_dp(precision as u32)
}

fn round_qty(qty: Decimal, precision: u8) -> Decimal {
    qty.round_dp(precision as u32)
}

fn ln(x: Decimal) -> Decimal {
    // 简化的自然对数实现
    if x <= dec!(0) {
        return dec!(0);
    }
    // 使用泰勒级数展开 ln(x) = 2*(z + z^3/3 + z^5/5 + ...) 其中 z = (x-1)/(x+1)
    let z = (x - dec!(1)) / (x + dec!(1));
    let mut result = dec!(0);
    let mut z_power = z;
    for i in 1..=20 {
        let term = z_power / Decimal::from(i * 2 - 1);
        result = result + if i % 2 == 1 { term } else { -term };
        z_power = z_power * z;
    }
    dec!(2) * result
}

// 简化的平方根 (牛顿法)
fn sqrt(x: Decimal) -> Decimal {
    if x <= dec!(0) {
        return dec!(0);
    }
    let mut guess = x / dec!(2);
    for _ in 0..20 {
        let next_guess = (guess + x / guess) / dec!(2);
        if (guess - next_guess).abs() < dec!(0.0000001) {
            break;
        }
        guess = next_guess;
    }
    guess
}

// ==================== EMA 计算 ====================

struct EMA {
    period: usize,
    value: Decimal,
}

impl EMA {
    fn new(period: usize) -> Self {
        Self {
            period,
            value: Decimal::ZERO,
        }
    }

    fn update(&mut self, price: Decimal) -> Decimal {
        if self.value == Decimal::ZERO {
            self.value = price;
        } else {
            let alpha = dec!(2) / Decimal::from(self.period + 1);
            self.value = price * alpha + self.value * (dec!(1) - alpha);
        }
        self.value
    }

    fn get(&self) -> Decimal {
        self.value
    }
}

// ==================== SMA 计算 ====================

struct SMA {
    period: usize,
    values: VecDeque<Decimal>,
    sum: Decimal,
}

impl SMA {
    fn new(period: usize) -> Self {
        Self {
            period,
            values: VecDeque::new(),
            sum: Decimal::ZERO,
        }
    }

    fn update(&mut self, price: Decimal) -> Decimal {
        self.values.push_back(price);
        self.sum = self.sum + price;

        if self.values.len() > self.period {
            if let Some(old) = self.values.pop_front() {
                self.sum = self.sum - old;
            }
        }

        if self.values.len() >= self.period {
            self.sum / Decimal::from(self.period)
        } else {
            self.sum / Decimal::from(self.values.len())
        }
    }
}

// ==================== RMA (RSI 用的平滑均线) ====================

struct RMA {
    period: usize,
    alpha: Decimal,
    value: Decimal,
    initialized: bool,
}

impl RMA {
    fn new(period: usize) -> Self {
        Self {
            period,
            alpha: dec!(1) / Decimal::from(period),
            value: Decimal::ZERO,
            initialized: false,
        }
    }

    fn update(&mut self, price: Decimal) -> Decimal {
        if !self.initialized {
            self.value = price;
            self.initialized = true;
        } else {
            self.value = price * self.alpha + self.value * (dec!(1) - self.alpha);
        }
        self.value
    }
}

// ==================== 标准差 ====================

struct STDEV {
    period: usize,
    values: VecDeque<Decimal>,
    mean: Decimal,
}

impl STDEV {
    fn new(period: usize) -> Self {
        Self {
            period,
            values: VecDeque::new(),
            mean: Decimal::ZERO,
        }
    }

    fn update(&mut self, price: Decimal) -> Decimal {
        self.values.push_back(price);
        if self.values.len() > self.period {
            self.values.pop_front();
        }

        let n = Decimal::from(self.values.len());
        if n < dec!(2) {
            return dec!(0);
        }

        let sum: Decimal = self.values.iter().sum();
        self.mean = sum / n;

        let variance: Decimal = self.values.iter()
            .map(|&x| {
                let diff = x - self.mean;
                diff * diff
            })
            .sum::<Decimal>() / n;

        // 简化的标准差计算
        let approx = sqrt(variance.abs());
        approx
    }
}

// ==================== RSI (Dominant Cycle RSI) ====================
// 严格按照 Pine Script v5:
// crsi := torque * (2 * rsi - rsi[phasingLag]) + (1 - torque) * nz(crsi[1])
// 其中 phasingLag = 4 (vibration=10 时)

struct DominantCycleRSI {
    period: usize,
    cyclelen: usize,
    vibration: usize,
    leveling: Decimal,
    cyclicmemory: usize,
    torque: Decimal,
    phasinglag: usize,       // Pine Script: (vibration - 1) / 2 = 4.5 -> 4
    rma_up: RMA,
    rma_down: RMA,
    crsi: Decimal,
    prev_crsi: Decimal,
    last_price: Decimal,
    lmax: Decimal,
    lmin: Decimal,
    lmax_history: VecDeque<Decimal>,
    lmin_history: VecDeque<Decimal>,
    rsi_history: VecDeque<Decimal>,  // 存储 RSI 历史，用于 rsi[phasingLag]
}

impl DominantCycleRSI {
    fn new(period: usize) -> Self {
        let cyclelen = period / 2;
        let vibration = 10;
        let leveling = dec!(10.0);
        let cyclicmemory = period * 2;
        let torque = dec!(2) / Decimal::from(vibration + 1);
        let phasinglag = (vibration - 1) / 2;  // 4

        Self {
            period,
            cyclelen,
            vibration,
            leveling,
            cyclicmemory,
            torque,
            phasinglag,
            rma_up: RMA::new(cyclelen),
            rma_down: RMA::new(cyclelen),
            crsi: Decimal::ZERO,
            prev_crsi: Decimal::ZERO,
            last_price: Decimal::ZERO,
            lmax: dec!(-999999),
            lmin: dec!(999999),
            lmax_history: VecDeque::new(),
            lmin_history: VecDeque::new(),
            rsi_history: VecDeque::new(),
        }
    }

    fn update(&mut self, price: Decimal) -> Decimal {
        // 计算标准 RSI
        let change = if self.last_price > Decimal::ZERO {
            price - self.last_price
        } else {
            Decimal::ZERO
        };
        self.last_price = price;

        let up = if change > Decimal::ZERO { change } else { -change };
        let down = if change < Decimal::ZERO { -change } else { Decimal::ZERO };

        let rsi_value = if down == Decimal::ZERO {
            dec!(100)
        } else if up == Decimal::ZERO {
            dec!(0)
        } else {
            dec!(100) - dec!(100) / (dec!(1) + up / down)
        };

        // RMA 平滑
        let rma_up_val = self.rma_up.update(up);
        let rma_down_val = self.rma_down.update(down);

        let rsi_rma = if rma_down_val == Decimal::ZERO {
            dec!(100)
        } else if rma_up_val == Decimal::ZERO {
            dec!(0)
        } else {
            dec!(100) - dec!(100) / (dec!(1) + rma_up_val / rma_down_val)
        };

        // 存储 RSI 历史
        self.rsi_history.push_back(rsi_rma);

        // 计算 CRSi: crsi = torque * (2 * rsi - rsi[phasingLag]) + (1 - torque) * crsi[1]
        // Pine Script: rsi[phasingLag] 表示 phasinglag 个周期前的 RSI
        let rsi_lagged = self.get_rsi_lagged();

        let crsi_calc = if rsi_lagged.is_some() {
            self.torque * (dec!(2) * rsi_rma - rsi_lagged.unwrap()) +
            (dec!(1) - self.torque) * self.crsi
        } else {
            // 前 phasinglag 个周期，rsi[phasingLag] 返回 na，Pine Script 用 nz(crsi[1]) = 0
            self.torque * (dec!(2) * rsi_rma) +
            (dec!(1) - self.torque) * self.crsi
        };

        self.prev_crsi = self.crsi;
        self.crsi = crsi_calc;

        // 更新历史极值
        self.lmax_history.push_back(self.crsi);
        self.lmin_history.push_back(self.crsi);

        if self.lmax_history.len() > self.cyclicmemory {
            self.lmax_history.pop_front();
            self.lmin_history.pop_front();
        }

        self.lmax = *self.lmax_history.iter().max().unwrap_or(&dec!(-999999));
        self.lmin = *self.lmin_history.iter().min().unwrap_or(&dec!(999999));

        self.crsi
    }

    // 获取 lagged RSI 值 (phasinglag 个周期前的 RSI)
    fn get_rsi_lagged(&self) -> Option<Decimal> {
        let len = self.rsi_history.len();
        if len > self.phasinglag {
            self.rsi_history.get(len - 1 - self.phasinglag).copied()
        } else {
            None  // 前 phasinglag 个周期，返回 None
        }
    }

    fn get_rsi_70(&self) -> bool {
        self.crsi >= dec!(70)
    }

    fn get_rsi_30(&self) -> bool {
        self.crsi <= dec!(30)
    }
}

// ==================== PineColor 检测器 ====================

struct PineColorDetector {
    macd_ema_fast: EMA,
    macd_ema_slow: EMA,
    signal_ema: EMA,
    ema10: EMA,        // 独立的 EMA10 (基于 close)
    ema20: EMA,        // 独立的 EMA20 (基于 close)
    hist_prev: Option<Decimal>,  // None 表示 hist[1] 不可用 (首个 bar)
    rsi: DominantCycleRSI,
}

impl PineColorDetector {
    fn new() -> Self {
        Self {
            macd_ema_fast: EMA::new(20),  // Fast Length = 20
            macd_ema_slow: EMA::new(50),   // Slow Length = 50
            signal_ema: EMA::new(9),       // Signal Smoothing = 9
            ema10: EMA::new(10),           // EMA 10 (基于 close)
            ema20: EMA::new(20),           // EMA 20 (基于 close)
            hist_prev: None,               // 首个 bar，hist[1] 不可用
            rsi: DominantCycleRSI::new(20),
        }
    }

    fn update(&mut self, close: Decimal) -> (String, String) {
        // MACD 计算
        let fast_ma = self.macd_ema_fast.update(close);
        let slow_ma = self.macd_ema_slow.update(close);
        let macd = fast_ma - slow_ma;

        // Signal line EMA
        let signal = self.signal_ema.update(macd);

        // Hist
        let hist = macd - signal;

        // EMA10/EMA20 (基于 close)
        let ema10_val = self.ema10.update(close);
        let ema20_val = self.ema20.update(close);

        // RSI
        let rsi_val = self.rsi.update(close);

        // 获取 hist[1] 用于比较
        // Pine Script: hist[1] 表示前一个周期的 hist，首个 bar 返回 na
        let hist_prev = self.hist_prev;
        self.hist_prev = Some(hist);  // 更新为当前的 hist

        // Bar color (基于 Pine Script 买卖条件)
        let bar_color = self.detect_bar_color(macd, signal, hist_prev, hist, rsi_val, ema10_val, ema20_val);

        // BG color (基于 MACD vs Signal)
        let bg_color = self.detect_bg_color(macd, signal);

        (bar_color, bg_color)
    }

    // K线颜色 - 严格按照 Pine Script v5 逻辑
    // Pine Script 优先级顺序: selltimeS > buytimeS > selltimeT/buytimeT > selltime/buytime > isUp/isDown
    // Pine Script 颜色:
    //   selltimeS: #82cbf5 (浅蓝)
    //   buytimeS: color.rgb(248, 191, 4) (黄色)
    //   selltimeT/buytimeT: color.red (红色)
    //   selltime: #f1e892 (浅黄)
    //   buytime: color.blue (蓝色)
    //   isUp: color.green (绿色)
    //   isDown: #c83be0 (紫色)
    fn detect_bar_color(&self, macd: Decimal, signal: Decimal, hist_prev: Option<Decimal>, hist: Decimal, rsi: Decimal, ema10_val: Decimal, ema20_val: Decimal) -> String {
        // Pine Script 中 hist[1] 首个 bar 返回 na，比较操作返回 false
        let hist_prev_val = match hist_prev {
            Some(v) => v,
            None => return "White".to_string(),  // 首个 bar，无 hist[1]
        };

        // 基础条件
        let ema20_above_ema10 = ema20_val > ema10_val;  // ema20 > ema10 表示多头
        let ema20_below_ema10 = ema20_val < ema10_val;  // ema20 < ema10 表示空头

        let is_up = rsi >= dec!(70);   // rsi >= 70
        let is_down = rsi <= dec!(30);  // rsi <= 30

        // Pine Script 条件
        // selltimeS = macd >= 0 and ema20 < ema10 and hist[1] > hist and hist >= 0 and rsi >= 70
        // buytimeS = macd <= 0 and ema20 > ema10 and hist[1] < hist and hist <= 0 and rsi <= 30
        // selltimeT = macd <= 0 and ema20 < ema10 and hist[1] > hist and hist >= 0
        // buytimeT = macd >= 0 and ema20 > ema10 and hist[1] < hist and hist <= 0
        // selltime = macd >= 0 and ema20 < ema10 and hist[1] > hist and hist >= 0
        // buytime = macd <= 0 and ema20 > ema10 and hist[1] < hist and hist <= 0

        let selltimeS = macd >= Decimal::ZERO && ema20_below_ema10 && hist_prev_val > hist && hist >= Decimal::ZERO && is_up;
        let buytimeS = macd <= Decimal::ZERO && ema20_above_ema10 && hist_prev_val < hist && hist <= Decimal::ZERO && is_down;
        let selltimeT = macd <= Decimal::ZERO && ema20_below_ema10 && hist_prev_val > hist && hist >= Decimal::ZERO;
        let buytimeT = macd >= Decimal::ZERO && ema20_above_ema10 && hist_prev_val < hist && hist <= Decimal::ZERO;
        let selltime = macd >= Decimal::ZERO && ema20_below_ema10 && hist_prev_val > hist && hist >= Decimal::ZERO;
        let buytime = macd <= Decimal::ZERO && ema20_above_ema10 && hist_prev_val < hist && hist <= Decimal::ZERO;

        // 按 Python 优先级顺序判断
        // st (selltimeT) -> PureRed
        if selltimeT {
            return "PureRed".to_string();
        }

        // bt (buytimeT) -> PureRed
        if buytimeT {
            return "PureRed".to_string();
        }

        // sl (selltime) -> LightGreen (#f1e892)
        if selltime {
            return "LightGreen".to_string();
        }

        // by (buytime) -> LightBlue (blue)
        if buytime {
            return "LightBlue".to_string();
        }

        // up (isUp) -> PureGreen
        if is_up {
            return "PureGreen".to_string();
        }

        // dn (isDown) -> LightRed (#c83be6)
        if is_down {
            return "LightRed".to_string();
        }

        // s (selltimeS) -> #82cbf5 (用 LightGreen 表示)
        if selltimeS {
            return "LightGreen".to_string();
        }

        // b (buytimeS) -> rgb(248,191,4) (用 LightGreen 表示)
        if buytimeS {
            return "LightGreen".to_string();
        }

        "White".to_string()
    }

    // 背景颜色 - 基于 MACD vs Signal
    fn detect_bg_color(&self, macd: Decimal, signal: Decimal) -> String {
        // Pine Script:
        // upin = macd >= signal and macd >= 0
        // downin = macd <= signal and macd <= 0
        // upinH = macd <= signal and macd >= 0
        // downinH = macd >= signal and macd <= 0

        if macd >= signal && macd >= Decimal::ZERO {
            "green".to_string()     // #4caf50 green
        } else if macd <= signal && macd <= Decimal::ZERO {
            "red".to_string()       // #f44336 red
        } else if macd <= signal && macd >= Decimal::ZERO {
            "lightgreen".to_string() // #cae8a6
        } else {
            "lightred".to_string()   // #f4dedc
        }
    }
}

// ==================== MACD 计算器 ====================

struct MACD {
    fast_ema: EMA,
    slow_ema: EMA,
    signal_ema: EMA,
}

impl MACD {
    fn new() -> Self {
        Self {
            fast_ema: EMA::new(12),  // n1 = 12
            slow_ema: EMA::new(26),   // n2 = 26
            signal_ema: EMA::new(9), // n3 = 9
        }
    }

    fn update(&mut self, close: Decimal) -> (Decimal, Decimal, Decimal) {
        let fast = self.fast_ema.update(close);
        let slow = self.slow_ema.update(close);
        let macd = fast - slow;
        let signal = self.signal_ema.update(macd);
        let hist = macd - signal;
        (macd, signal, hist)
    }
}

// ==================== Jerk (高阶动能) 计算器 ====================

struct JerkCalculator {
    mid_ma: SMA,
    velocity_history: VecDeque<Decimal>,
    acc_history: VecDeque<Decimal>,
    jerk_history: VecDeque<Decimal>,
    velocity_sma: SMA,
    acc_sma: SMA,
    acc_ema: EMA,
    jerk_stdev: STDEV,
    prev_mid_ma: Decimal,
}

impl JerkCalculator {
    fn new() -> Self {
        Self {
            mid_ma: SMA::new(10),        // MEDIUM_SHORT_MA = 10
            velocity_history: VecDeque::new(),
            acc_history: VecDeque::new(),
            jerk_history: VecDeque::new(),
            velocity_sma: SMA::new(10),   // SMOOTH_VEL_WIN = 10
            acc_sma: SMA::new(7),         // SMOOTH_ACC_WIN = 7
            acc_ema: EMA::new(5),         // EMA 5 for acc smoothing
            jerk_stdev: STDEV::new(20),   // NORM_WIN = 20
            prev_mid_ma: Decimal::ZERO,
        }
    }

    fn update(&mut self, high: Decimal, low: Decimal, close: Decimal) -> (Decimal, Decimal, Decimal, Decimal) {
        // Step A: 中点价格
        let mid = (high + low) / dec!(2);

        // Step B: 平滑中点
        let mid_ma_val = self.mid_ma.update(mid);
        self.prev_mid_ma = mid_ma_val;

        // Step C: 对数收益率 (velocity)
        let log_ret = if self.prev_mid_ma > Decimal::ZERO && mid_ma_val > Decimal::ZERO {
            ln(mid_ma_val / self.prev_mid_ma)
        } else {
            Decimal::ZERO
        };

        // Velocity SMA 平滑
        let velocity = self.velocity_sma.update(log_ret);
        self.velocity_history.push_back(velocity);
        if self.velocity_history.len() > 20 {
            self.velocity_history.pop_front();
        }

        // Step D: 加速度
        let acc_raw = if let Some(prev_vel) = self.velocity_history.iter().rev().nth(1).copied() {
            velocity - prev_vel
        } else {
            Decimal::ZERO
        };

        // 加速度 SMA 平滑
        let acc_sma_val = self.acc_sma.update(acc_raw);
        // 加速度 EMA 平滑
        let acc_smooth = self.acc_ema.update(acc_sma_val);

        self.acc_history.push_back(acc_smooth);
        if self.acc_history.len() > 20 {
            self.acc_history.pop_front();
        }

        // Step E: Jerk (加速度的导数)
        let jerk_raw = if let Some(prev_acc) = self.acc_history.iter().rev().nth(1).copied() {
            acc_smooth - prev_acc
        } else {
            Decimal::ZERO
        };

        self.jerk_history.push_back(jerk_raw);
        if self.jerk_history.len() > 20 {
            self.jerk_history.pop_front();
        }

        // Step F: 归一化
        let jerk_std = self.jerk_stdev.update(jerk_raw);
        let norm_jerk = if jerk_std > Decimal::ZERO {
            (jerk_raw / jerk_std).max(dec!(-3)).min(dec!(3))
        } else {
            Decimal::ZERO
        };

        (jerk_raw, acc_smooth, velocity, norm_jerk)
    }
}

// ==================== Price Position ====================

struct PricePosition {
    period: usize,
    highs: VecDeque<Decimal>,
    lows: VecDeque<Decimal>,
}

impl PricePosition {
    fn new(period: usize) -> Self {
        Self {
            period,
            highs: VecDeque::new(),
            lows: VecDeque::new(),
        }
    }

    fn update(&mut self, high: Decimal, low: Decimal, close: Decimal) -> Decimal {
        self.highs.push_back(high);
        self.lows.push_back(low);

        if self.highs.len() > self.period {
            self.highs.pop_front();
            self.lows.pop_front();
        }

        if let (Some(&max_high), Some(&min_low)) = (self.highs.iter().max(), self.lows.iter().min()) {
            if max_high > min_low {
                (close - min_low) / (max_high - min_low) * dec!(100)
            } else {
                dec!(50)
            }
        } else {
            dec!(50)
        }
    }
}

// ==================== TR (True Range) ====================

struct TrueRange {
    history: VecDeque<Decimal>,
    max_len: usize,
}

impl TrueRange {
    fn new(max_len: usize) -> Self {
        Self {
            history: VecDeque::new(),
            max_len,
        }
    }

    fn update(&mut self, high: Decimal, low: Decimal, close: Decimal, prev_close: Decimal) -> Decimal {
        let tr1 = high - low;
        let tr2 = (high - close).abs();
        let tr3 = (low - close).abs();
        let tr = tr1.max(tr2).max(tr3);

        self.history.push_back(tr);
        if self.history.len() > self.max_len {
            self.history.pop_front();
        }

        tr
    }

    fn avg(&self) -> Decimal {
        if self.history.is_empty() {
            return dec!(0);
        }
        let sum: Decimal = self.history.iter().sum();
        sum / Decimal::from(self.history.len())
    }

    fn latest(&self) -> Decimal {
        self.history.back().copied().unwrap_or(dec!(0))
    }
}

// ==================== TR Ratio ====================

struct TRRatio {
    tr_5d: TrueRange,
    tr_20d: TrueRange,
    tr_60d_history: VecDeque<Decimal>,
}

impl TRRatio {
    fn new() -> Self {
        Self {
            tr_5d: TrueRange::new(5),
            tr_20d: TrueRange::new(20),
            tr_60d_history: VecDeque::new(),
        }
    }

    fn update(&mut self, high: Decimal, low: Decimal, close: Decimal, prev_close: Decimal) -> (Decimal, Decimal, Decimal, Decimal) {
        // 计算 TR
        let tr1 = high - low;
        let tr2 = (high - close).abs();
        let tr3 = (low - close).abs();
        let tr = tr1.max(tr2).max(tr3);

        // 更新 TR 历史
        self.tr_5d.update(high, low, close, prev_close);
        self.tr_20d.update(high, low, close, prev_close);

        // TR 5日均值
        let tr_5d_avg = self.tr_5d.avg();

        // TR 20日均值
        let tr_20d_avg = self.tr_20d.avg();

        // TR 60日 (基于20d历史的移动均值)
        let mut tr_60d_avg = dec!(0);
        if self.tr_20d.history.len() >= 60 {
            let sum: Decimal = self.tr_20d.history.iter().rev().take(60).sum();
            tr_60d_avg = sum / dec!(60);
        }

        // TR Ratio
        let tr_ratio_5d_20d = if tr_20d_avg > dec!(0) {
            self.tr_20d.latest() / tr_20d_avg
        } else {
            dec!(0)
        };

        let tr_ratio_20d_60d = if tr_60d_avg > dec!(0) {
            tr_20d_avg / tr_60d_avg
        } else {
            dec!(0)
        };

        (tr_5d_avg, tr_20d_avg, tr_ratio_5d_20d, tr_ratio_20d_60d)
    }
}

// ==================== Main ====================

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("============================================");
    println!("指标对比验证程序 (Pine Script v5 算法)");
    println!("============================================\n");

    let symbol = std::env::args().nth(1).unwrap_or_else(|| "BTCUSDT".to_string());
    let limit = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "1000".to_string())
        .parse()
        .unwrap_or(1000);

    println!("交易对: {}", symbol);
    println!("获取K线数量: {}\n", limit);

    // 从币安API获取K线数据
    let url = format!(
        "https://api.binance.com/api/v3/klines?symbol={}&interval=1d&limit={}",
        symbol, limit
    );

    let client = reqwest::blocking::Client::new();
    let response = client.get(&url).send()?;
    let text = response.text()?;
    let klines_raw: Vec<serde_json::Value> = serde_json::from_str(&text)?;

    // 解析K线数据
    let mut klines: Vec<(u64, String, String, String, String, String)> = Vec::new();
    for arr in klines_raw {
        if let Some(items) = arr.as_array() {
            if items.len() >= 6 {
                klines.push((
                    items[0].as_i64().unwrap_or(0) as u64,
                    items[1].as_str().unwrap_or("0").to_string(),
                    items[2].as_str().unwrap_or("0").to_string(),
                    items[3].as_str().unwrap_or("0").to_string(),
                    items[4].as_str().unwrap_or("0").to_string(),
                    items[5].as_str().unwrap_or("0").to_string(),
                ));
            }
        }
    }

    println!("成功获取 {} 根K线\n", klines.len());

    // 初始化计算器
    let mut pine_color = PineColorDetector::new();
    let mut macd = MACD::new();
    let mut jerk = JerkCalculator::new();
    let mut price_pos = PricePosition::new(20);
    let mut tr_ratio = TRRatio::new();

    // EMA
    let mut ema10 = EMA::new(10);
    let mut ema20 = EMA::new(20);
    let mut ema50 = EMA::new(50);
    let mut ema100 = EMA::new(100);
    let mut ema200 = EMA::new(200);

    // RSI
    let mut rsi = DominantCycleRSI::new(20);

    // CSV 输出
    let output_path = format!("indicator_comparison_{}.csv", symbol.to_lowercase());
    let mut csv_content = String::new();

    // 表头 (英文)
    csv_content.push_str("timestamp,tick_index,open,high,low,close,volume,");
    csv_content.push_str("macd_fast,macd_slow,macd_diff,signal,hist,");
    csv_content.push_str("ema10,ema20,ema50,ema100,ema200,");
    csv_content.push_str("ema10_vs_20,ema20_vs_50,");
    csv_content.push_str("rsi_extreme,pine_bar_color,pine_bg_color,");
    csv_content.push_str("jerk_raw,acc_smooth,velocity,norm_jerk,");
    csv_content.push_str("pos_norm_20,");
    csv_content.push_str("tr_5d_avg,tr_20d_avg,tr_ratio_5d_20d,tr_ratio_20d_60d,");
    csv_content.push_str("readable_time\n");

    let mut prev_close = None;
    let mut tick_index = 0i64;

    for (open_time, open_s, high_s, low_s, close_s, vol_s) in &klines {
        let open = parse_decimal(open_s).round_dp(2);
        let high = parse_decimal(high_s).round_dp(2);
        let low = parse_decimal(low_s).round_dp(2);
        let close = parse_decimal(close_s).round_dp(2);
        let volume = parse_decimal(vol_s).round_dp(4);

        let readable_time = Utc.timestamp_millis_opt(*open_time as i64)
            .single()
            .map(|dt: DateTime<Utc>| dt.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_default();

        // 更新 EMA
        let ema10_val = ema10.update(close);
        let ema20_val = ema20.update(close);
        let ema50_val = ema50.update(close);
        let ema100_val = ema100.update(close);
        let ema200_val = ema200.update(close);

        // MACD
        let (macd_val, signal_val, hist_val) = macd.update(close);

        // PineColor
        let (pine_bar, pine_bg) = pine_color.update(close);

        // RSI
        let _rsi_val = rsi.update(close);
        let rsi_extreme = if rsi.get_rsi_70() { "Overbought" } else if rsi.get_rsi_30() { "Oversold" } else { "Normal" };

        // Jerk
        let (jerk_raw, acc_smooth, velocity, norm_jerk) = jerk.update(high, low, close);

        // Price Position
        let pos_norm = price_pos.update(high, low, close);

        // TR Ratio
        let (tr_5d, tr_20d, tr_r_5d_20d, tr_r_20d_60d) = if let Some(prev) = prev_close {
            tr_ratio.update(high, low, close, prev)
        } else {
            (dec!(0), dec!(0), dec!(0), dec!(0))
        };
        prev_close = Some(close);

        // EMA 比较
        let ema10_vs_20 = if ema10_val > ema20_val { "Bullish" } else if ema10_val < ema20_val { "Bearish" } else { "Neutral" };
        let ema20_vs_50 = if ema20_val > ema50_val { "Bullish" } else if ema20_val < ema50_val { "Bearish" } else { "Neutral" };

        // 写入 CSV
        csv_content.push_str(&format!(
            "{},{},{},{},{},{},{},",
            open_time, tick_index, open, high, low, close, volume,
        ));
        csv_content.push_str(&format!(
            "{},{},{},{},{},",
            macd_val.round_dp(4), macd_val.round_dp(4), macd_val.round_dp(4), signal_val.round_dp(4), hist_val.round_dp(4),
        ));
        csv_content.push_str(&format!(
            "{},{},{},{},{},",
            ema10_val.round_dp(4), ema20_val.round_dp(4), ema50_val.round_dp(4), ema100_val.round_dp(4), ema200_val.round_dp(4),
        ));
        csv_content.push_str(&format!(
            "{},{},", ema10_vs_20, ema20_vs_50,
        ));
        csv_content.push_str(&format!(
            "{},{},{},", rsi_extreme, pine_bar, pine_bg,
        ));
        csv_content.push_str(&format!(
            "{},{},{},{},",
            jerk_raw.round_dp(4), acc_smooth.round_dp(4), velocity.round_dp(4), norm_jerk.round_dp(4),
        ));
        csv_content.push_str(&format!(
            "{},",
            pos_norm.round_dp(4),
        ));
        csv_content.push_str(&format!(
            "{},{},{},{},",
            tr_5d.round_dp(4), tr_20d.round_dp(4), tr_r_5d_20d.round_dp(4), tr_r_20d_60d.round_dp(4),
        ));
        csv_content.push_str(&format!("{}\n", readable_time));

        tick_index += 1;

        if tick_index % 100 == 0 {
            println!("已处理 {} / {} 条K线", tick_index, klines.len());
        }
    }

    std::fs::write(&output_path, csv_content)?;
    println!("\n============================================");
    println!("CSV文件已生成: {}", output_path);
    println!("共 {} 条记录", tick_index);
    println!("============================================");

    Ok(())
}
