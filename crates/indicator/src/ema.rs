use rust_decimal::Decimal;
use rust_decimal_macros::dec;

#![forbid(unsafe_code)]

#[derive(Debug, Clone)]
pub struct EMA {
    pub period: u32,
    pub value: Decimal,
    k: Decimal,
}

impl EMA {
    pub fn new(period: u32) -> Self {
        let k = dec!(2) / (Decimal::from(period) + dec!(1));
        Self {
            period,
            value: Decimal::ZERO,
            k,
        }
    }

    pub fn calculate(&mut self, price: Decimal) -> Decimal {
        if self.value.is_zero() {
            self.value = price;
        } else {
            self.value = price * self.k + self.value * (dec!(1) - self.k);
        }
        self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// E1.1 EMA 增量计算测试 - 验证 O(1) 特性
    ///
    /// EMA 公式: EMA = price * k + prev_ema * (1 - k), 其中 k = 2/(period+1)
    /// 测试场景: 输入连续 K 线，验证每步计算结果正确
    #[test]
    fn test_ema_incremental_calculation() {
        // 验证 EMA(10) 增量计算
        let mut ema = EMA::new(10);
        let k = dec!(2) / dec!(11); // k = 2/(10+1)

        // 第一个价格，直接赋值
        let price1 = dec!(100);
        let result1 = ema.calculate(price1);
        assert_eq!(result1, price1, "First price should be used directly as EMA value");

        // 第二个价格
        let price2 = dec!(105);
        let expected2 = price2 * k + price1 * (dec!(1) - k);
        let result2 = ema.calculate(price2);
        assert_eq!(result2, expected2, "EMA should follow incremental formula");

        // 第三个价格
        let price3 = dec!(110);
        let expected3 = price3 * k + expected2 * (dec!(1) - k);
        let result3 = ema.calculate(price3);
        assert_eq!(result3, expected3, "EMA should use previous EMA value");

        // 验证 EMA 值随价格变化
        assert!(ema.value > price1, "EMA should converge towards recent prices");
    }

    /// 测试 EMA 周期参数
    #[test]
    fn test_ema_period_parameter() {
        let mut ema_short = EMA::new(5);
        let mut ema_long = EMA::new(20);

        // 同样的价格序列
        let prices = [dec!(100), dec!(105), dec!(110), dec!(108), dec!(115)];

        for price in prices {
            ema_short.calculate(price);
            ema_long.calculate(price);
        }

        // 短周期 EMA 应该对价格变化更敏感
        // 在上涨趋势中，短周期 EMA 应该 >= 长周期 EMA
        assert!(ema_short.value >= ema_long.value,
            "Short period EMA should be more responsive to recent price changes");
    }

    /// 测试 EMA 零值初始化
    #[test]
    fn test_ema_zero_initialization() {
        let mut ema = EMA::new(14);
        assert!(ema.value.is_zero(), "EMA should start with zero value");

        // 第一个价格后，不再是零
        let first_price = dec!(100);
        ema.calculate(first_price);
        assert!(!ema.value.is_zero(), "EMA should not be zero after first calculation");
        assert_eq!(ema.value, first_price, "First calculation should use price directly");
    }

    /// 测试 EMA 稳定值计算
    #[test]
    fn test_ema_stable_value() {
        let mut ema = EMA::new(10);

        // 稳定价格，EMA 应该收敛到该价格
        let stable_price = dec!(100);
        for _ in 0..100 {
            ema.calculate(stable_price);
        }

        // EMA 应该在稳定价格附近
        let diff = (ema.value - stable_price).abs();
        assert!(diff < dec!(0.01),
            format!("EMA should converge to stable price, diff: {}", diff));
    }
}
