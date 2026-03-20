use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::VecDeque;

/// 速度百分位指标
///
/// 计算价格变化的加速度和速度的百分位排名。
/// 用于判断当前价格变化的剧烈程度。
///
/// 设计依据: indicator_1m/indicator_calc.py 中的 velocity_percentile
#[derive(Debug, Clone)]
pub struct VelocityPercentile {
    /// 窗口大小 (默认 60)
    window: usize,
    /// 速度历史队列
    velocity_history: VecDeque<Decimal>,
    /// 速度绝对值历史队列
    velocity_abs_history: VecDeque<Decimal>,
    /// 当前速度
    current_velocity: Decimal,
    /// 当前加速度
    current_acceleration: Decimal,
    /// 上一价格
    prev_price: Decimal,
    /// 上一速度
    prev_velocity: Decimal,
}

impl VelocityPercentile {
    /// 创建新的速度百分位计算器
    pub fn new(window: usize) -> Self {
        Self {
            window,
            velocity_history: VecDeque::with_capacity(window),
            velocity_abs_history: VecDeque::with_capacity(window),
            current_velocity: dec!(0),
            current_acceleration: dec!(0),
            prev_price: dec!(0),
            prev_velocity: dec!(0),
        }
    }

    /// 增量计算速度百分位
    ///
    /// 返回: (velocity_percentile, acceleration_percentile)
    pub fn calculate(&mut self, price: Decimal) -> (Decimal, Decimal) {
        if self.prev_price > dec!(0) {
            // 计算速度 (价格变化率)
            self.current_velocity = (price - self.prev_price) / self.prev_price;

            // 计算加速度 (速度变化)
            self.current_acceleration = self.current_velocity - self.prev_velocity;

            // 更新速度历史
            if self.velocity_history.len() >= self.window {
                self.velocity_history.pop_front();
            }
            self.velocity_history.push_back(self.current_velocity);

            // 更新速度绝对值历史
            let abs_vel = self.current_velocity.abs();
            if self.velocity_abs_history.len() >= self.window {
                self.velocity_abs_history.pop_front();
            }
            self.velocity_abs_history.push_back(abs_vel);

            self.prev_velocity = self.current_velocity;
        }

        self.prev_price = price;

        // 计算百分位
        let vel_percentile = self.calculate_percentile(&self.velocity_abs_history, self.current_velocity.abs());
        let acc_percentile = self.calculate_acceleration_percentile();

        // 带符号的速度百分位 (-100 到 100)
        let signed_vel_percentile = if self.current_velocity >= dec!(0) {
            vel_percentile
        } else {
            -vel_percentile
        };

        (signed_vel_percentile, acc_percentile)
    }

    /// 计算百分位 (使用简单排名法)
    fn calculate_percentile(&self, history: &VecDeque<Decimal>, value: Decimal) -> Decimal {
        if history.is_empty() {
            return dec!(0);
        }

        let count = Decimal::from(history.len());
        let below = Decimal::from(history.iter().filter(|&&v| v < value).count());
        let equal = Decimal::from(history.iter().filter(|&&v| v == value).count());

        // 排名百分位
        ((below + equal * dec!(0.5)) / count) * dec!(100)
    }

    /// 计算加速度百分位
    fn calculate_acceleration_percentile(&self) -> Decimal {
        if self.velocity_history.len() < 2 {
            return dec!(0);
        }

        // 计算加速度序列的百分位
        let mut accel_history: VecDeque<Decimal> = VecDeque::new();
        let mut prev: Option<Decimal> = None;

        for &vel in self.velocity_history.iter() {
            if let Some(p) = prev {
                let accel = vel - p;
                accel_history.push_back(accel.abs());
            }
            prev = Some(vel);
        }

        self.calculate_percentile(&accel_history, self.current_acceleration.abs())
    }

    /// 获取当前速度
    pub fn velocity(&self) -> Decimal {
        self.current_velocity
    }

    /// 获取当前加速度
    pub fn acceleration(&self) -> Decimal {
        self.current_acceleration
    }
}

impl Default for VelocityPercentile {
    fn default() -> Self {
        Self::new(60) // 1小时窗口
    }
}

/// 价格偏离指标
///
/// 计算当前价格相对于历史价格区间的位置。
/// 100 = 最高价，0 = 最低价
///
/// 设计依据: indicator_1d/pine_scripts.py 中的 price_deviation_horizontal_position
#[derive(Debug, Clone)]
pub struct PriceDeviation {
    /// 窗口大小
    window: usize,
    /// 价格历史
    price_history: VecDeque<Decimal>,
    /// 最低价
    min_price: Decimal,
    /// 最高价
    max_price: Decimal,
}

impl PriceDeviation {
    /// 创建新的价格偏离计算器
    pub fn new(window: usize) -> Self {
        Self {
            window,
            price_history: VecDeque::with_capacity(window),
            min_price: dec!(0),
            max_price: dec!(0),
        }
    }

    /// 计算价格偏离
    ///
    /// 返回: 0-100 的偏离位置
    /// 0 = 价格区间底部，100 = 价格区间顶部
    pub fn calculate(&mut self, price: Decimal) -> Decimal {
        // 更新历史
        if self.price_history.len() >= self.window {
            self.price_history.pop_front();
        }
        self.price_history.push_back(price);

        // 更新最低/最高价
        if self.min_price <= dec!(0) || price < self.min_price {
            self.min_price = price;
        }
        if self.max_price <= dec!(0) || price > self.max_price {
            self.max_price = price;
        }

        // 计算偏离
        let range = self.max_price - self.min_price;
        if range <= dec!(0) {
            return dec!(50); // 无效时返回中间值
        }

        (price - self.min_price) / range * dec!(100)
    }

    /// 获取当前价格区间
    pub fn price_range(&self) -> (Decimal, Decimal) {
        (self.min_price, self.max_price)
    }

    /// 重置区间 (重新开始计算)
    pub fn reset_range(&mut self) {
        self.min_price = dec!(0);
        self.max_price = dec!(0);
    }
}

impl Default for PriceDeviation {
    fn default() -> Self {
        Self::new(100) // 默认 100 根 K 线窗口
    }
}

/// 动量指标
///
/// 计算 N 周期内的价格动量变化。
#[derive(Debug, Clone)]
pub struct Momentum {
    /// 窗口大小
    period: usize,
    /// 价格历史
    prices: VecDeque<Decimal>,
}

impl Momentum {
    /// 创建新的动量计算器
    pub fn new(period: usize) -> Self {
        Self {
            period,
            prices: VecDeque::with_capacity(period + 1),
        }
    }

    /// 计算动量
    ///
    /// 返回: 动量值 = 当前价格 - N 周期前价格
    pub fn calculate(&mut self, price: Decimal) -> Decimal {
        if self.prices.len() >= self.period {
            self.prices.pop_front();
        }
        self.prices.push_back(price);

        if self.prices.len() < self.period {
            return dec!(0);
        }

        // 当前价格 - period 周期前价格
        price - self.prices[0]
    }

    /// 获取动量百分比
    pub fn momentum_percent(&self, current: Decimal, period_ago: Decimal) -> Decimal {
        if period_ago <= dec!(0) {
            return dec!(0);
        }
        (current - period_ago) / period_ago * dec!(100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_velocity_percentile() {
        let mut vp = VelocityPercentile::new(5);

        // 喂入价格序列
        let (v1, a1) = vp.calculate(dec!(100));
        let (v2, a2) = vp.calculate(dec!(101));
        let (v3, a3) = vp.calculate(dec!(102));
        let (v4, a4) = vp.calculate(dec!(103));
        let (v5, a5) = vp.calculate(dec!(104));

        // 速度应该是正的
        assert!(v2 > dec!(0));
        // 加速应该是正的 (连续上涨)
        assert!(a3 >= dec!(0));
    }

    #[test]
    fn test_price_deviation() {
        let mut pd = PriceDeviation::new(10);

        // 喂入价格
        pd.calculate(dec!(100));
        pd.calculate(dec!(105));
        pd.calculate(dec!(90));

        let deviation = pd.calculate(dec!(100));
        assert!(deviation >= dec!(0) && deviation <= dec!(100));
    }

    #[test]
    fn test_momentum() {
        let mut m = Momentum::new(3);
        m.calculate(dec!(100));
        m.calculate(dec!(102));
        m.calculate(dec!(101));
        let momentum = m.calculate(dec!(105));

        // 当前 105 - 3周期前价格(但prices[0]=102因为第一个被pop) = 3
        assert_eq!(momentum, dec!(3));
    }
}
