//! Pine Script v5 指标计算模块
//!
//! 严格按照 Pine Script @version=5 算法实现所有指标计算

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::VecDeque;

// ==================== 辅助函数 ====================

/// 自然对数 (泰勒级数展开)
pub fn ln(x: Decimal) -> Decimal {
    if x <= dec!(0) {
        return dec!(0);
    }
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

/// 平方根 (牛顿法)
pub fn sqrt(x: Decimal) -> Decimal {
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

// ==================== EMA ====================

#[derive(Debug, Clone)]
pub struct EMA {
    pub period: usize,
    pub value: Decimal,
}

impl EMA {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            value: Decimal::ZERO,
        }
    }

    pub fn update(&mut self, price: Decimal) -> Decimal {
        if self.value == Decimal::ZERO {
            self.value = price;
        } else {
            let alpha = dec!(2) / Decimal::from(self.period + 1);
            self.value = price * alpha + self.value * (dec!(1) - alpha);
        }
        self.value
    }

    pub fn get(&self) -> Decimal {
        self.value
    }
}

// ==================== SMA ====================

#[derive(Debug, Clone)]
pub struct SMA {
    period: usize,
    values: VecDeque<Decimal>,
    sum: Decimal,
}

impl SMA {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            values: VecDeque::new(),
            sum: Decimal::ZERO,
        }
    }

    pub fn update(&mut self, price: Decimal) -> Decimal {
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

// ==================== RMA (RSI 平滑) ====================

#[derive(Debug, Clone)]
pub struct RMA {
    period: usize,
    alpha: Decimal,
    value: Decimal,
    initialized: bool,
}

impl RMA {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            alpha: dec!(1) / Decimal::from(period),
            value: Decimal::ZERO,
            initialized: false,
        }
    }

    pub fn update(&mut self, price: Decimal) -> Decimal {
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

#[derive(Debug, Clone)]
pub struct STDEV {
    period: usize,
    values: VecDeque<Decimal>,
    mean: Decimal,
}

impl STDEV {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            values: VecDeque::new(),
            mean: Decimal::ZERO,
        }
    }

    pub fn update(&mut self, price: Decimal) -> Decimal {
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

        sqrt(variance.abs())
    }
}

// ==================== Dominant Cycle RSI ====================

#[derive(Debug, Clone)]
pub struct DominantCycleRSI {
    period: usize,
    cyclelen: usize,
    vibration: usize,
    leveling: Decimal,
    cyclicmemory: usize,
    torque: Decimal,
    phasinglag: Decimal,
    rma_up: RMA,
    rma_down: RMA,
    crsi: Decimal,
    prev_crsi: Decimal,
    last_price: Decimal,
    lmax_history: VecDeque<Decimal>,
    lmin_history: VecDeque<Decimal>,
}

impl DominantCycleRSI {
    pub fn new(period: usize) -> Self {
        let cyclelen = period / 2;
        let vibration = 10;
        let leveling = dec!(10.0);
        let cyclicmemory = period * 2;
        let torque = dec!(2) / Decimal::from(vibration + 1);
        let phasinglag = Decimal::from(vibration - 1) / dec!(2);

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
            lmax_history: VecDeque::new(),
            lmin_history: VecDeque::new(),
        }
    }

    pub fn update(&mut self, price: Decimal) -> Decimal {
        // 计算价格变化
        let change = if self.last_price > Decimal::ZERO {
            price - self.last_price
        } else {
            Decimal::ZERO
        };
        self.last_price = price;

        let up = if change > Decimal::ZERO { change } else { -change };
        let down = if change < Decimal::ZERO { -change } else { Decimal::ZERO };

        // RSI
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

        // CRSI
        let crsi_calc = self.torque * (dec!(2) * rsi_rma - self.get_prev_crsi()) +
                        (dec!(1) - self.torque) * self.crsi;

        self.prev_crsi = self.crsi;
        self.crsi = crsi_calc;

        // 更新历史极值
        self.lmax_history.push_back(self.crsi);
        self.lmin_history.push_back(self.crsi);

        if self.lmax_history.len() > self.cyclicmemory {
            self.lmax_history.pop_front();
            self.lmin_history.pop_front();
        }

        self.crsi
    }

    fn get_prev_crsi(&self) -> Decimal {
        let lag = self.phasinglag.to_string().parse::<usize>().unwrap_or(0);
        let idx = self.cyclicmemory.saturating_sub(lag);
        if idx < self.lmax_history.len() {
            self.lmax_history[idx]
        } else {
            self.crsi
        }
    }

    pub fn is_overbought(&self) -> bool {
        self.crsi >= dec!(70)
    }

    pub fn is_oversold(&self) -> bool {
        self.crsi <= dec!(30)
    }

    pub fn get_value(&self) -> Decimal {
        self.crsi
    }
}

// ==================== MACD ====================

#[derive(Debug, Clone)]
pub struct MACD {
    fast_ema: EMA,
    slow_ema: EMA,
    signal_ema: EMA,
}

impl MACD {
    pub fn new(fast: usize, slow: usize, signal: usize) -> Self {
        Self {
            fast_ema: EMA::new(fast),
            slow_ema: EMA::new(slow),
            signal_ema: EMA::new(signal),
        }
    }

    pub fn update(&mut self, close: Decimal) -> (Decimal, Decimal, Decimal) {
        let fast = self.fast_ema.update(close);
        let slow = self.slow_ema.update(close);
        let macd = fast - slow;
        let signal = self.signal_ema.update(macd);
        let hist = macd - signal;
        (macd, signal, hist)
    }

    pub fn get_macd(&self) -> Decimal {
        self.fast_ema.get() - self.slow_ema.get()
    }

    pub fn get_signal(&self) -> Decimal {
        self.signal_ema.get()
    }

    pub fn get_hist(&self) -> Decimal {
        self.get_macd() - self.get_signal()
    }
}

// ==================== PineColor ====================

/// Pine 颜色枚举
#[derive(Debug, Clone, PartialEq)]
pub enum PineBarColor {
    PureGreen,   // 纯绿 (isUp: rsi >= 70)
    LightGreen,  // 浅绿/浅蓝/黄色 (selltime, selltimeS, buytimeS)
    PureRed,     // 纯红 (selltimeT, buytimeT)
    LightRed,    // 浅红/紫色 (isDown: rsi <= 30)
    LightBlue,   // 蓝色 (buytime)
    White,       // 白色
}

impl PineBarColor {
    pub fn to_str(&self) -> &str {
        match self {
            PineBarColor::PureGreen => "PureGreen",
            PineBarColor::LightGreen => "LightGreen",
            PineBarColor::PureRed => "PureRed",
            PineBarColor::LightRed => "LightRed",
            PineBarColor::LightBlue => "LightBlue",
            PineBarColor::White => "White",
        }
    }
}

/// Pine 背景颜色枚举
#[derive(Debug, Clone, PartialEq)]
pub enum PineBgColor {
    Green,      // 绿色
    Red,        // 红色
    LightGreen, // 浅绿
    LightRed,   // 浅红
    White,      // 白色
}

impl PineBgColor {
    pub fn to_str(&self) -> &str {
        match self {
            PineBgColor::Green => "green",
            PineBgColor::Red => "red",
            PineBgColor::LightGreen => "lightgreen",
            PineBgColor::LightRed => "lightred",
            PineBgColor::White => "white",
        }
    }
}

/// Pine 颜色检测器
#[derive(Debug, Clone)]
pub struct PineColorDetector {
    // MACD EMA (Pine Script: n1=20, n2=50)
    macd_fast: EMA,
    macd_slow: EMA,
    // Signal EMA (Pine Script: n3=9)
    signal_ema: EMA,
    // 独立的 EMA10/EMA20 (基于 close 计算)
    ema10: EMA,
    ema20: EMA,
    // RSI (Dominant Cycle)
    rsi: DominantCycleRSI,
    // 前一个 hist 值
    hist_prev: Decimal,
}

impl PineColorDetector {
    pub fn new() -> Self {
        Self {
            macd_fast: EMA::new(20),   // Fast Length = 20
            macd_slow: EMA::new(50),  // Slow Length = 50
            signal_ema: EMA::new(9),    // Signal Smoothing = 9
            ema10: EMA::new(10),       // EMA 10 (基于 close)
            ema20: EMA::new(20),       // EMA 20 (基于 close)
            rsi: DominantCycleRSI::new(20),
            hist_prev: Decimal::ZERO,
        }
    }

    /// 更新并返回 K线颜色和背景颜色
    pub fn update(&mut self, high: Decimal, low: Decimal, close: Decimal) -> (PineBarColor, PineBgColor) {
        // MACD 计算
        let fast_ma = self.macd_fast.update(close);
        let slow_ma = self.macd_slow.update(close);
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

        // Bar color (基于 Pine Script barcolor 逻辑)
        let bar_color = self.detect_bar_color(close, macd, signal, hist, rsi_val, ema10_val, ema20_val);

        // BG color (基于 MACD vs Signal)
        let bg_color = self.detect_bg_color(macd, signal);

        self.hist_prev = hist;

        (bar_color, bg_color)
    }

    /// K线颜色检测 - 严格按照 Pine Script barcolor 逻辑
    ///
    /// Python 原始条件 (按优先级从低到高):
    /// - st (selltimeT) = macd<=0 and ema20<ema10 and hist_prev>hist and hist>=0       -> PureRed (color.red)
    /// - bt (buytimeT) = macd>=0 and ema20>ema10 and hist_prev<hist and hist<=0       -> PureRed (color.red)
    /// - sl (selltime) = macd>=0 and ema20<ema10 and hist_prev>hist and hist>=0       -> LightGreen (#f1e892)
    /// - by (buytime) = macd<=0 and ema20>ema10 and hist_prev<hist and hist<=0        -> LightBlue (blue)
    /// - up (isUp) = rsi >= 70                                                           -> PureGreen (color.green)
    /// - dn (isDown) = rsi <= 30                                                         -> LightRed (#c83be6)
    /// - s (selltimeS) = selltime and is_up  (macd>=0 and ema20<ema10 and hist>=0 and rsi>=70) -> LightGreen (#82cbf5)
    /// - b (buytimeS) = buytime and is_down (macd<=0 and ema20>ema10 and hist<=0 and rsi<=30)  -> LightGreen (rgb(248,191,4))
    ///
    /// 注意: Python 中 selltimeS 和 buytimeS 覆盖前面的 isUp/isDown
    fn detect_bar_color(&self, close: Decimal, macd: Decimal, signal: Decimal, hist: Decimal, rsi: Decimal, ema10_val: Decimal, ema20_val: Decimal) -> PineBarColor {
        let hist_prev = self.hist_prev;

        // 基础条件 (来自 Python calculate_trade_conditions)
        let ema20_above_ema10 = ema20_val > ema10_val;  // ema20 > ema10 表示多头趋势
        let ema20_below_ema10 = ema20_val < ema10_val;  // ema20 < ema10 表示空头趋势

        let is_up = rsi >= dec!(70);   // rsi >= 70
        let is_down = rsi <= dec!(30);  // rsi <= 30

        // 衍生条件
        let selltime = macd >= Decimal::ZERO && ema20_below_ema10 && hist_prev > hist && hist >= Decimal::ZERO;
        let buytime = macd <= Decimal::ZERO && ema20_above_ema10 && hist_prev < hist && hist <= Decimal::ZERO;
        let selltimeT = macd <= Decimal::ZERO && ema20_below_ema10 && hist_prev > hist && hist >= Decimal::ZERO;
        let buytimeT = macd >= Decimal::ZERO && ema20_above_ema10 && hist_prev < hist && hist <= Decimal::ZERO;
        let selltimeS = selltime && is_up;   // 强卖信号
        let buytimeS = buytime && is_down;   // 强买信号

        // 按 Python 优先级顺序判断 (从 st/bt 低优先级到 s/b 高优先级)
        // Python 顺序: [st, bt, sl, by, up, dn, s, b]
        // 颜色顺序: [red, red, #f1e892, blue, green, #c83be0, #82cbf5, rgb(248,191,4)]

        // st (selltimeT) -> PureRed
        if selltimeT {
            return PineBarColor::PureRed;
        }

        // bt (buytimeT) -> PureRed
        if buytimeT {
            return PineBarColor::PureRed;
        }

        // sl (selltime) -> LightGreen (#f1e892)
        if selltime {
            return PineBarColor::LightGreen;
        }

        // by (buytime) -> LightBlue (blue)
        if buytime {
            return PineBarColor::LightBlue;
        }

        // up (isUp) -> PureGreen
        if is_up {
            return PineBarColor::PureGreen;
        }

        // dn (isDown) -> LightRed (#c83be6)
        if is_down {
            return PineBarColor::LightRed;
        }

        // s (selltimeS) -> LightGreen (#82cbf5) - 注意这是浅蓝
        if selltimeS {
            return PineBarColor::LightGreen; // #82cbf5 在 Rust 端用 LightGreen 表示
        }

        // b (buytimeS) -> LightGreen (rgb(248,191,4)) - 注意这是黄色
        if buytimeS {
            return PineBarColor::LightGreen; // rgb(248,191,4) 在 Rust 端用 LightGreen 表示
        }

        PineBarColor::White
    }

    /// 背景颜色检测 - 严格按照 Pine Script bgcolor 逻辑
    ///
    /// Pine Script 原始逻辑:
    /// - upin = macd >= signal and macd >= 0  -> color.green (透明度75)
    /// - downin = macd <= signal and macd <= 0 -> color.red (透明度75)
    /// - upinH = macd <= signal and macd >= 0  -> #cae8a6 (透明度75)
    /// - downinH = macd >= signal and macd <= 0 -> #f4dedc (透明度75)
    fn detect_bg_color(&self, macd: Decimal, signal: Decimal) -> PineBgColor {
        let macd_above_signal = macd >= signal;

        if macd >= signal && macd >= Decimal::ZERO {
            PineBgColor::Green      // #4caf50
        } else if macd <= signal && macd <= Decimal::ZERO {
            PineBgColor::Red        // #f44336
        } else if macd <= signal && macd >= Decimal::ZERO {
            PineBgColor::LightGreen // #cae8a6
        } else {
            PineBgColor::LightRed   // #f4dedc
        }
    }

    /// 获取 RSI 值
    pub fn get_rsi(&self) -> Decimal {
        self.rsi.get_value()
    }

    /// 是否超买
    pub fn is_overbought(&self) -> bool {
        self.rsi.is_overbought()
    }

    /// 是否超卖
    pub fn is_oversold(&self) -> bool {
        self.rsi.is_oversold()
    }
}

impl Default for PineColorDetector {
    fn default() -> Self {
        Self::new()
    }
}

// ==================== Jerk (高阶动能) ====================

#[derive(Debug, Clone)]
pub struct JerkCalculator {
    mid_ma: SMA,                // MEDIUM_SHORT_MA = 10
    velocity_sma: SMA,          // SMOOTH_VEL_WIN = 10
    acc_sma: SMA,               // SMOOTH_ACC_WIN = 7
    acc_ema: EMA,               // EMA 5 for smoothing
    jerk_stdev: STDEV,          // NORM_WIN = 20
    velocity_history: VecDeque<Decimal>,
    acc_history: VecDeque<Decimal>,
    jerk_history: VecDeque<Decimal>,
    prev_mid_ma: Decimal,
}

impl JerkCalculator {
    pub fn new() -> Self {
        Self {
            mid_ma: SMA::new(10),
            velocity_sma: SMA::new(10),
            acc_sma: SMA::new(7),
            acc_ema: EMA::new(5),
            jerk_stdev: STDEV::new(20),
            velocity_history: VecDeque::new(),
            acc_history: VecDeque::new(),
            jerk_history: VecDeque::new(),
            prev_mid_ma: Decimal::ZERO,
        }
    }

    /// 更新并返回 (jerk_raw, acc_smooth, velocity, norm_jerk)
    pub fn update(&mut self, high: Decimal, low: Decimal, close: Decimal) -> (Decimal, Decimal, Decimal, Decimal) {
        // Step A: 中点价格
        let mid = (high + low) / dec!(2);

        // Step B: 平滑中点 (SMA 10)
        let mid_ma_val = self.mid_ma.update(mid);
        self.prev_mid_ma = mid_ma_val;

        // Step C: 对数收益率 (velocity)
        let log_ret = if self.prev_mid_ma > Decimal::ZERO && mid_ma_val > Decimal::ZERO {
            ln(mid_ma_val / self.prev_mid_ma)
        } else {
            Decimal::ZERO
        };

        // Velocity SMA 平滑 (10)
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

        // 加速度 SMA 平滑 (7)
        let acc_sma_val = self.acc_sma.update(acc_raw);

        // 加速度 EMA 平滑 (5)
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

impl Default for JerkCalculator {
    fn default() -> Self {
        Self::new()
    }
}

// ==================== Price Position ====================

#[derive(Debug, Clone)]
pub struct PricePosition {
    period: usize,
    highs: VecDeque<Decimal>,
    lows: VecDeque<Decimal>,
}

impl PricePosition {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            highs: VecDeque::new(),
            lows: VecDeque::new(),
        }
    }

    pub fn update(&mut self, high: Decimal, low: Decimal, close: Decimal) -> Decimal {
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

#[derive(Debug, Clone)]
pub struct TrueRange {
    history: VecDeque<Decimal>,
    max_len: usize,
}

impl TrueRange {
    pub fn new(max_len: usize) -> Self {
        Self {
            history: VecDeque::new(),
            max_len,
        }
    }

    pub fn update(&mut self, high: Decimal, low: Decimal, close: Decimal, prev_close: Decimal) -> Decimal {
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

    pub fn avg(&self) -> Decimal {
        if self.history.is_empty() {
            return dec!(0);
        }
        let sum: Decimal = self.history.iter().sum();
        sum / Decimal::from(self.history.len())
    }

    pub fn latest(&self) -> Decimal {
        self.history.back().copied().unwrap_or(dec!(0))
    }
}

// ==================== TR Ratio ====================

#[derive(Debug, Clone)]
pub struct TRRatio {
    tr_5d: TrueRange,
    tr_20d: TrueRange,
}

impl TRRatio {
    pub fn new() -> Self {
        Self {
            tr_5d: TrueRange::new(5),
            tr_20d: TrueRange::new(20),
        }
    }

    pub fn update(&mut self, high: Decimal, low: Decimal, close: Decimal, prev_close: Decimal) -> (Decimal, Decimal, Decimal, Decimal) {
        // 更新 TR
        self.tr_5d.update(high, low, close, prev_close);
        self.tr_20d.update(high, low, close, prev_close);

        // TR 均值
        let tr_5d_avg = self.tr_5d.avg();
        let tr_20d_avg = self.tr_20d.avg();

        // TR 60日 (基于20d历史的移动均值)
        let mut tr_60d_avg = dec!(0);
        if self.tr_20d.history.len() >= 60 {
            let sum: Decimal = self.tr_20d.history.iter().rev().take(60).sum();
            tr_60d_avg = sum / dec!(60);
        }

        // TR Ratio
        let tr_ratio_5d_20d = if tr_20d_avg > dec!(0) {
            self.tr_5d.latest() / tr_20d_avg
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

impl Default for TRRatio {
    fn default() -> Self {
        Self::new()
    }
}
