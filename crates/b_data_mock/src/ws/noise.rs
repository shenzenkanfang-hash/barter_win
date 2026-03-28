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
    rng: rand::rngs::ThreadRng,
}

impl GaussianNoise {
    pub fn new() -> Self {
        Self {
            rng: rand::thread_rng(),
        }
    }

    pub fn sample(&mut self) -> f64 {
        let u1: f64 = rand::Rng::r#gen(&mut self.rng);
        let u2: f64 = rand::Rng::r#gen(&mut self.rng);
        let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
        z
    }

    pub fn sample_with_params(&mut self, mean: f64, std: f64) -> f64 {
        mean + std * self.sample()
    }

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
pub fn noise_scale(interval: Decimal, factor: Decimal) -> Decimal {
    interval * factor / dec!(100)
}
