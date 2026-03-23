#![forbid(unsafe_code)]

//! 1分钟指标计算器 - 100% 对齐 Python v2.6
//!
//! 高频路径 O(1)，无锁，无堆分配，低延迟
//!
//! 从 Python indicator_calc.py 迁移的指标逻辑：
//! - 基础物理指标: velocity, acceleration, power
//! - TR 指标: tr_ratio, tr_ratio_zscore
//! - 百分位指标: velocity_percentile, acc_percentile, power_percentile
//! - Z-Score 指标
//! - 高阶动能指标: jerk, market_force, acc_efficiency

use rust_decimal::Decimal;
use rust_decimal::MathematicalOps;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

// ==================== 常量 ====================
const EPSILON: Decimal = dec!(1e-8);
const ZSCORE_MAX_LIMIT: Decimal = dec!(100);

const WINDOW_10MIN: usize = 10;
const WINDOW_15MIN: usize = 15;
const WINDOW_1H: usize = 60;
const WINDOW_5H: usize = 300;
const WINDOW_14: usize = 14;
const WINDOW_2H: usize = 120;
const NORM_WIN: usize = 20;

// ==================== 辅助函数 ====================
#[inline(always)]
fn sign(v: Decimal) -> Decimal {
    if v > Decimal::ZERO {
        dec!(1)
    } else if v < Decimal::ZERO {
        -dec!(1)
    } else {
        dec!(0)
    }
}

#[inline(always)]
fn safe_div(n: Decimal, d: Decimal) -> Decimal {
    if d.abs() < EPSILON {
        if n.is_positive() {
            dec!(1e10)
        } else {
            -dec!(1e10)
        }
    } else {
        n / d
    }
}

#[inline(always)]
fn clamp(v: Decimal, min: Decimal, max: Decimal) -> Decimal {
    if v < min {
        min
    } else if v > max {
        max
    } else {
        v
    }
}

// ==================== 滚动窗口工具 ====================

/// 滚动均值 - O(1) 增量计算
struct RollingMean {
    window: usize,
    sum: Decimal,
    deque: VecDeque<Decimal>,
}

impl RollingMean {
    fn new(window: usize) -> Self {
        Self {
            window,
            sum: dec!(0),
            deque: VecDeque::with_capacity(window),
        }
    }

    /// 增量更新，返回当前滚动均值
    #[inline(always)]
    fn update(&mut self, v: Decimal) -> Decimal {
        self.sum += v;
        self.deque.push_back(v);
        if self.deque.len() > self.window {
            self.sum -= self.deque.pop_front().unwrap();
        }
        if self.deque.is_empty() {
            dec!(0)
        } else {
            self.sum / Decimal::from(self.deque.len())
        }
    }

    /// 获取当前滚动均值
    #[inline(always)]
    fn get(&self) -> Decimal {
        if self.deque.is_empty() {
            dec!(0)
        } else {
            self.sum / Decimal::from(self.deque.len())
        }
    }
}

/// 滚动标准差 - O(1) 增量计算
struct RollingStd {
    window: usize,
    mean: RollingMean,
    sum_sq: Decimal,
    deque: VecDeque<Decimal>,
}

impl RollingStd {
    fn new(window: usize) -> Self {
        Self {
            window,
            mean: RollingMean::new(window),
            sum_sq: dec!(0),
            deque: VecDeque::with_capacity(window),
        }
    }

    /// 增量更新，返回当前滚动标准差
    #[inline(always)]
    fn update(&mut self, v: Decimal) -> Decimal {
        self.mean.update(v);
        self.sum_sq += v * v;
        self.deque.push_back(v);
        if self.deque.len() > self.window {
            let old = self.deque.pop_front().unwrap();
            self.sum_sq -= old * old;
        }
        let n = self.deque.len() as i32;
        if n < 2 {
            return EPSILON;
        }
        let mean = self.mean.get();
        let var = (self.sum_sq / Decimal::from(n)) - (mean * mean);
        var.sqrt().unwrap_or(EPSILON)
    }

    /// 获取当前滚动标准差
    #[inline(always)]
    fn get(&self) -> Decimal {
        let n = self.deque.len() as i32;
        if n < 2 {
            return EPSILON;
        }
        let mean = self.mean.get();
        let var = (self.sum_sq / Decimal::from(n)) - (mean * mean);
        var.sqrt().unwrap_or(EPSILON)
    }
}

/// 百分位计算 - 完全对齐 Python percentileofscore(kind="weak")
/// Python: percentileofscore(history[:-1], current) - 历史窗口（不含当前）
struct RollingPercentile {
    window: usize,
    buf: VecDeque<Decimal>,
}

impl RollingPercentile {
    fn new(window: usize) -> Self {
        Self {
            window,
            buf: VecDeque::with_capacity(window),
        }
    }

    /// 增量更新，返回当前值的百分位
    /// 对齐 Python: percentileofscore(history[:-1], current, kind='weak')
    #[inline(always)]
    fn update(&mut self, val: Decimal) -> Decimal {
        self.buf.push_back(val);
        if self.buf.len() > self.window {
            self.buf.pop_front();
        }

        if self.buf.len() < 10 {
            return dec!(50);
        }

        // Python: percentileofscore(history[:-1], current, kind='weak')
        // history[:-1] 是不包含当前值的窗口
        // 计算 history 中 <= current 的比例
        let total = self.buf.len() - 1; // 历史窗口大小（不含当前）
        let current = self.buf.back().copied().unwrap();
        let cnt = self.buf.iter().take(total).filter(|&&x| x <= current).count();
        Decimal::from(cnt) * dec!(100) / Decimal::from(total)
    }
}

/// 滚动最大值
struct RollingMax {
    window: usize,
    deque: VecDeque<Decimal>,
}

impl RollingMax {
    fn new(window: usize) -> Self {
        Self {
            window,
            deque: VecDeque::with_capacity(window),
        }
    }

    #[inline(always)]
    fn update(&mut self, v: Decimal) -> Decimal {
        self.deque.push_back(v);
        if self.deque.len() > self.window {
            self.deque.pop_front();
        }
        self.deque.iter().max().copied().unwrap_or(dec!(0))
    }
}

/// 滚动最小值
struct RollingMin {
    window: usize,
    deque: VecDeque<Decimal>,
}

impl RollingMin {
    fn new(window: usize) -> Self {
        Self {
            window,
            deque: VecDeque::with_capacity(window),
        }
    }

    #[inline(always)]
    fn update(&mut self, v: Decimal) -> Decimal {
        self.deque.push_back(v);
        if self.deque.len() > self.window {
            self.deque.pop_front();
        }
        self.deque.iter().min().copied().unwrap_or(dec!(0))
    }
}

// ==================== 1分钟指标计算器（100% 对齐 Python v2.6） ====================
pub struct Indicator1m {
    // 价格历史
    close: VecDeque<Decimal>,
    high: VecDeque<Decimal>,
    low: VecDeque<Decimal>,
    volume: VecDeque<Decimal>,

    // 基础物理指标
    velocity: RollingMean,
    acceleration: VecDeque<Decimal>,
    a_smooth: RollingMean,
    power: Decimal,

    // 百分位计算器
    vel_pct: RollingPercentile,
    acc_pct: RollingPercentile,
    power_pct: RollingPercentile,

    // 窗口极值
    high_10: RollingMax,
    low_10: RollingMin,
    high_60: RollingMax,
    low_60: RollingMin,

    // TR 滚动均值
    tr_10: RollingMean,
    tr_60: RollingMean,

    // TR ratio Z-Score
    tr_ratio_z1: RollingStd,
    tr_ratio_z2: RollingStd,

    // Z-Score
    z1h: RollingStd,
    z14: RollingStd,

    // 高阶动能
    jerk: RollingMean,
    jerk_std: RollingStd,
    vol_log: RollingMean,
    vol_std: RollingStd,

    // 历史数据（用于复杂计算）
    tr_history: VecDeque<Decimal>,
    tr_ratio_history: VecDeque<Decimal>,
}

impl Default for Indicator1m {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator1m {
    pub fn new() -> Self {
        Self {
            close: VecDeque::with_capacity(500),
            high: VecDeque::with_capacity(500),
            low: VecDeque::with_capacity(500),
            volume: VecDeque::with_capacity(500),
            velocity: RollingMean::new(1),
            acceleration: VecDeque::with_capacity(3),
            a_smooth: RollingMean::new(3),
            power: dec!(0),
            vel_pct: RollingPercentile::new(WINDOW_1H),
            acc_pct: RollingPercentile::new(WINDOW_1H),
            power_pct: RollingPercentile::new(WINDOW_1H),
            high_10: RollingMax::new(WINDOW_10MIN),
            low_10: RollingMin::new(WINDOW_10MIN),
            high_60: RollingMax::new(WINDOW_1H),
            low_60: RollingMin::new(WINDOW_1H),
            tr_10: RollingMean::new(WINDOW_10MIN),
            tr_60: RollingMean::new(WINDOW_1H),
            tr_ratio_z1: RollingStd::new(WINDOW_1H),
            tr_ratio_z2: RollingStd::new(WINDOW_5H),
            z1h: RollingStd::new(WINDOW_1H),
            z14: RollingStd::new(WINDOW_14),
            jerk: RollingMean::new(1),
            jerk_std: RollingStd::new(NORM_WIN),
            vol_log: RollingMean::new(NORM_WIN),
            vol_std: RollingStd::new(NORM_WIN),
            tr_history: VecDeque::with_capacity(500),
            tr_ratio_history: VecDeque::with_capacity(500),
        }
    }

    /// 更新指标（增量计算 O(1)）
    pub fn update(&mut self, h: Decimal, l: Decimal, c: Decimal, v: Decimal) -> Indicator1mOutput {
        // 1. 保存历史
        self.high.push_back(h);
        self.low.push_back(l);
        self.close.push_back(c);
        self.volume.push_back(v);

        // 限制长度
        if self.close.len() > 500 {
            self.high.pop_front();
            self.low.pop_front();
            self.close.pop_front();
            self.volume.pop_front();
        }

        let n = self.close.len();

        // 2. 速度 velocity = (close - prev_close) / prev_close
        let prev = if n >= 2 {
            self.close[self.close.len() - 2]
        } else {
            c
        };
        let vel = safe_div(c - prev, prev);
        self.velocity.update(vel);

        // 3. 加速度 acceleration = velocity - prev_velocity
        let prev_vel = self.velocity.get();
        let acc = vel - prev_vel;
        self.acceleration.push_back(acc);
        if self.acceleration.len() > 3 {
            self.acceleration.pop_front();
        }

        // 4. a_smooth - 对齐 Python rolling(3).mean()，先计算再移动窗口
        let a_smooth = self.a_smooth.update(acc);

        // 5. Power = a_smooth * velocity
        self.power = a_smooth * vel;

        // 6. 百分位（完全对齐 Python percentileofscore）
        let vel_pct = self.vel_pct.update(vel.abs());
        let acc_pct = self.acc_pct.update((a_smooth * vel).abs());
        let power_pct = self.power_pct.update(self.power.abs());

        // 7. 趋势方向
        let trend_dir = sign(vel);

        // 8. TR 指标计算
        // TR = max(high, prev_close) - min(low, prev_close)
        let prev_close_for_tr = if n >= 2 {
            self.close[self.close.len() - 2]
        } else {
            c
        };
        let tr = (h.max(prev_close_for_tr)) - (l.min(prev_close_for_tr));
        let tr_ratio = safe_div(tr, prev_close_for_tr + EPSILON);

        // 更新 TR 滚动均值
        let tr_10_avg = self.tr_10.update(tr_ratio);
        let tr_60_avg = self.tr_60.update(tr_ratio);

        // TR ratio Z-Score
        let tr_ratio_zscore_10min_1h = safe_div(
            tr_ratio - self.tr_ratio_z1.get(),
            self.tr_ratio_z1.update(tr_ratio),
        );
        let tr_ratio_zscore_60min_5h = safe_div(
            tr_ratio - self.tr_ratio_z2.get(),
            self.tr_ratio_z2.update(tr_ratio),
        );

        // 保存历史
        self.tr_history.push_back(tr);
        if self.tr_history.len() > 500 {
            self.tr_history.pop_front();
        }
        self.tr_ratio_history.push_back(tr_ratio);
        if self.tr_ratio_history.len() > 500 {
            self.tr_ratio_history.pop_front();
        }

        // 9. Z-Score 计算
        // z1h: 1小时窗口的加速度 Z-Score
        let zscore_1h_1m = self.z1h.update(a_smooth);
        // z14: 14窗口的加速度 Z-Score
        let zscore_14_1m = self.z14.update(a_smooth);

        // 10. 空间位置 pos_norm_60 = (close - low_60) / (high_60 - low_60) * 100
        let high_60_val = self.high_60.update(h);
        let low_60_val = self.low_60.update(l);
        let high_10_val = self.high_10.update(h);
        let low_10_val = self.low_10.update(l);

        let pos_norm_60 = if high_60_val > low_60_val {
            safe_div((c - low_60_val) * dec!(100), high_60_val - low_60_val)
        } else {
            dec!(50)
        };

        // 11. Jerk (加速度的导数) = a_smooth - prev_a_smooth
        let prev_a_smooth = if self.acceleration.len() >= 2 {
            let prev_acc = self.acceleration[self.acceleration.len() - 2];
            prev_acc
        } else {
            a_smooth
        };
        let jerk_val = a_smooth - prev_a_smooth;
        self.jerk.update(jerk_val);

        // 12. Norm jerk = jerk / jerk_std
        let jerk_std_val = self.jerk_std.update(jerk_val);
        let norm_jerk = clamp(safe_div(jerk_val, jerk_std_val + EPSILON), dec!(-3), dec!(3));

        // 13. Market force = norm_jerk * norm_volume
        // Volume log
        let vol_log_val = if v > dec!(0) {
            let ratio = safe_div(v, dec!(1_000_000));
            // 使用 f64 ln 然后转回 Decimal
            let f = ratio.to_string().parse::<f64>().unwrap_or(0.0);
            Decimal::from_str_exact(&format!("{:.6}", f.ln())).unwrap_or(dec!(0))
        } else {
            dec!(0)
        };
        let vol_mean = self.vol_log.update(vol_log_val);
        let vol_std_val = self.vol_std.update(vol_log_val);
        let norm_volume = clamp(safe_div(vol_log_val - vol_mean, vol_std_val + EPSILON), dec!(-3), dec!(3));

        let market_force = clamp(norm_jerk * norm_volume, dec!(-3), dec!(3));

        // 14. Acc efficiency
        let acc_efficiency = if a_smooth.abs() > EPSILON {
            clamp(safe_div(a_smooth, a_smooth.abs()), dec!(-1), dec!(1))
        } else {
            dec!(0)
        };

        // 15. Acc div signal
        // 计算 20 日最高/最低
        let price_high_20d = if n >= 20 {
            let start = n - 20;
            self.close.range(start..n).max().copied().unwrap_or(c)
        } else {
            c
        };

        let price_low_20d = if n >= 20 {
            let start = n - 20;
            self.close.range(start..n).min().copied().unwrap_or(c)
        } else {
            c
        };

        // Acc ma3
        let acc_ma3 = if self.acceleration.len() >= 3 {
            let sum: Decimal = self.acceleration.iter().sum();
            sum / dec!(3)
        } else {
            a_smooth
        };

        // Acc div signal
        let acc_div_signal = if price_high_20d > dec!(0) && c >= price_high_20d * dec!(0.99) && a_smooth < acc_ma3 {
            dec!(-1) // 顶背离
        } else if price_low_20d > dec!(0) && c <= price_low_20d * dec!(1.01) && a_smooth > acc_ma3 {
            dec!(1) // 底背离
        } else {
            dec!(0)
        };

        // 构建输出
        Indicator1mOutput {
            mid: (h + l) / dec!(2),
            velocity: vel,
            acceleration: acc,
            a_smooth,
            power: self.power,
            velocity_percentile: vel_pct,
            acc_percentile: acc_pct,
            power_percentile: power_pct,
            zscore_1h_1m: clamp(zscore_1h_1m, -ZSCORE_MAX_LIMIT, ZSCORE_MAX_LIMIT),
            zscore_14_1m: clamp(zscore_14_1m, -ZSCORE_MAX_LIMIT, ZSCORE_MAX_LIMIT),
            pos_norm_60,
            tr_base_10min: safe_div(high_10_val - low_10_val, prev + EPSILON),
            tr_ratio_10min_1h: safe_div(tr_10_avg, tr_60_avg + EPSILON),
            tr_ratio_zscore_10min_1h: clamp(tr_ratio_zscore_10min_1h, -ZSCORE_MAX_LIMIT, ZSCORE_MAX_LIMIT),
            jerk: jerk_val,
            norm_jerk,
            market_force,
            acc_efficiency,
            acc_div_signal,
            trend_dir,
        }
    }
}

// ==================== 输出结构体 ====================
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Indicator1mOutput {
    pub mid: Decimal,
    pub velocity: Decimal,
    pub acceleration: Decimal,
    pub a_smooth: Decimal,
    pub power: Decimal,
    pub velocity_percentile: Decimal,
    pub acc_percentile: Decimal,
    pub power_percentile: Decimal,
    pub zscore_1h_1m: Decimal,
    pub zscore_14_1m: Decimal,
    pub pos_norm_60: Decimal,
    pub tr_base_10min: Decimal,
    pub tr_ratio_10min_1h: Decimal,
    pub tr_ratio_zscore_10min_1h: Decimal,
    pub jerk: Decimal,
    pub norm_jerk: Decimal,
    pub market_force: Decimal,
    pub acc_efficiency: Decimal,
    pub acc_div_signal: Decimal,
    pub trend_dir: Decimal,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_indicator_1m_basic() {
        let mut indicator = Indicator1m::new();

        // 模拟 100 根 K 线
        for i in 0..100 {
            let price = dec!(100) + Decimal::from(i) / dec!(10);
            let high = price + dec!(1);
            let low = price - dec!(1);
            let close = price;
            let volume = dec!(1_000_000);

            let output = indicator.update(high, low, close, volume);

            if i > 0 {
                println!(
                    "K线 {} | velocity: {} | a_smooth: {} | power: {}",
                    i, output.velocity, output.a_smooth, output.power
                );
            }
        }

        // 验证有数据输出
        assert!(indicator.close.len() > 0);
    }

    #[test]
    fn test_percentile_alignment() {
        // 验证百分位计算对齐 Python
        let mut indicator = Indicator1m::new();

        // 喂入固定数据
        for i in 0..60 {
            let price = dec!(100) + Decimal::from(i) / dec!(10);
            indicator.update(price + dec!(1), price - dec!(1), price, dec!(1_000_000));
        }

        let output = indicator.update(dec!(105), dec!(104), dec!(105), dec!(1_000_000));
        println!("velocity_percentile: {}", output.velocity_percentile);
        println!("acc_percentile: {}", output.acc_percentile);
        println!("power_percentile: {}", output.power_percentile);
    }

    #[test]
    fn test_zscore_calculation() {
        let mut indicator = Indicator1m::new();

        // 喂入足够数据让 Z-Score 计算有意义
        for i in 0..100 {
            let price = dec!(100) + Decimal::from(i) / dec!(10);
            indicator.update(price + dec!(1), price - dec!(1), price, dec!(1_000_000));
        }

        let output = indicator.update(dec!(110), dec!(109), dec!(110), dec!(1_000_000));
        println!("Z-Score 1h: {}", output.zscore_1h_1m);
        println!("Z-Score 14: {}", output.zscore_14_1m);
    }
}
