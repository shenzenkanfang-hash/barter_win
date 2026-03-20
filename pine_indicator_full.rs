//! Pine Script v5 指标完整实现
//!
//! 对应 Python pine_scripts.py 的完整实现
//! 使用 Python 的 alpha=1/period EMA 公式

use rust_decimal::Decimal;
use rust_decimal_macros::dec;

// ==================== 常量定义 ====================

/// Pine颜色定义 (匹配 Python PINE_COLOR_MAP)
pub mod colors {
    pub const STRONG_TOP: &'static str = "浅蓝";      // selltimeS
    pub const STRONG_BOT: &'static str = "橙色";      // buytimeS
    pub const TOP_WARNING: &'static str = "浅黄";     // selltime
    pub const BOTTOM_WARNING: &'static str = "纯蓝";  // buytime
    pub const WEAK_SIGNAL: &'static str = "纯红";     // selltimeT/buytimeT
    pub const BULL_TREND: &'static str = "纯绿";     // is_up (rsi >= 70)
    pub const BEAR_TREND: &'static str = "紫色";     // is_down (rsi <= 30)
    pub const DEFAULT: &'static str = "白色";         // default

    // BG颜色映射 (匹配 Python PINE_BG_COLOR_MAP)
    pub const BULL_TREND_BG: &'static str = "纯绿";      // macd >= signal && macd >= 0
    pub const BULL_CONSOLIDATION: &'static str = "浅绿";  // macd <= signal && macd >= 0
    pub const BEAR_TREND_BG: &'static str = "纯红";      // macd <= signal && macd <= 0
    pub const BEAR_CONSOLIDATION: &'static str = "浅红"; // macd >= signal && macd <= 0
    pub const DEFAULT_BG: &'static str = "白色";
}

// ==================== EMA ====================

/// EMA 指标
/// Python _sma_or_ema 使用 alpha = 1/period
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
            // Python _sma_or_ema uses: alpha = 1.0 / window
            let alpha = dec!(1) / Decimal::from(self.period);
            self.value = price * alpha + self.value * (dec!(1) - alpha);
        }
        self.value
    }

    fn get(&self) -> Decimal {
        self.value
    }
}

// ==================== RMA (RSI 平滑) ====================

/// RMA (RSI 平滑移动平均)
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

// ==================== Dominant Cycle RSI ====================

/// Dominant Cycle RSI
/// 对应 Python _vectorized_crsi
struct DominantCycleRSI {
    period: usize,
    cyclelen: usize,
    torque: Decimal,
    phasinglag: usize,
    rma_up: RMA,
    rma_down: RMA,
    crsi: Decimal,
    last_price: Decimal,
    rsi_history: Vec<Decimal>,  // 使用 Vec 而非 VecDeque
}

impl DominantCycleRSI {
    fn new(period: usize) -> Self {
        // Python: cyclelen = PINE_DOMCYCLE / 2 = 20 / 2 = 10
        let cyclelen = period / 2;
        // Python: torque = 2.0 / (VIBRATION + 1) = 2.0 / 11
        let torque = dec!(2) / Decimal::from(11);
        // Python: phasingLag = (VIBRATION - 1) / 2.0 = (10 - 1) / 2 = 4.5 -> 4
        let phasinglag = 4;

        Self {
            period,
            cyclelen,
            torque,
            phasinglag,
            rma_up: RMA::new(cyclelen),
            rma_down: RMA::new(cyclelen),
            crsi: Decimal::ZERO,
            last_price: Decimal::ZERO,
            rsi_history: Vec::new(),
        }
    }

    fn update(&mut self, price: Decimal) -> Decimal {
        // 计算价格变化
        let change = if self.last_price > Decimal::ZERO {
            price - self.last_price
        } else {
            Decimal::ZERO
        };
        self.last_price = price;

        // 计算 up/down
        let up = if change > Decimal::ZERO { change } else { -change };
        let down = if change < Decimal::ZERO { -change } else { Decimal::ZERO };

        // 计算基础 RSI
        let rsi = if down == Decimal::ZERO {
            dec!(100)
        } else if up == Decimal::ZERO {
            dec!(0)
        } else {
            dec!(100) - dec!(100) / (dec!(1) + up / down)
        };

        // 计算 RMA
        let rma_up_val = self.rma_up.update(up);
        let rma_down_val = self.rma_down.update(down);

        // 计算 RSI RMA
        let rsi_rma = if rma_down_val == Decimal::ZERO {
            dec!(100)
        } else if rma_up_val == Decimal::ZERO {
            dec!(0)
        } else {
            dec!(100) - dec!(100) / (dec!(1) + rma_up_val / rma_down_val)
        };

        self.rsi_history.push(rsi_rma);

        // 获取滞后的 RSI
        let rsi_lagged = self.get_rsi_lagged();

        // 计算 CRSI
        let crsi_calc = if let Some(rsi_lagged_val) = rsi_lagged {
            self.torque * (dec!(2) * rsi_rma - rsi_lagged_val) +
            (dec!(1) - self.torque) * self.crsi
        } else {
            self.torque * (dec!(2) * rsi_rma) +
            (dec!(1) - self.torque) * self.crsi
        };

        self.crsi = crsi_calc;
        self.crsi
    }

    fn get_rsi_lagged(&self) -> Option<Decimal> {
        let len = self.rsi_history.len();
        if len > self.phasinglag {
            // Python np.roll(rsi, phasingLag) 将数组向左移动 phasingLag 位
            // 所以 rsi_phasing[i] = rsi[i - phasingLag]
            // 在 Rust 中，rsi_history 的最新元素在末尾
            // 要获取滞后的值，需要 len - 1 - phasinglag
            self.rsi_history.get(len - 1 - self.phasinglag).copied()
        } else {
            None
        }
    }

    fn get_value(&self) -> Decimal {
        self.crsi
    }
}

// ==================== PineColorDetector ====================

/// Pine 颜色检测器
/// 对应 Python PineColorOnlyCalculator
pub struct PineColorDetector {
    macd_fast: EMA,   // fast=100, slow=200
    macd_slow: EMA,
    signal_ema: EMA,  // signal=9
    ema10: EMA,       // fixed 10
    ema20: EMA,       // fixed 20
    hist_prev: Option<Decimal>,
    rsi: DominantCycleRSI,
}

impl PineColorDetector {
    /// 创建新的检测器
    /// MACD 参数: fast=100, slow=200, signal=9
    /// EMA 参数: 10, 20
    /// RSI 参数: period=20, vibration=10
    pub fn new() -> Self {
        Self {
            macd_fast: EMA::new(100),
            macd_slow: EMA::new(200),
            signal_ema: EMA::new(9),
            ema10: EMA::new(10),
            ema20: EMA::new(20),
            hist_prev: None,
            rsi: DominantCycleRSI::new(20),
        }
    }

    /// 更新指标并返回所有计算值
    /// 返回: (bar_color, bg_color, macd, signal, hist, ema10, ema20, rsi)
    pub fn update(&mut self, close: Decimal) -> (String, String, Decimal, Decimal, Decimal, Decimal, Decimal, Decimal) {
        // 计算 MACD
        let fast_ma = self.macd_fast.update(close);
        let slow_ma = self.macd_slow.update(close);
        let macd = fast_ma - slow_ma;
        let signal = self.signal_ema.update(macd);
        let hist = macd - signal;

        // 计算 EMA10/EMA20
        let ema10_val = self.ema10.update(close);
        let ema20_val = self.ema20.update(close);

        // 计算 RSI
        let rsi_val = self.rsi.update(close);

        // 获取 hist_prev
        let hist_prev = self.hist_prev;
        self.hist_prev = Some(hist);

        // 检测颜色
        let bar_color = self.detect_bar_color(macd, signal, hist_prev, hist, rsi_val, ema10_val, ema20_val);
        let bg_color = self.detect_bg_color(macd, signal);

        (bar_color, bg_color, macd, signal, hist, ema10_val, ema20_val, rsi_val)
    }

    /// 获取 MACD 指标值（不更新状态）
    pub fn get_macd(&self) -> (Decimal, Decimal, Decimal) {
        (self.macd_fast.get(), self.macd_slow.get(), self.signal_ema.get())
    }

    /// 获取 RSI 值
    pub fn get_rsi(&self) -> Decimal {
        self.rsi.get_value()
    }

    /// 检测 bar 颜色
    /// 匹配 Python calculate_bar_color 和 calculate_trade_conditions
    fn detect_bar_color(
        &self,
        macd: Decimal,
        _signal: Decimal,
        hist_prev: Option<Decimal>,
        hist: Decimal,
        rsi: Decimal,
        ema10_val: Decimal,
        ema20_val: Decimal,
    ) -> String {
        let hist_prev_val = match hist_prev {
            Some(v) => v,
            None => return colors::DEFAULT.to_string(),
        };

        let ema20_above_ema10 = ema20_val > ema10_val;
        let ema20_below_ema10 = ema20_val < ema10_val;
        let is_up = rsi >= dec!(70);
        let is_down = rsi <= dec!(30);

        // Python 条件计算 (calculate_trade_conditions)
        // selltime = (macd >= 0) & (ema20 < ema10) & (hist_prev > hist) & (hist >= 0)
        // buytime = (macd <= 0) & (ema20 > ema10) & (hist_prev < hist) & (hist <= 0)
        // selltimeT = (macd <= 0) & (ema20 < ema10) & (hist_prev > hist) & (hist >= 0)
        // buytimeT = (macd >= 0) & (ema20 > ema10) & (hist_prev < hist) & (hist <= 0)
        // selltimeS = selltime & is_up
        // buytimeS = buytime & is_down

        let selltime = macd >= Decimal::ZERO && ema20_below_ema10 && hist_prev_val > hist && hist >= Decimal::ZERO;
        let buytime = macd <= Decimal::ZERO && ema20_above_ema10 && hist_prev_val < hist && hist <= Decimal::ZERO;
        let selltimeT = macd <= Decimal::ZERO && ema20_below_ema10 && hist_prev_val > hist && hist >= Decimal::ZERO;
        let buytimeT = macd >= Decimal::ZERO && ema20_above_ema10 && hist_prev_val < hist && hist <= Decimal::ZERO;
        let selltimeS = selltime && is_up;
        let buytimeS = buytime && is_down;

        // Python calculate_bar_color 优先级顺序 (conds 数组顺序):
        // [st, bt, sl, by, up, dn, s, b]
        // cols: [weak_signal, weak_signal, top_warning, bottom_warning, bull_trend, bear_trend, strong_top, strong_bot]

        // 优先级1-2: st/bt -> 纯红 (weak_signal)
        if selltimeT || buytimeT {
            return colors::WEAK_SIGNAL.to_string();
        }
        // 优先级3: sl -> 浅黄 (top_warning)
        if selltime {
            return colors::TOP_WARNING.to_string();
        }
        // 优先级4: by -> 纯蓝 (bottom_warning)
        if buytime {
            return colors::BOTTOM_WARNING.to_string();
        }
        // 优先级5: up -> 纯绿 (bull_trend)
        if is_up {
            return colors::BULL_TREND.to_string();
        }
        // 优先级6: dn -> 紫色 (bear_trend)
        if is_down {
            return colors::BEAR_TREND.to_string();
        }
        // 优先级7: s -> 浅蓝 (strong_top)
        if selltimeS {
            return colors::STRONG_TOP.to_string();
        }
        // 优先级8: b -> 橙色 (strong_bot)
        if buytimeS {
            return colors::STRONG_BOT.to_string();
        }

        colors::DEFAULT.to_string()
    }

    /// 检测背景颜色
    /// 匹配 Python calculate_bg_color
    fn detect_bg_color(&self, macd: Decimal, signal: Decimal) -> String {
        // Python 条件顺序:
        // (macd >= sig) & (macd >= 0) -> bull_trend (纯绿)
        // (macd <= sig) & (macd <= 0) -> bear_trend (纯红)
        // (macd <= sig) & (macd >= 0) -> bull_consolidation (浅绿)
        // (macd >= sig) & (macd <= 0) -> bear_consolidation (浅红)

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
}

impl Default for PineColorDetector {
    fn default() -> Self {
        Self::new()
    }
}

// ==================== 主函数示例 ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pine_color_detector_basic() {
        let mut detector = PineColorDetector::new();

        // 测试基本更新
        let (bar, bg, macd, signal, hist, ema10, ema20, rsi) = detector.update(dec!(30000));

        println!("First update: bar={}, bg={}", bar, bg);
        println!("MACD: macd={}, signal={}, hist={}", macd, signal, hist);
        println!("EMA: ema10={}, ema20={}", ema10, ema20);
        println!("RSI: {}", rsi);

        // 第一次更新时 hist_prev 为 None，应该返回 DEFAULT
        assert_eq!(bar, colors::DEFAULT);
    }

    #[test]
    fn test_pine_color_with_price_series() {
        let mut detector = PineColorDetector::new();

        // 模拟价格序列
        let prices = vec![
            dec!(30000),
            dec!(30500),
            dec!(30200),
            dec!(29800),
            dec!(30100),
            dec!(30600),
            dec!(30400),
            dec!(29900),
            dec!(30200),
            dec!(30700),
        ];

        for price in prices {
            let (bar, bg, _, _, _, _, _, _) = detector.update(price);
            println!("Price: {}, bar={}, bg={}", price, bar, bg);
        }
    }
}
