//! GaussianNoise - 高斯噪声模块
//!
//! 用于 Tick 价格生成时添加微小波动，模拟真实市场噪声。

use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal_macros::dec;

/// 高斯噪声生成器
///
/// 使用 Box-Muller 变换生成正态分布随机数
pub struct GaussianNoise {
    /// 随机数生成器（使用线程本地 rand）
    #[cfg(feature = "rand")]
    rng: rand::rngs::ThreadRng,
}

impl GaussianNoise {
    /// 创建噪声生成器
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "rand")]
            rng: rand::thread_rng(),
        }
    }

    /// 生成标准高斯噪声（均值 0，标准差 1）
    #[cfg(feature = "rand")]
    pub fn sample(&mut self) -> f64 {
        let u1: f64 = rand::Rng::r#gen(&mut self.rng);
        let u2: f64 = rand::Rng::r#gen(&mut self.rng);
        // Box-Muller 变换
        let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
        z
    }

    /// 生成指定均值和标准差的高斯噪声
    #[cfg(feature = "rand")]
    pub fn sample_with_params(&mut self, mean: f64, std: f64) -> f64 {
        mean + std * self.sample()
    }

    /// 生成 Decimal 类型的高斯噪声
    ///
    /// mean: 均值
    /// std: 标准差（绝对值）
    #[cfg(feature = "rand")]
    pub fn sample_decimal(&mut self, mean: Decimal, std: Decimal) -> Decimal {
        let mean_f = mean.to_f64().unwrap_or(0.0);
        let std_f = std.to_f64().unwrap_or(1.0);
        let z = self.sample_with_params(mean_f, std_f);
        Decimal::from_f64_retain(z).unwrap_or(mean)
    }

    /// 简化版本：使用固定噪声（无 rand 依赖）
    ///
    /// 返回 [-scale, scale] 范围内的近似高斯噪声
    /// 使用伪随机序列模拟
    #[cfg(not(feature = "rand"))]
    pub fn sample(&mut self) -> f64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);

        let idx = COUNTER.fetch_add(1, Ordering::Relaxed);
        // 使用分数法近似高斯分布
        let u1 = ((idx * 1103515245 + 12345) % (1 << 31)) as f64 / (1 << 31) as f64;
        let u2 = (((idx * 1103515245 + 12345) >> 16) % (1 << 15)) as f64 / (1 << 15) as f64;
        let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
        z
    }

    #[cfg(not(feature = "rand"))]
    pub fn sample_with_params(&mut self, mean: f64, std: f64) -> f64 {
        mean + std * self.sample()
    }

    #[cfg(not(feature = "rand"))]
    pub fn sample_decimal(&mut self, mean: Decimal, std: Decimal) -> Decimal {
        let mean_f = mean.to_f64().unwrap_or(0.0);
        let std_f = std.to_f64().unwrap_or(1.0);
        let z = self.sample_with_params(mean_f, std_f);
        Decimal::from_f64_retain(z).unwrap_or(mean)
    }
}

impl Default for GaussianNoise {
    fn default() -> Self {
        Self::new()
    }
}

/// 根据价格区间生成噪声缩放因子
///
/// interval: 价格区间 (high - low)
/// factor: 噪声因子（默认 2%）
pub fn noise_scale(interval: Decimal, factor: Decimal) -> Decimal {
    interval * factor / dec!(100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noise_creation() {
        let noise = GaussianNoise::new();
        assert!(std::mem::size_of_val(&noise) > 0);
    }

    #[test]
    fn test_noise_scale() {
        use rust_decimal_macros::dec;
        let scale = noise_scale(dec!(1000), dec!(2));
        assert_eq!(scale, dec!(20)); // 2% of 1000
    }
}
