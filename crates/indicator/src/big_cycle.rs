use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// 大周期指标计算器
///
/// 计算日线级别的 TR Ratio、区间位置、PineColor 指标。
///
/// 设计依据:
/// - TR Ratio: big_cycle_calc.py 中的 tr_ratio_5d_20d, tr_ratio_20d_60d
/// - 区间位置: pos_norm_20, ma5_close_in_20d_ma5_pos, ma20_close_in_60d_ma20_pos
/// - PineColor: pine_scripts.py 中的三种参数组合 (100/200, 20/50, 12/26)

/// TR Ratio 信号
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TRRatioSignal {
    /// 极端波动
    Extreme,
    /// 高波动
    High,
    /// 正常
    Normal,
}

/// 大周期配置
#[derive(Debug, Clone)]
pub struct BigCycleConfig {
    /// 窗口大小
    pub window_5d: usize,
    pub window_20d: usize,
    pub window_60d: usize,
    /// TR Ratio 极端阈值
    pub tr_extreme_threshold: Decimal,
    /// TR Ratio 高阈值
    pub tr_high_threshold: Decimal,
    /// RSI 超买/超卖
    pub rsi_overbought: Decimal,
    pub rsi_oversold: Decimal,
}

impl Default for BigCycleConfig {
    fn default() -> Self {
        Self {
            window_5d: 5,
            window_20d: 20,
            window_60d: 60,
            tr_extreme_threshold: dec!(2.0),
            tr_high_threshold: dec!(1.5),
            rsi_overbought: dec!(70),
            rsi_oversold: dec!(30),
        }
    }
}

/// 大周期指标
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BigCycleIndicators {
    /// TR Ratio 5d/20d
    pub tr_ratio_5d_20d: Decimal,
    /// TR Ratio 20d/60d
    pub tr_ratio_20d_60d: Decimal,
    /// 20日区间位置 (0-100)
    pub pos_norm_20: Decimal,
    /// MA5 在 20日 MA5 区间的位置
    pub ma5_in_20d_ma5_pos: Decimal,
    /// MA20 在 60日 MA20 区间的位置
    pub ma20_in_60d_ma20_pos: Decimal,
    /// Pine 颜色 (100/200)
    pub pine_color_100_200: PineColorBig,
    /// Pine 颜色 (20/50)
    pub pine_color_20_50: PineColorBig,
    /// Pine 颜色 (12/26)
    pub pine_color_12_26: PineColorBig,
}

/// Pine 颜色枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PineColorBig {
    PureGreen,    // 纯绿
    LightGreen,   // 浅绿
    PureRed,      // 纯红
    LightRed,     // 浅红
    Purple,       // 紫色 (RSI 极值)
    Neutral,      // 中性
}

impl PineColorBig {
    /// 判断是否为绿色系
    pub fn is_green(&self) -> bool {
        matches!(self, PineColorBig::PureGreen | PineColorBig::LightGreen)
    }

    /// 判断是否为红色系
    pub fn is_red(&self) -> bool {
        matches!(self, PineColorBig::PureRed | PineColorBig::LightRed)
    }
}

/// 大周期指标计算器
pub struct BigCycleCalculator {
    config: BigCycleConfig,

    // 价格历史
    high_history: VecDeque<Decimal>,
    low_history: VecDeque<Decimal>,
    close_history: VecDeque<Decimal>,

    // 中间值缓存
    mid_ma10_cache: VecDeque<Decimal>,

    // 预计算的极值
    high_5d_max: Decimal,
    low_5d_min: Decimal,
    high_20d_max: Decimal,
    low_20d_min: Decimal,
    high_60d_max: Decimal,
    low_60d_min: Decimal,

    // MA 缓存
    ma5_close: Decimal,
    ma20_close: Decimal,
    ma5_in_20d_max: Decimal,
    ma5_in_20d_min: Decimal,
    ma20_in_60d_max: Decimal,
    ma20_in_60d_min: Decimal,

    // TR 历史
    tr_base_5d_history: VecDeque<Decimal>,
    tr_base_20d_history: VecDeque<Decimal>,
    tr_5d_avg: Decimal,
    tr_20d_avg: Decimal,
    tr_60d_avg: Decimal,

    // EMA 缓存 (用于 PineColor)
    ema_fast_100_200: Decimal,
    ema_slow_100_200: Decimal,
    ema_fast_20_50: Decimal,
    ema_slow_20_50: Decimal,
    ema_fast_12_26: Decimal,
    ema_slow_12_26: Decimal,

    // RSI 缓存
    rsi_100_200: Decimal,
    rsi_20_50: Decimal,
    rsi_12_26: Decimal,
}

impl Default for BigCycleCalculator {
    fn default() -> Self {
        Self::new()
    }
}

impl BigCycleCalculator {
    /// 创建大周期计算器
    pub fn new() -> Self {
        let config = BigCycleConfig::default();
        Self {
            config,
            high_history: VecDeque::with_capacity(100),
            low_history: VecDeque::with_capacity(100),
            close_history: VecDeque::with_capacity(100),
            mid_ma10_cache: VecDeque::with_capacity(20),
            high_5d_max: dec!(0),
            low_5d_min: dec!(0),
            high_20d_max: dec!(0),
            low_20d_min: dec!(0),
            high_60d_max: dec!(0),
            low_60d_min: dec!(0),
            ma5_close: dec!(0),
            ma20_close: dec!(0),
            ma5_in_20d_max: dec!(0),
            ma5_in_20d_min: dec!(0),
            ma20_in_60d_max: dec!(0),
            ma20_in_60d_min: dec!(0),
            tr_base_5d_history: VecDeque::with_capacity(100),
            tr_base_20d_history: VecDeque::with_capacity(200),
            tr_5d_avg: dec!(0),
            tr_20d_avg: dec!(0),
            tr_60d_avg: dec!(0),
            ema_fast_100_200: dec!(0),
            ema_slow_100_200: dec!(0),
            ema_fast_20_50: dec!(0),
            ema_slow_20_50: dec!(0),
            ema_fast_12_26: dec!(0),
            ema_slow_12_26: dec!(0),
            rsi_100_200: dec!(50),
            rsi_20_50: dec!(50),
            rsi_12_26: dec!(50),
        }
    }

    /// 更新价格数据
    pub fn update(&mut self, high: Decimal, low: Decimal, close: Decimal) {
        // 更新历史
        self.high_history.push_back(high);
        self.low_history.push_back(low);
        self.close_history.push_back(close);

        // 保持窗口大小
        if self.high_history.len() > 100 {
            self.high_history.pop_front();
            self.low_history.pop_front();
            self.close_history.pop_front();
        }

        // 更新 MA
        self.update_ma();

        // 更新极值
        self.update_extremes();

        // 更新 TR
        self.update_tr();
    }

    /// 更新移动平均
    fn update_ma(&mut self) {
        let n = self.close_history.len();

        // MA5
        if n >= 5 {
            let sum: Decimal = self.close_history.iter().rev().take(5).sum();
            self.ma5_close = sum / dec!(5);
        }

        // MA20
        if n >= 20 {
            let sum: Decimal = self.close_history.iter().rev().take(20).sum();
            self.ma20_close = sum / dec!(20);
        }

        // Mid MA10 (用于 PineColor)
        if n >= 10 {
            let recent: Vec<_> = self.high_history.iter().rev().take(10).cloned().collect();
            let mid_sum: Decimal = recent.iter().map(|&h| {
                let idx = self.high_history.iter().position(|&x| x == h).unwrap_or(0);
                let low_val = self.low_history[idx];
                (h + low_val) / dec!(2)
            }).sum();
            self.mid_ma10_cache.push_back(mid_sum / dec!(10));
            if self.mid_ma10_cache.len() > 10 {
                self.mid_ma10_cache.pop_front();
            }
        }
    }

    /// 更新极值
    fn update_extremes(&mut self) {
        let n = self.high_history.len();

        // 5d 极值
        if n >= 5 {
            self.high_5d_max = self.high_history.iter().rev().take(5).cloned().max().unwrap_or(dec!(0));
            self.low_5d_min = self.low_history.iter().rev().take(5).cloned().min().unwrap_or(dec!(0));
        }

        // 20d 极值
        if n >= 20 {
            self.high_20d_max = self.high_history.iter().rev().take(20).cloned().max().unwrap_or(dec!(0));
            self.low_20d_min = self.low_history.iter().rev().take(20).cloned().min().unwrap_or(dec!(0));

            // MA5 在 20 日 MA5 区间
            let ma5_recent: Vec<_> = self.close_history.iter().rev().take(20).cloned().collect();
            let ma5_values: Vec<Decimal> = (0..=15).filter_map(|i| {
                if i + 5 <= ma5_recent.len() {
                    Some(ma5_recent[i..i+5].iter().sum::<Decimal>() / dec!(5))
                } else {
                    None
                }
            }).collect();
            if !ma5_values.is_empty() {
                self.ma5_in_20d_max = *ma5_values.iter().max().unwrap_or(&dec!(0));
                self.ma5_in_20d_min = *ma5_values.iter().min().unwrap_or(&dec!(0));
            }
        }

        // 60d 极值
        if n >= 60 {
            self.high_60d_max = self.high_history.iter().rev().take(60).cloned().max().unwrap_or(dec!(0));
            self.low_60d_min = self.low_history.iter().rev().take(60).cloned().min().unwrap_or(dec!(0));

            // MA20 在 60 日 MA20 区间
            let ma20_values: Vec<Decimal> = (0..=40).filter_map(|i| {
                if i + 20 <= self.close_history.len() {
                    let slice: Vec<_> = self.close_history.iter().rev().skip(i).take(20).cloned().collect();
                    Some(slice.iter().sum::<Decimal>() / dec!(20))
                } else {
                    None
                }
            }).collect();
            if !ma20_values.is_empty() {
                self.ma20_in_60d_max = *ma20_values.iter().max().unwrap_or(&dec!(0));
                self.ma20_in_60d_min = *ma20_values.iter().min().unwrap_or(&dec!(0));
            }
        }
    }

    /// 更新 TR 指标
    fn update_tr(&mut self) {
        let n = self.close_history.len();
        if n < 2 {
            return;
        }

        // 获取 N 天前的收盘价作为锚点
        let close_prev_5d = if n >= 5 {
            self.close_history[n - 5]
        } else {
            self.close_history[0]
        };

        let close_prev_20d = if n >= 20 {
            self.close_history[n - 20]
        } else {
            self.close_history[0]
        };

        // TR Base 5d
        if self.high_5d_max > dec!(0) && close_prev_5d > dec!(0) {
            let tr_5d = (self.high_5d_max - self.low_5d_min) / close_prev_5d;
            self.tr_base_5d_history.push_back(tr_5d);
            if self.tr_base_5d_history.len() > 100 {
                self.tr_base_5d_history.pop_front();
            }

            // TR 5d 均值
            let sum: Decimal = self.tr_base_5d_history.iter().sum();
            self.tr_5d_avg = sum / Decimal::from(self.tr_base_5d_history.len());
        }

        // TR Base 20d
        if self.high_20d_max > dec!(0) && close_prev_20d > dec!(0) {
            let tr_20d = (self.high_20d_max - self.low_20d_min) / close_prev_20d;
            self.tr_base_20d_history.push_back(tr_20d);
            if self.tr_base_20d_history.len() > 200 {
                self.tr_base_20d_history.pop_front();
            }

            // TR 20d 均值
            let sum: Decimal = self.tr_base_20d_history.iter().sum();
            self.tr_20d_avg = sum / Decimal::from(self.tr_base_20d_history.len());
        }

        // TR 60d (基于 20d 的移动均值)
        if self.tr_base_20d_history.len() >= 60 {
            let sum: Decimal = self.tr_base_20d_history.iter().rev().take(60).sum();
            self.tr_60d_avg = sum / dec!(60);
        }
    }

    /// 计算 TR Ratio
    pub fn calculate_tr_ratio(&self) -> (Decimal, Decimal) {
        let tr_ratio_5d_20d = if self.tr_20d_avg > dec!(0) {
            let current_tr = self.tr_base_5d_history.back().copied().unwrap_or(dec!(0));
            current_tr / self.tr_20d_avg
        } else {
            dec!(0)
        };

        let tr_ratio_20d_60d = if self.tr_60d_avg > dec!(0) {
            let current_tr_20d = self.tr_base_20d_history.back().copied().unwrap_or(dec!(0));
            current_tr_20d / self.tr_60d_avg
        } else {
            dec!(0)
        };

        (tr_ratio_5d_20d, tr_ratio_20d_60d)
    }

    /// 计算 TR Ratio 信号
    pub fn tr_ratio_signal(&self) -> TRRatioSignal {
        let (ratio_5d_20d, _) = self.calculate_tr_ratio();

        if ratio_5d_20d >= self.config.tr_extreme_threshold {
            TRRatioSignal::Extreme
        } else if ratio_5d_20d >= self.config.tr_high_threshold {
            TRRatioSignal::High
        } else {
            TRRatioSignal::Normal
        }
    }

    /// 计算 20 日区间位置
    pub fn calculate_pos_norm_20(&self) -> Decimal {
        if self.high_20d_max <= self.low_20d_min {
            return dec!(50);
        }

        if let Some(&current_close) = self.close_history.back() {
            let range = self.high_20d_max - self.low_20d_min;
            let pos = (current_close - self.low_20d_min) / range * dec!(100);
            return pos.max(dec!(0)).min(dec!(100));
        }

        dec!(50)
    }

    /// 计算 MA5 在 20 日 MA5 区间的位置
    pub fn calculate_ma5_in_20d_ma5_pos(&self) -> Decimal {
        if self.ma5_in_20d_max <= self.ma5_in_20d_min {
            return dec!(50);
        }

        let range = self.ma5_in_20d_max - self.ma5_in_20d_min;
        let pos = (self.ma5_close - self.ma5_in_20d_min) / range * dec!(100);
        pos.max(dec!(0)).min(dec!(100))
    }

    /// 计算 MA20 在 60 日 MA20 区间的位置
    pub fn calculate_ma20_in_60d_ma20_pos(&self) -> Decimal {
        if self.ma20_in_60d_max <= self.ma20_in_60d_min {
            return dec!(50);
        }

        let range = self.ma20_in_60d_max - self.ma20_in_60d_min;
        let pos = (self.ma20_close - self.ma20_in_60d_min) / range * dec!(100);
        pos.max(dec!(0)).min(dec!(100))
    }

    /// 更新 PineColor EMA
    pub fn update_pine_ema(&mut self, high: Decimal, low: Decimal, close: Decimal) {
        let mid = (high + low) / dec!(2);

        // EMA 100/200
        self.ema_fast_100_200 = self.ema(self.ema_fast_100_200, mid, 100);
        self.ema_slow_100_200 = self.ema(self.ema_slow_100_200, mid, 200);

        // EMA 20/50
        self.ema_fast_20_50 = self.ema(self.ema_fast_20_50, mid, 20);
        self.ema_slow_20_50 = self.ema(self.ema_slow_20_50, mid, 50);

        // EMA 12/26
        self.ema_fast_12_26 = self.ema(self.ema_fast_12_26, mid, 12);
        self.ema_slow_12_26 = self.ema(self.ema_slow_12_26, mid, 26);

        // 更新 RSI (简化版)
        self.update_rsi(close);
    }

    /// EMA 计算
    fn ema(&self, prev: Decimal, current: Decimal, period: usize) -> Decimal {
        if prev == dec!(0) {
            return current;
        }
        let alpha = dec!(2) / Decimal::from(period + 1);
        current * alpha + prev * (dec!(1) - alpha)
    }

    /// 更新 RSI (简化)
    fn update_rsi(&mut self, close: Decimal) {
        // 简化 RSI 计算，使用固定平滑
        let rsi_period = 14;
        let n = self.close_history.len();

        if n > rsi_period {
            let recent: Vec<_> = self.close_history.iter().rev().take(rsi_period + 1).cloned().collect();
            let mut gains = dec!(0);
            let mut losses = dec!(0);

            for i in 1..recent.len() {
                let diff = recent[i - 1] - recent[i];
                if diff > dec!(0) {
                    gains += diff;
                } else {
                    losses += diff.abs();
                }
            }

            if losses > dec!(0) {
                let rs = gains / losses;
                let rsi = dec!(100) - dec!(100) / (dec!(1) + rs);
                // 三种参数组合使用不同的 RSI 值 (简化)
                self.rsi_100_200 = rsi;
                self.rsi_20_50 = rsi;
                self.rsi_12_26 = rsi;
            }
        }
    }

    /// 检测 PineColor (100/200)
    pub fn detect_pine_color_100_200(&self) -> PineColorBig {
        self.detect_pine_color(
            self.ema_fast_100_200,
            self.ema_slow_100_200,
            self.rsi_100_200,
        )
    }

    /// 检测 PineColor (20/50)
    pub fn detect_pine_color_20_50(&self) -> PineColorBig {
        self.detect_pine_color(
            self.ema_fast_20_50,
            self.ema_slow_20_50,
            self.rsi_20_50,
        )
    }

    /// 检测 PineColor (12/26)
    pub fn detect_pine_color_12_26(&self) -> PineColorBig {
        self.detect_pine_color(
            self.ema_fast_12_26,
            self.ema_slow_12_26,
            self.rsi_12_26,
        )
    }

    /// PineColor 检测核心逻辑
    fn detect_pine_color(&self, ema_fast: Decimal, ema_slow: Decimal, rsi: Decimal) -> PineColorBig {
        // RSI 极值优先
        if rsi >= self.config.rsi_overbought || rsi <= self.config.rsi_oversold {
            return PineColorBig::Purple;
        }

        // MACD 判断
        let macd = ema_fast - ema_slow;

        if macd >= Decimal::ZERO && ema_fast >= ema_slow {
            PineColorBig::PureGreen
        } else if macd <= Decimal::ZERO && ema_fast >= ema_slow {
            PineColorBig::LightGreen
        } else if macd <= Decimal::ZERO && ema_fast <= ema_slow {
            PineColorBig::PureRed
        } else {
            PineColorBig::LightRed
        }
    }

    /// 计算所有大周期指标
    pub fn calculate(&mut self, high: Decimal, low: Decimal, close: Decimal) -> BigCycleIndicators {
        self.update(high, low, close);
        self.update_pine_ema(high, low, close);

        BigCycleIndicators {
            tr_ratio_5d_20d: self.calculate_tr_ratio().0,
            tr_ratio_20d_60d: self.calculate_tr_ratio().1,
            pos_norm_20: self.calculate_pos_norm_20(),
            ma5_in_20d_ma5_pos: self.calculate_ma5_in_20d_ma5_pos(),
            ma20_in_60d_ma20_pos: self.calculate_ma20_in_60d_ma20_pos(),
            pine_color_100_200: self.detect_pine_color_100_200(),
            pine_color_20_50: self.detect_pine_color_20_50(),
            pine_color_12_26: self.detect_pine_color_12_26(),
        }
    }

    /// 获取当前价格
    pub fn current_price(&self) -> Option<Decimal> {
        self.close_history.back().copied()
    }

    /// 获取窗口数据量
    pub fn len(&self) -> usize {
        self.close_history.len()
    }

    /// 检查是否有足够数据
    pub fn is_ready(&self) -> bool {
        self.close_history.len() >= 60
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_big_cycle_basic() {
        let mut calc = BigCycleCalculator::new();

        // 模拟 60 天数据
        for i in 0..60 {
            let base = dec!(100) + Decimal::from(i);
            let high = base + dec!(2);
            let low = base - dec!(2);
            let close = base;
            calc.update(high, low, close);
        }

        assert!(calc.is_ready());
        assert!(calc.current_price().is_some());
    }

    #[test]
    fn test_tr_ratio() {
        let mut calc = BigCycleCalculator::new();

        // 喂入波动递增的价格
        for i in 0..30 {
            let volatility = Decimal::from(i) * dec!(0.1);
            let base = dec!(100);
            let high = base + dec!(5) + volatility;
            let low = base - dec!(5) - volatility;
            let close = base + volatility / dec!(2);
            calc.update(high, low, close);
        }

        let (ratio_5d_20d, ratio_20d_60d) = calc.calculate_tr_ratio();
        println!("TR Ratio 5d/20d: {}", ratio_5d_20d);
        println!("TR Ratio 20d/60d: {}", ratio_20d_60d);
    }

    #[test]
    fn test_position() {
        let mut calc = BigCycleCalculator::new();

        // 喂入 60 天价格
        for i in 0..60 {
            let base = dec!(100) + Decimal::from(i);
            calc.update(base + dec!(2), base - dec!(2), base);
        }

        let pos = calc.calculate_pos_norm_20();
        assert!(pos >= dec!(0) && pos <= dec!(100));
    }

    #[test]
    fn test_pine_color() {
        let mut calc = BigCycleCalculator::new();

        // 模拟上涨趋势
        for i in 0..100 {
            let price = dec!(100) + Decimal::from(i) / dec!(10);
            let high = price + dec!(1);
            let low = price - dec!(1);
            calc.update(high, low, price);
            calc.update_pine_ema(high, low, price);
        }

        let color = calc.detect_pine_color_20_50();
        println!("Pine Color: {:?}", color);
    }
}
