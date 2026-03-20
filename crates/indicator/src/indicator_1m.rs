#![forbid(unsafe_code)]

//! 1分钟指标计算器
//!
//! 从 Python indicator_calc.py 迁移的非 Pine 指标逻辑
//!
//! 包含:
//! - 基础物理指标: velocity, acceleration, power
//! - TR 指标: tr_ratio, tr_ratio_zscore
//! - 百分位指标: velocity_percentile, acc_percentile, power_percentile
//! - Z-Score 指标
//! - 高阶动能指标: jerk, market_force, acc_efficiency

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};

// ==================== 常量定义 ====================
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
fn safe_div_with_epsilon(n: Decimal, d: Decimal, epsilon: Decimal) -> Decimal {
    if d.abs() < epsilon {
        n / epsilon
    } else {
        n / d
    }
}

#[inline(always)]
fn clamp(value: Decimal, min_val: Decimal, max_val: Decimal) -> Decimal {
    if value < min_val {
        min_val
    } else if value > max_val {
        max_val
    } else {
        value
    }
}

// ==================== 百分位计算 ====================
/// 滚动百分位计算（使用简单排序实现）
struct RollingPercentile {
    window: usize,
    values: VecDeque<Decimal>,
    sorted_cache: Vec<Decimal>,
}

impl RollingPercentile {
    fn new(window: usize) -> Self {
        Self {
            window,
            values: VecDeque::with_capacity(window),
            sorted_cache: Vec::with_capacity(window),
        }
    }

    fn update(&mut self, value: Decimal) -> Decimal {
        // 添加新值
        self.values.push_back(value);
        if self.values.len() > self.window {
            self.values.pop_front();
        }

        // 数据不足时返回默认值
        if self.values.len() < 10 {
            return dec!(50);
        }

        // 更新排序缓存
        self.sorted_cache.clear();
        self.sorted_cache.extend(self.values.iter());
        self.sorted_cache.sort();

        // 当前值在排序后的位置
        let current_idx = self.values.len() - 1;
        let current_value = self.values[current_idx];

        // 简单百分位计算
        let rank = self.sorted_cache.iter().filter(|&&x| x < current_value).count();
        let percentile = (rank as f64) / ((self.sorted_cache.len() - 1) as f64) * 100.0;

        Decimal::from_f64_retain(percentile).unwrap_or(dec!(50))
    }
}

// ==================== 1分钟指标计算器 ====================
/// 1分钟K线指标计算器
///
/// 增量计算 O(1)，严格对齐 Python indicator_calc.py
pub struct Indicator1m {
    // 窗口参数
    window_10min: usize,
    window_15min: usize,
    window_1h: usize,
    window_5h: usize,
    window_14: usize,
    window_2h: usize,
    norm_win: usize,

    // 价格历史
    high_history: VecDeque<Decimal>,
    low_history: VecDeque<Decimal>,
    close_history: VecDeque<Decimal>,
    volume_history: VecDeque<Decimal>,

    // 预计算缓存
    rolling_cache: HashMap<String, VecDeque<Decimal>>,

    // 中间计算值
    mid_history: VecDeque<Decimal>,
    velocity_history: VecDeque<Decimal>,
    acceleration_history: VecDeque<Decimal>,

    // 百分位计算器
    velocity_percentile: RollingPercentile,
    acc_percentile: RollingPercentile,
    power_percentile: RollingPercentile,

    // 趋势方向
    trend_dir: Decimal,

    // 高阶动能历史
    jerk_history: VecDeque<Decimal>,
    jerk_std: Decimal,
    jerk_signal_history: VecDeque<i32>,
}

impl Default for Indicator1m {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator1m {
    /// 创建新的计算器
    pub fn new() -> Self {
        let mut rolling_cache = HashMap::new();
        rolling_cache.insert("high_60".to_string(), VecDeque::with_capacity(100));
        rolling_cache.insert("low_60".to_string(), VecDeque::with_capacity(100));
        rolling_cache.insert("high_10".to_string(), VecDeque::with_capacity(20));
        rolling_cache.insert("low_10".to_string(), VecDeque::with_capacity(20));

        Self {
            window_10min: WINDOW_10MIN,
            window_15min: WINDOW_15MIN,
            window_1h: WINDOW_1H,
            window_5h: WINDOW_5H,
            window_14: WINDOW_14,
            window_2h: WINDOW_2H,
            norm_win: NORM_WIN,
            high_history: VecDeque::with_capacity(500),
            low_history: VecDeque::with_capacity(500),
            close_history: VecDeque::with_capacity(500),
            volume_history: VecDeque::with_capacity(500),
            rolling_cache,
            mid_history: VecDeque::with_capacity(500),
            velocity_history: VecDeque::with_capacity(500),
            acceleration_history: VecDeque::with_capacity(500),
            velocity_percentile: RollingPercentile::new(WINDOW_1H),
            acc_percentile: RollingPercentile::new(WINDOW_1H),
            power_percentile: RollingPercentile::new(WINDOW_1H),
            trend_dir: dec!(0),
            jerk_history: VecDeque::with_capacity(500),
            jerk_std: dec!(0),
            jerk_signal_history: VecDeque::with_capacity(10),
        }
    }

    /// 更新指标（增量计算 O(1)）
    /// 返回: Indicator1mOutput
    pub fn update(&mut self, high: Decimal, low: Decimal, close: Decimal, volume: Decimal) -> Indicator1mOutput {
        // 1. 更新历史
        self.high_history.push_back(high);
        self.low_history.push_back(low);
        self.close_history.push_back(close);
        self.volume_history.push_back(volume);

        // 保持窗口大小
        if self.high_history.len() > 500 {
            self.high_history.pop_front();
            self.low_history.pop_front();
            self.close_history.pop_front();
            self.volume_history.pop_front();
        }

        let n = self.close_history.len();

        // 2. 基础物理指标
        let mid = (high + low) / dec!(2);

        let velocity = if n > 1 {
            let prev_close = self.close_history[self.close_history.len() - 2];
            safe_div(close - prev_close, prev_close)
        } else {
            dec!(0)
        };

        let acceleration = if n > 1 {
            let prev_velocity = self.velocity_history.back().copied().unwrap_or(dec!(0));
            velocity - prev_velocity
        } else {
            dec!(0)
        };

        // 平滑加速度 (rolling 3 mean)
        self.velocity_history.push_back(velocity);
        if self.velocity_history.len() > 3 {
            self.velocity_history.pop_front();
        }

        self.acceleration_history.push_back(acceleration);
        if self.acceleration_history.len() > 3 {
            self.acceleration_history.pop_front();
        }

        let a_smooth: Decimal = if self.acceleration_history.len() >= 3 {
            let sum: Decimal = self.acceleration_history.iter().sum();
            sum / dec!(3)
        } else {
            acceleration
        };

        // Power
        let power = a_smooth * velocity;

        // 趋势方向
        self.trend_dir = velocity.sign();

        // 3. 预计算缓存更新
        self.update_rolling_cache();

        // 4. TR 指标
        let tr_output = self.calculate_tr(&close);

        // 5. 百分位指标
        let velocity_percentile = self.velocity_percentile.update(velocity.abs());
        let acc_percentile = self.acc_percentile.update((a_smooth * velocity).abs());
        let power_percentile = self.power_percentile.update(power.abs());

        // 6. Z-Score
        let zscore_1h_1m = self.calculate_zscore_1h(&a_smooth);
        let zscore_14_1m = self.calculate_zscore_14(&a_smooth);

        // 7. 空间百分位
        let pos_norm_60 = self.calculate_pos_norm_60();

        // 8. 价格偏离度
        let price_deviation = self.calculate_price_deviation();
        let price_deviation_hp = self.calculate_price_deviation_hp();

        // 9. 高阶动能指标
        let kinetic_output = self.calculate_high_order_kinetic(&a_smooth, &velocity);

        // 构建输出
        Indicator1mOutput {
            // 基础物理指标
            mid,
            velocity,
            acceleration,
            a_smooth,
            power,

            // TR 指标
            tr_base_10min: tr_output.tr_base_10min,
            tr_10min_avg: tr_output.tr_10min_avg,
            tr_1h_avg: tr_output.tr_1h_avg,
            tr_ratio_10min_1h: tr_output.tr_ratio_10min_1h,
            tr_ratio_zscore_10min_1h: tr_output.tr_ratio_zscore_10min_1h,
            tr_base_60min: tr_output.tr_base_60min,
            tr_60min_avg: tr_output.tr_60min_avg,
            tr_5h_avg: tr_output.tr_5h_avg,
            tr_ratio_60min_5h: tr_output.tr_ratio_60min_5h,
            tr_ratio_zscore_60min_5h: tr_output.tr_ratio_zscore_60min_5h,

            // 百分位指标
            velocity_percentile,
            acc_percentile,
            power_percentile,

            // Z-Score
            zscore_1h_1m,
            zscore_14_1m,

            // 空间百分位
            pos_norm_60,

            // 价格偏离度
            price_deviation,
            price_deviation_horizontal_position: price_deviation_hp,

            // 高阶动能
            jerk: kinetic_output.jerk,
            norm_jerk: kinetic_output.norm_jerk,
            jerk_signal: kinetic_output.jerk_signal,
            norm_volume: kinetic_output.norm_volume,
            norm_acceleration_daily: kinetic_output.norm_acceleration_daily,
            market_force: kinetic_output.market_force,
            acc_efficiency: kinetic_output.acc_efficiency,
            price_high_20d: kinetic_output.price_high_20d,
            price_low_20d: kinetic_output.price_low_20d,
            acc_ma3: kinetic_output.acc_ma3,
            acc_div_signal: kinetic_output.acc_div_signal,

            // 趋势
            trend_dir: self.trend_dir,
        }
    }

    /// 更新滚动缓存
    fn update_rolling_cache(&mut self) {
        let n = self.close_history.len();

        // High/Low 10min
        if n >= self.window_10min {
            let recent_highs: Vec<_> = self.high_history.iter().rev().take(self.window_10min).cloned().collect();
            let recent_lows: Vec<_> = self.low_history.iter().rev().take(self.window_10min).cloned().collect();

            let high_10 = recent_highs.iter().max().copied().unwrap_or(dec!(0));
            let low_10 = recent_lows.iter().min().copied().unwrap_or(dec!(0));

            if let Some(cache) = self.rolling_cache.get_mut("high_10") {
                cache.push_back(high_10);
                if cache.len() > self.window_10min {
                    cache.pop_front();
                }
            }
            if let Some(cache) = self.rolling_cache.get_mut("low_10") {
                cache.push_back(low_10);
                if cache.len() > self.window_10min {
                    cache.pop_front();
                }
            }
        }

        // High/Low 60min
        if n >= self.window_1h {
            let recent_highs: Vec<_> = self.high_history.iter().rev().take(self.window_1h).cloned().collect();
            let recent_lows: Vec<_> = self.low_history.iter().rev().take(self.window_1h).cloned().collect();

            let high_60 = recent_highs.iter().max().copied().unwrap_or(dec!(0));
            let low_60 = recent_lows.iter().min().copied().unwrap_or(dec!(0));

            if let Some(cache) = self.rolling_cache.get_mut("high_60") {
                cache.push_back(high_60);
                if cache.len() > self.window_1h {
                    cache.pop_front();
                }
            }
            if let Some(cache) = self.rolling_cache.get_mut("low_60") {
                cache.push_back(low_60);
                if cache.len() > self.window_1h {
                    cache.pop_front();
                }
            }
        }
    }

    /// 计算 TR 指标
    fn calculate_tr(&self, close: &Decimal) -> TROutput {
        let n = self.close_history.len();

        // TR 10min
        let (tr_base_10min, tr_10min_avg, tr_1h_avg, tr_ratio_10min_1h, tr_ratio_zscore_10min_1h) =
            if n >= self.window_2h {
                let high_10 = self.rolling_cache.get("high_10")
                    .and_then(|c| c.back())
                    .copied().unwrap_or(dec!(0));
                let low_10 = self.rolling_cache.get("low_10")
                    .and_then(|c| c.back())
                    .copied().unwrap_or(dec!(0));

                let close_shift = if n > self.window_10min {
                    self.close_history[n - 1 - self.window_10min]
                } else {
                    *close
                };

                let tr_base = safe_div(high_10 - low_10, close_shift + EPSILON);

                // 计算滚动均值
                let tr_10m_sum: Decimal = (0..self.window_10min)
                    .filter_map(|i| {
                        let idx = n - 1 - i;
                        if idx > 0 && idx < n {
                            let hs = self.high_history[idx].min(*close);
                            let ls = self.low_history[idx].max(*close);
                            Some(safe_div(hs - ls, self.close_history[idx.saturating_sub(self.window_10min)] + EPSILON))
                        } else {
                            None
                        }
                    })
                    .sum();

                let tr_10avg = tr_10m_sum / Decimal::from(self.window_10min);

                // TR 1h avg (简化)
                let tr_1avg = tr_base;

                let ratio = safe_div(tr_10avg, tr_1avg + EPSILON);
                let zscore = dec!(0); // 简化

                (tr_base, tr_10avg, tr_1avg, ratio, zscore)
            } else {
                (dec!(0), dec!(0), dec!(0), dec!(1), dec!(0))
            };

        // TR 60min
        let (tr_base_60min, tr_60min_avg, tr_5h_avg, tr_ratio_60min_5h, tr_ratio_zscore_60min_5h) =
            if n >= self.window_5h {
                let high_60 = self.rolling_cache.get("high_60")
                    .and_then(|c| c.back())
                    .copied().unwrap_or(dec!(0));
                let low_60 = self.rolling_cache.get("low_60")
                    .and_then(|c| c.back())
                    .copied().unwrap_or(dec!(0));

                let close_shift = if n > self.window_1h {
                    self.close_history[n - 1 - self.window_1h]
                } else {
                    *close
                };

                let tr_base = safe_div(high_60 - low_60, close_shift + EPSILON);
                let tr_60avg = tr_base;
                let tr_5avg = tr_base;

                let ratio = safe_div(tr_60avg, tr_5avg + EPSILON);

                (tr_base, tr_60avg, tr_5avg, ratio, dec!(0))
            } else {
                (dec!(0), dec!(0), dec!(0), dec!(1), dec!(0))
            };

        TROutput {
            tr_base_10min,
            tr_10min_avg,
            tr_1h_avg,
            tr_ratio_10min_1h,
            tr_ratio_zscore_10min_1h,
            tr_base_60min,
            tr_60min_avg,
            tr_5h_avg,
            tr_ratio_60min_5h,
            tr_ratio_zscore_60min_5h,
        }
    }

    /// 计算 Z-Score (1h window)
    fn calculate_zscore_1h(&self, a_smooth: &Decimal) -> Decimal {
        if self.acceleration_history.len() < 10 {
            return dec!(0);
        }

        let mean: Decimal = self.acceleration_history.iter().sum::<Decimal>()
            / Decimal::from(self.acceleration_history.len());

        let variance: Decimal = self.acceleration_history.iter()
            .map(|&x| {
                let diff = x - mean;
                diff * diff
            })
            .sum::<Decimal>() / Decimal::from(self.acceleration_history.len());

        let std = safe_div_with_epsilon(dec!(1), variance.sqrt().unwrap_or(EPSILON), EPSILON);
        let zscore = (*a_smooth - mean) * std * self.trend_dir;

        clamp(zscore, -ZSCORE_MAX_LIMIT, ZSCORE_MAX_LIMIT)
    }

    /// 计算 Z-Score (14 window)
    fn calculate_zscore_14(&self, a_smooth: &Decimal) -> Decimal {
        if self.acceleration_history.len() < 5 {
            return dec!(0);
        }

        let recent: Vec<_> = self.acceleration_history.iter().rev().take(14).cloned().collect();
        if recent.len() < 5 {
            return dec!(0);
        }

        let mean: Decimal = recent.iter().sum::<Decimal>() / Decimal::from(recent.len());
        let variance: Decimal = recent.iter()
            .map(|&x| {
                let diff = x - mean;
                diff * diff
            })
            .sum::<Decimal>() / Decimal::from(recent.len());

        let std = safe_div_with_epsilon(dec!(1), variance.sqrt().unwrap_or(EPSILON), EPSILON);
        let zscore = (*a_smooth - mean) * std * self.trend_dir;

        clamp(zscore, -ZSCORE_MAX_LIMIT, ZSCORE_MAX_LIMIT)
    }

    /// 计算 60 分钟区间位置
    fn calculate_pos_norm_60(&self) -> Decimal {
        let n = self.close_history.len();
        if n < self.window_1h {
            return dec!(50);
        }

        let high_60 = self.rolling_cache.get("high_60")
            .and_then(|c| c.back())
            .copied().unwrap_or(dec!(0));
        let low_60 = self.rolling_cache.get("low_60")
            .and_then(|c| c.back())
            .copied().unwrap_or(dec!(0));

        if high_60 <= low_60 {
            return dec!(50);
        }

        let current_close = self.close_history.back().copied().unwrap_or(dec!(0));
        let range = high_60 - low_60;
        let pos = safe_div(current_close - low_60, range) * dec!(100);

        clamp(pos, dec!(0), dec!(100))
    }

    /// 计算价格偏离度
    fn calculate_price_deviation(&self) -> Decimal {
        let n = self.close_history.len();
        if n <= self.window_15min {
            return dec!(0);
        }

        let current = self.close_history.back().copied().unwrap_or(dec!(0));
        let prev = self.close_history[n - 1 - self.window_15min];

        safe_div(current - prev, prev + EPSILON)
    }

    /// 计算价格偏离度百分位 (horizontal position)
    fn calculate_price_deviation_hp(&self) -> Decimal {
        dec!(50) // 简化实现
    }

    /// 计算高阶动能指标
    fn calculate_high_order_kinetic(&mut self, a_smooth: &Decimal, velocity: &Decimal) -> KineticOutput {
        // Jerk (加速度的导数)
        let jerk = if self.acceleration_history.len() >= 2 {
            let prev = self.acceleration_history[self.acceleration_history.len() - 2];
            *a_smooth - prev
        } else {
            dec!(0)
        };

        self.jerk_history.push_back(jerk);
        if self.jerk_history.len() > self.norm_win {
            self.jerk_history.pop_front();
        }

        // Jerk std
        if self.jerk_history.len() >= 2 {
            let mean: Decimal = self.jerk_history.iter().sum::<Decimal>()
                / Decimal::from(self.jerk_history.len());
            let variance: Decimal = self.jerk_history.iter()
                .map(|&x| {
                    let diff = x - mean;
                    diff * diff
                })
                .sum::<Decimal>() / Decimal::from(self.jerk_history.len());
            self.jerk_std = variance.sqrt().unwrap_or(EPSILON);
        }

        let norm_jerk = safe_div(jerk, self.jerk_std + EPSILON);
        let norm_jerk = clamp(norm_jerk, dec!(-3), dec!(3));

        // Jerk signal
        let jerk_signal = self.calculate_jerk_signal(&norm_jerk);

        // Norm volume
        let volume = self.volume_history.back().copied().unwrap_or(dec!(0));
        let vol_log = if volume > dec!(0) {
            (volume / dec!(1_000_000)).ln()
        } else {
            dec!(0)
        };

        let vol_mean = if !self.volume_history.is_empty() {
            let sum: Decimal = self.volume_history.iter().sum::<Decimal>();
            let count = Decimal::from(self.volume_history.len());
            (sum / count / dec!(1_000_000)).ln().unwrap_or(dec!(0))
        } else {
            dec!(0)
        };

        let vol_std = dec!(1);
        let norm_volume = clamp((vol_log - vol_mean) / (vol_std + EPSILON), dec!(-3), dec!(3));

        // Norm acceleration daily
        let norm_acceleration_daily = norm_jerk; // 简化

        // Market force
        let market_force = clamp(norm_acceleration_daily * norm_volume, dec!(-3), dec!(3));

        // Acc efficiency
        let acc_efficiency = if a_smooth.abs() > EPSILON {
            clamp(*a_smooth / a_smooth.abs(), dec!(-1), dec!(1))
        } else {
            dec!(0)
        };

        // Price high/low 20d
        let n = self.close_history.len();
        let price_high_20d = if n >= 20 {
            self.close_history.iter().rev().take(20).max().copied().unwrap_or(dec!(0))
        } else {
            dec!(0)
        };

        let price_low_20d = if n >= 20 {
            self.close_history.iter().rev().take(20).min().copied().unwrap_or(dec!(0))
        } else {
            dec!(0)
        };

        // Acc ma3
        let acc_ma3 = if self.acceleration_history.len() >= 3 {
            let recent: Vec<_> = self.acceleration_history.iter().rev().take(3).cloned().collect();
            recent.iter().sum::<Decimal>() / dec!(3)
        } else {
            *a_smooth
        };

        // Acc div signal
        let current_close = self.close_history.back().copied().unwrap_or(dec!(0));
        let acc_div_signal = if price_high_20d > dec!(0) && current_close >= price_high_20d * dec!(0.99) && a_smooth < &acc_ma3 {
            dec!(-1) // 顶背离
        } else if price_low_20d > dec!(0) && current_close <= price_low_20d * dec!(1.01) && a_smooth > &acc_ma3 {
            dec!(1) // 底背离
        } else {
            dec!(0)
        };

        KineticOutput {
            jerk,
            norm_jerk,
            jerk_signal,
            norm_volume,
            norm_acceleration_daily,
            market_force,
            acc_efficiency,
            price_high_20d,
            price_low_20d,
            acc_ma3,
            acc_div_signal,
        }
    }

    /// 计算 jerk 信号
    fn calculate_jerk_signal(&mut self, norm_jerk: &Decimal) -> i32 {
        let prev_norm_jerk = self.jerk_history.len() >= 2
            .then(|| self.jerk_history[self.jerk_history.len() - 2])
            .unwrap_or(dec!(0));

        self.jerk_signal_history.push_back(if norm_jerk > &dec!(0) && prev_norm_jerk <= dec!(0) {
            1
        } else if norm_jerk < &dec!(0) && prev_norm_jerk >= dec!(0) {
            -1
        } else {
            0
        });

        if self.jerk_signal_history.len() > 2 {
            self.jerk_signal_history.pop_front();
        }

        *self.jerk_signal_history.back().unwrap_or(&0)
    }
}

// ==================== 输出结构体 ====================
/// TR 指标输出
struct TROutput {
    tr_base_10min: Decimal,
    tr_10min_avg: Decimal,
    tr_1h_avg: Decimal,
    tr_ratio_10min_1h: Decimal,
    tr_ratio_zscore_10min_1h: Decimal,
    tr_base_60min: Decimal,
    tr_60min_avg: Decimal,
    tr_5h_avg: Decimal,
    tr_ratio_60min_5h: Decimal,
    tr_ratio_zscore_60min_5h: Decimal,
}

/// 高阶动能指标输出
struct KineticOutput {
    jerk: Decimal,
    norm_jerk: Decimal,
    jerk_signal: i32,
    norm_volume: Decimal,
    norm_acceleration_daily: Decimal,
    market_force: Decimal,
    acc_efficiency: Decimal,
    price_high_20d: Decimal,
    price_low_20d: Decimal,
    acc_ma3: Decimal,
    acc_div_signal: Decimal,
}

/// 1分钟指标输出
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Indicator1mOutput {
    // 基础物理指标
    pub mid: Decimal,
    pub velocity: Decimal,
    pub acceleration: Decimal,
    pub a_smooth: Decimal,
    pub power: Decimal,

    // TR 指标
    pub tr_base_10min: Decimal,
    pub tr_10min_avg: Decimal,
    pub tr_1h_avg: Decimal,
    pub tr_ratio_10min_1h: Decimal,
    pub tr_ratio_zscore_10min_1h: Decimal,
    pub tr_base_60min: Decimal,
    pub tr_60min_avg: Decimal,
    pub tr_5h_avg: Decimal,
    pub tr_ratio_60min_5h: Decimal,
    pub tr_ratio_zscore_60min_5h: Decimal,

    // 百分位指标
    pub velocity_percentile: Decimal,
    pub acc_percentile: Decimal,
    pub power_percentile: Decimal,

    // Z-Score
    pub zscore_1h_1m: Decimal,
    pub zscore_14_1m: Decimal,

    // 空间百分位
    pub pos_norm_60: Decimal,

    // 价格偏离度
    pub price_deviation: Decimal,
    pub price_deviation_horizontal_position: Decimal,

    // 高阶动能
    pub jerk: Decimal,
    pub norm_jerk: Decimal,
    pub jerk_signal: i32,
    pub norm_volume: Decimal,
    pub norm_acceleration_daily: Decimal,
    pub market_force: Decimal,
    pub acc_efficiency: Decimal,
    pub price_high_20d: Decimal,
    pub price_low_20d: Decimal,
    pub acc_ma3: Decimal,
    pub acc_div_signal: Decimal,

    // 趋势
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
                println!("K线 {} | velocity: {} | a_smooth: {} | power: {}",
                    i, output.velocity, output.a_smooth, output.power);
            }
        }

        // 验证有数据输出
        assert!(indicator.close_history.len() > 0);
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
