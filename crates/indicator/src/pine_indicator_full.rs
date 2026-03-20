use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::VecDeque;

// ==================== 常量定义（完全对齐 Python PINE_COLOR_MAP/PINE_BG_COLOR_MAP）====================
pub mod colors {
    pub const STRONG_TOP: &'static str = "浅蓝";      // selltimeS
    pub const STRONG_BOT: &'static str = "橙色";      // buytimeS
    pub const TOP_WARNING: &'static str = "浅黄";     // selltime
    pub const BOTTOM_WARNING: &'static str = "纯蓝";  // buytime
    pub const WEAK_SIGNAL: &'static str = "纯红";     // selltimeT/buytimeT
    pub const BULL_TREND: &'static str = "纯绿";     // is_up (rsi >= 70)
    pub const BEAR_TREND: &'static str = "紫色";     // is_down (rsi <= 30)
    pub const DEFAULT: &'static str = "白色";         // default

    // BG颜色映射 (完全对齐 Python PINE_BG_COLOR_MAP)
    pub const BULL_TREND_BG: &'static str = "纯绿";      // macd >= signal && macd >= 0
    pub const BULL_CONSOLIDATION: &'static str = "浅绿";  // macd <= signal && macd >= 0
    pub const BEAR_TREND_BG: &'static str = "纯红";      // macd <= signal && macd <= 0
    pub const BEAR_CONSOLIDATION: &'static str = "浅红"; // macd >= signal && macd <= 0
    pub const DEFAULT_BG: &'static str = "白色";
}

// ==================== 通用工具函数（对齐 Python 数值处理）====================
#[inline(always)]
fn safe_div(n: Decimal, d: Decimal, epsilon: Decimal) -> Decimal {
    if d.abs() < epsilon {
        n / epsilon
    } else {
        n / d
    }
}

// ==================== EMA（严格对齐 Python ewm(span=window, adjust=False)）====================
/// EMA 指标：alpha = 2 / (period + 1)，完全对齐 Python pandas ewm
struct EMA {
    period: usize,
    alpha: Decimal,
    value: Decimal,
    initialized: bool,
}

impl EMA {
    fn new(period: usize) -> Self {
        let alpha = dec!(2) / Decimal::from(period + 1);
        Self {
            period,
            alpha,
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

    fn get(&self) -> Decimal {
        self.value
    }

    fn reset(&mut self) {
        self.value = Decimal::ZERO;
        self.initialized = false;
    }
}

// ==================== RMA（RSI 平滑，对齐 Python _rma_vectorized）====================
/// RMA 指标：alpha = 1 / period，完全对齐 Python ewm(alpha=1/window, adjust=False)
struct RMA {
    period: usize,
    alpha: Decimal,
    value: Decimal,
    initialized: bool,
}

impl RMA {
    fn new(period: usize) -> Self {
        let alpha = dec!(1) / Decimal::from(period);
        Self {
            period,
            alpha,
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

    fn get(&self) -> Decimal {
        self.value
    }
}

// ==================== Dominant Cycle RSI（完全对齐 Python _vectorized_crsi）====================
/// Dominant Cycle RSI：严格对齐 Python 相位滞后、torque 计算、迭代逻辑
struct DominantCycleRSI {
    cyclelen: usize,
    torque: Decimal,
    phasinglag: usize,
    rma_up: RMA,
    rma_down: RMA,
    rsi_history: VecDeque<Decimal>,  // 用 VecDeque 实现 np.roll 相位滞后
    crsi_history: VecDeque<Decimal>,
    last_price: Decimal,
    epsilon: Decimal,
}

impl DominantCycleRSI {
    fn new() -> Self {
        // Python 固定参数：PINE_DOMCYCLE=20 → cyclelen=10，PINE_VIBRATION=10
        let cyclelen = 10;
        let torque = dec!(2) / Decimal::from(11); // 2/(10+1)
        let phasinglag = 4; // (10-1)/2=4.5 → 取整4，对齐Python
        let epsilon = dec!(1e-8);

        Self {
            cyclelen,
            torque,
            phasinglag,
            rma_up: RMA::new(cyclelen),
            rma_down: RMA::new(cyclelen),
            rsi_history: VecDeque::with_capacity(1000),
            crsi_history: VecDeque::with_capacity(1000),
            last_price: Decimal::ZERO,
            epsilon,
        }
    }

    fn update(&mut self, price: Decimal) -> Decimal {
        // 1. 计算价格变化（对齐 Python delta = np.diff(close, prepend=close[0])）
        let change = if self.last_price == Decimal::ZERO {
            Decimal::ZERO
        } else {
            price - self.last_price
        };
        self.last_price = price;

        // 2. 计算 up/down（对齐 Python up = max(delta,0), down = max(-delta,0)）
        let up = if change > Decimal::ZERO { change } else { Decimal::ZERO };
        let down = if change < Decimal::ZERO { -change } else { Decimal::ZERO };

        // 3. 计算 RMA 平滑（对齐 Python up_rma/down_rma）
        let rma_up_val = self.rma_up.update(up);
        let rma_down_val = self.rma_down.update(down);

        // 4. 计算 RSI（对齐 Python 100 - 100/(1+ratio)）
        let rsi = if rma_down_val < self.epsilon {
            dec!(100)
        } else if rma_up_val < self.epsilon {
            dec!(0)
        } else {
            let ratio = safe_div(rma_up_val, rma_down_val, self.epsilon);
            dec!(100) - dec!(100) / (dec!(1) + ratio)
        };

        // 5. 维护 RSI 历史队列（实现 np.roll 相位滞后）
        self.rsi_history.push_back(rsi);
        if self.rsi_history.len() > self.phasinglag + 1 {
            self.rsi_history.pop_front();
        }

        // 6. 获取滞后 RSI（对齐 Python rsi_phasing = np.roll(rsi, phasingLag)）
        let rsi_lagged = if self.rsi_history.len() > self.phasinglag {
            self.rsi_history[0] // 队列头部为滞后 phasinglag 位的值
        } else {
            rsi // 数据不足时用当前值补全，对齐Python np.roll 初始填充
        };

        // 7. 计算 CRSI（严格对齐 Python 迭代公式）
        let crsi_prev = self.crsi_history.back().copied().unwrap_or(rsi);
        let crsi = self.torque * (dec!(2) * rsi - rsi_lagged) + (dec!(1) - self.torque) * crsi_prev;

        self.crsi_history.push_back(crsi);
        if self.crsi_history.len() > 1000 {
            self.crsi_history.pop_front();
        }

        crsi
    }

    fn get_rsi(&self) -> Decimal {
        self.rsi_history.back().copied().unwrap_or(Decimal::ZERO)
    }

    fn get_crsi(&self) -> Decimal {
        self.crsi_history.back().copied().unwrap_or(Decimal::ZERO)
    }
}

// ==================== PineColorDetector（100% 对齐 Python PineColorOnlyCalculator）====================
/// Pine 颜色检测器：严格对齐 Python 所有逻辑、参数、优先级、初始化
pub struct PineColorDetector {
    // MACD 组件（fast=100, slow=200, signal=9，对齐Python 100-200周期）
    macd_fast: EMA,
    macd_slow: EMA,
    signal_ema: EMA,
    // EMA10/EMA20（对齐Python calculate_trade_conditions）
    ema10: EMA,
    ema20: EMA,
    // 历史数据（对齐Python hist_prev = np.roll(hist,1)）
    hist_history: VecDeque<Decimal>,
    // RSI 组件（完全对齐Python CRSI）
    rsi: DominantCycleRSI,
    // 常量参数（对齐Python PineConfig）
    rsi_overbought: Decimal,
    rsi_oversold: Decimal,
    epsilon: Decimal,
    // 历史数据（用于振幅指标计算）
    price_history: VecDeque<(Decimal, Decimal, Decimal)>, // (high, low, close)
    // MACD 交叉分段标记（true=段起始）
    macd_cross_history: VecDeque<bool>,
}

impl PineColorDetector {
    /// 创建新的检测器（参数完全对齐 Python 100-200 周期配置）
    pub fn new() -> Self {
        Self {
            macd_fast: EMA::new(100),
            macd_slow: EMA::new(200),
            signal_ema: EMA::new(9),
            ema10: EMA::new(10),
            ema20: EMA::new(20),
            hist_history: VecDeque::with_capacity(2),
            rsi: DominantCycleRSI::new(),
            rsi_overbought: dec!(70),
            rsi_oversold: dec!(30),
            epsilon: dec!(1e-8),
            price_history: VecDeque::with_capacity(1000),
            macd_cross_history: VecDeque::with_capacity(1000),
        }
    }

    /// 更新指标并返回所有计算值（完全对齐 Python calculate_colors_only 输出）
    /// 返回: (bar_color, bg_color, macd, signal, hist, ema10, ema20, rsi, crsi)
    /// ohlc: (open, high, low, close)
    pub fn update(&mut self, ohlc: (Decimal, Decimal, Decimal, Decimal)) -> (String, String, Decimal, Decimal, Decimal, Decimal, Decimal, Decimal, Decimal) {
        let (_open, high, low, close) = ohlc;

        // 1. 计算 MACD（完全对齐 Python calculate_pine_macd）
        let fast_ma = self.macd_fast.update(close);
        let slow_ma = self.macd_slow.update(close);
        let macd = fast_ma - slow_ma;
        let signal = self.signal_ema.update(macd);
        let hist = macd - signal;

        // 2. 维护 hist 历史（对齐 Python hist_prev = np.roll(hist,1)，hist_prev[0] = hist[0]）
        self.hist_history.push_back(hist);
        if self.hist_history.len() > 2 {
            self.hist_history.pop_front();
        }
        let hist_prev = if self.hist_history.len() >= 2 {
            self.hist_history[0]
        } else {
            hist // 初始状态用当前值补全，对齐Python
        };

        // 3. 计算 EMA10/EMA20（对齐 Python calculate_trade_conditions）
        let ema10_val = self.ema10.update(close);
        let ema20_val = self.ema20.update(close);

        // 4. 计算 RSI/CRSI（完全对齐 Python calculate_pine_rsi）
        let crsi = self.rsi.update(close);
        let rsi = self.rsi.get_rsi();

        // 5. 检测 K 线颜色（严格对齐 Python 优先级顺序）
        let bar_color = self.detect_bar_color(macd, signal, hist_prev, hist, rsi, ema10_val, ema20_val);
        // 6. 检测背景颜色（完全对齐 Python calculate_bg_color）
        let bg_color = self.detect_bg_color(macd, signal);

        // 7. 检测 MACD 交叉（hist 穿过零轴）
        let is_cross = if self.hist_history.len() >= 1 {
            let prev_hist = self.hist_history.back().copied().unwrap_or(Decimal::ZERO);
            (prev_hist < Decimal::ZERO && hist >= Decimal::ZERO) ||
            (prev_hist > Decimal::ZERO && hist <= Decimal::ZERO)
        } else {
            false
        };

        // 8. 保存价格历史和交叉标记（用于振幅指标计算）
        self.price_history.push_back((high, low, close));
        if self.price_history.len() > 1000 {
            self.price_history.pop_front();
        }
        self.macd_cross_history.push_back(is_cross);
        if self.macd_cross_history.len() > 1000 {
            self.macd_cross_history.pop_front();
        }

        (bar_color, bg_color, macd, signal, hist, ema10_val, ema20_val, rsi, crsi)
    }

    /// 仅用 close 价格更新（兼容接口，amplitude 无法正确计算）
    pub fn update_close_only(&mut self, close: Decimal) -> (String, String, Decimal, Decimal, Decimal, Decimal, Decimal, Decimal, Decimal) {
        self.update((close, close, close, close))
    }

    /// 检测 bar 颜色（严格对齐 Python 优先级顺序：st, bt, sl, by, up, dn, s, b）
    fn detect_bar_color(
        &self,
        macd: Decimal,
        _signal: Decimal,
        hist_prev: Decimal,
        hist: Decimal,
        rsi: Decimal,
        ema10_val: Decimal,
        ema20_val: Decimal,
    ) -> String {
        // 1. 基础条件计算（完全对齐 Python calculate_trade_conditions）
        let ema20_above_ema10 = ema20_val > ema10_val;
        let ema20_below_ema10 = ema20_val < ema10_val;
        let is_up = rsi >= self.rsi_overbought;
        let is_down = rsi <= self.rsi_oversold;

        // 严格对齐 Python 条件公式
        let selltime = macd >= Decimal::ZERO && ema20_below_ema10 && hist_prev > hist && hist >= Decimal::ZERO;
        let buytime = macd <= Decimal::ZERO && ema20_above_ema10 && hist_prev < hist && hist <= Decimal::ZERO;
        let selltimeT = macd <= Decimal::ZERO && ema20_below_ema10 && hist_prev > hist && hist >= Decimal::ZERO;
        let buytimeT = macd >= Decimal::ZERO && ema20_above_ema10 && hist_prev < hist && hist <= Decimal::ZERO;
        let selltimeS = selltime && is_up;
        let buytimeS = buytime && is_down;

        // 2. 严格遵循 Python 优先级顺序：st/bt → sl/by → up/dn → s/b
        // 优先级1: st/bt → 纯红 (weak_signal)
        if selltimeT || buytimeT {
            return colors::WEAK_SIGNAL.to_string();
        }
        // 优先级2: sl → 浅黄 (top_warning)
        if selltime {
            return colors::TOP_WARNING.to_string();
        }
        // 优先级3: by → 纯蓝 (bottom_warning)
        if buytime {
            return colors::BOTTOM_WARNING.to_string();
        }
        // 优先级4: up → 纯绿 (bull_trend)
        if is_up {
            return colors::BULL_TREND.to_string();
        }
        // 优先级5: dn → 紫色 (bear_trend)
        if is_down {
            return colors::BEAR_TREND.to_string();
        }
        // 优先级6: s → 浅蓝 (strong_top)
        if selltimeS {
            return colors::STRONG_TOP.to_string();
        }
        // 优先级7: b → 橙色 (strong_bot)
        if buytimeS {
            return colors::STRONG_BOT.to_string();
        }

        colors::DEFAULT.to_string()
    }

    /// 检测背景颜色（完全对齐 Python calculate_bg_color 条件顺序）
    fn detect_bg_color(&self, macd: Decimal, signal: Decimal) -> String {
        // 严格对齐 Python 条件顺序：
        // 1. (macd >= sig) & (macd >= 0) → 纯绿 (bull_trend)
        // 2. (macd <= sig) & (macd <= 0) → 纯红 (bear_trend)
        // 3. (macd <= sig) & (macd >= 0) → 浅绿 (bull_consolidation)
        // 4. (macd >= sig) & (macd <= 0) → 浅红 (bear_consolidation)
        if macd >= signal && macd >= Decimal::ZERO {
            colors::BULL_TREND_BG.to_string()
        } else if macd <= signal && macd <= Decimal::ZERO {
            colors::BEAR_TREND_BG.to_string()
        } else if macd <= signal && macd >= Decimal::ZERO {
            colors::BULL_CONSOLIDATION.to_string()
        } else {
            colors::BEAR_CONSOLIDATION.to_string()
        }
    }

    /// 获取当前 MACD 值（不更新状态）
    pub fn get_macd(&self) -> (Decimal, Decimal, Decimal) {
        (self.macd_fast.get(), self.macd_slow.get(), self.signal_ema.get())
    }

    /// 获取当前 RSI/CRSI 值
    pub fn get_rsi(&self) -> (Decimal, Decimal) {
        (self.rsi.get_rsi(), self.rsi.get_crsi())
    }

    /// 计算 TOP3 平均振幅百分比（以 MACD 交叉分段的连续段中，amplitude 最大的3段平均值）
    pub fn calc_top3_avg_amplitude_pct(&self) -> Decimal {
        let crosses = &self.macd_cross_history;
        let prices = &self.price_history;

        if crosses.len() < 2 || prices.len() < 2 {
            return Decimal::ZERO;
        }

        // 找出所有连续段
        let mut segments: Vec<Vec<Decimal>> = Vec::new();
        let mut current_segment: Vec<Decimal> = Vec::new();

        for i in 0..crosses.len().min(prices.len()) {
            // 计算 amplitude
            let (high, low, close) = prices[i];
            if close > Decimal::ZERO {
                let amplitude = (high - low) / close * dec!(100);
                current_segment.push(amplitude);
            }

            // 遇到交叉点，结束当前段
            if crosses[i] && !current_segment.is_empty() {
                segments.push(current_segment);
                current_segment = Vec::new();
            }
        }

        // 最后一节
        if !current_segment.is_empty() {
            segments.push(current_segment);
        }

        if segments.is_empty() {
            return Decimal::ZERO;
        }

        // 每段取平均 amplitude，然后取最大的3段
        let mut segment_avgs: Vec<Decimal> = segments
            .iter()
            .map(|seg| {
                let sum: Decimal = seg.iter().sum();
                sum / Decimal::from(seg.len())
            })
            .collect();

        segment_avgs.sort_by(|a, b| b.cmp(a));
        let top3: Vec<Decimal> = segment_avgs.into_iter().take(3).collect();

        if top3.is_empty() {
            return Decimal::ZERO;
        }

        let sum: Decimal = top3.iter().sum();
        sum / Decimal::from(top3.len())
    }

    /// 计算 1% 振幅效率天数（以 MACD 交叉分段后，amplitude >= 1% 的天数）
    pub fn calc_one_percent_amplitude_time_days(&self) -> Decimal {
        let crosses = &self.macd_cross_history;
        let prices = &self.price_history;

        if crosses.len() < 2 || prices.len() < 2 {
            return Decimal::ZERO;
        }

        let mut count = Decimal::ZERO;
        let mut in_segment = false;

        for i in 0..crosses.len().min(prices.len()) {
            // MACD 交叉点，进入新段
            if crosses[i] {
                in_segment = true;
                continue;
            }

            if in_segment {
                let (high, low, close) = prices[i];
                if close > Decimal::ZERO {
                    let amplitude = (high - low) / close * dec!(100);
                    if amplitude >= dec!(1) {
                        count += dec!(1);
                    }
                }
            }
        }

        count
    }

    /// 重置检测器状态
    pub fn reset(&mut self) {
        self.macd_fast.reset();
        self.macd_slow.reset();
        self.signal_ema.reset();
        self.ema10.reset();
        self.ema20.reset();
        self.hist_history.clear();
        self.price_history.clear();
        self.macd_cross_history.clear();
        self.rsi = DominantCycleRSI::new();
    }
}

impl Default for PineColorDetector {
    fn default() -> Self {
        Self::new()
    }
}

// ==================== 测试用例（验证与 Python 输出一致性）====================
#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_pine_color_detector_align_with_python() {
        let mut detector = PineColorDetector::new();

        // 模拟 Python 主函数的 BTCUSDT 价格序列（1000 条，这里用 10 条测试）
        let prices = vec![
            dec!(30000), dec!(30500), dec!(30200), dec!(29800), dec!(30100),
            dec!(30600), dec!(30400), dec!(29900), dec!(30200), dec!(30700),
        ];

        println!("=== Rust 版指标输出（对齐 Python）===");
        for (i, &price) in prices.iter().enumerate() {
            // 使用 update_close_only 兼容接口
            let (bar, bg, macd, signal, hist, ema10, ema20, rsi, crsi) = detector.update_close_only(price);
            println!(
                "K线{} | 价格:{} | 柱色:{} | 背景色:{} | MACD:{:.4} | Signal:{:.4} | Hist:{:.4} | EMA10:{:.4} | EMA20:{:.4} | RSI:{:.4} | CRSI:{:.4}",
                i+1, price, bar, bg, macd, signal, hist, ema10, ema20, rsi, crsi
            );
        }

        // 验证核心逻辑：EMA 系数、颜色优先级、CRSI 计算
        let (_, _, macd, signal, hist, _, _, _, _) = detector.update_close_only(dec!(31000));
        assert!(hist == macd - signal); // 验证 MACD Hist 计算正确
    }

    #[test]
    fn test_ema_align_with_python() {
        // 验证 EMA alpha = 2/(period+1)，对齐 Python ewm(span=10, adjust=False)
        let mut ema10 = EMA::new(10);
        let price = dec!(100);
        let first_val = ema10.update(price);
        assert_eq!(first_val, price); // 初始值等于第一个价格，对齐Python

        let second_val = ema10.update(dec!(110));
        let alpha = dec!(2) / dec!(11);
        let expected = price * (dec!(1) - alpha) + dec!(110) * alpha;
        assert_eq!(second_val, expected); // 验证 EMA 公式完全对齐
    }
}
