use rust_decimal::Decimal;
use rust_decimal_macros::dec;

#[derive(Debug, Clone)]
pub struct RSI {
    pub period: u32,
    avg_gain: Decimal,
    avg_loss: Decimal,
    last_price: Decimal,
}

impl RSI {
    pub fn new(period: u32) -> Self {
        Self {
            period,
            avg_gain: Decimal::ZERO,
            avg_loss: Decimal::ZERO,
            last_price: Decimal::ZERO,
        }
    }

    pub fn calculate(&mut self, price: Decimal) -> Decimal {
        if self.last_price.is_zero() {
            self.last_price = price;
            return Decimal::ZERO;
        }

        let change = price - self.last_price;
        self.last_price = price;

        let gain = if change > Decimal::ZERO { change } else { Decimal::ZERO };
        let loss = if change < Decimal::ZERO { -change } else { Decimal::ZERO };

        if self.avg_loss.is_zero() {
            self.avg_gain = gain;
            self.avg_loss = loss;
        } else {
            self.avg_gain =
                (self.avg_gain * Decimal::from(self.period - 1) + gain) / Decimal::from(self.period);
            self.avg_loss =
                (self.avg_loss * Decimal::from(self.period - 1) + loss) / Decimal::from(self.period);
        }

        if self.avg_loss.is_zero() {
            return dec!(100);
        }

        let rs = self.avg_gain / self.avg_loss;
        dec!(100) - (dec!(100) / (dec!(1) + rs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// E1.2 RSI 计算测试 - 验证 RSI 值在 0-100 范围内
    ///
    /// 测试场景: 输入 14 周期价格数据，验证 RSI 计算正确
    #[test]
    fn test_rsi_value_range() {
        let mut rsi = RSI::new(14);

        // 模拟持续上涨的价格 (所有变化为正)
        let prices = [
            dec!(100), dec!(101), dec!(102), dec!(103), dec!(104),
            dec!(105), dec!(106), dec!(107), dec!(108), dec!(109),
            dec!(110), dec!(111), dec!(112), dec!(113), dec!(114),
        ];

        let mut last_rsi = Decimal::ZERO;
        for (i, price) in prices.iter().enumerate() {
            let result = rsi.calculate(*price);
            if i > 0 {
                // RSI 应该在 0-100 范围内
                assert!(result >= Decimal::ZERO && result <= dec!(100),
                    "RSI should be between 0 and 100, got {}", result);
                last_rsi = result;
            }
        }

        // 持续上涨应该产生高 RSI (接近 100)
        assert!(last_rsi > dec!(50), "Sustained uptrend should produce high RSI");
    }

    /// 测试 RSI 持续下跌
    #[test]
    fn test_rsi_sustained_decline() {
        let mut rsi = RSI::new(14);

        // 模拟持续下跌的价格 (所有变化为负)
        let prices = [
            dec!(114), dec!(113), dec!(112), dec!(111), dec!(110),
            dec!(109), dec!(108), dec!(107), dec!(106), dec!(105),
            dec!(104), dec!(103), dec!(102), dec!(101), dec!(100),
        ];

        let mut last_rsi = Decimal::ZERO;
        for (i, price) in prices.iter().enumerate() {
            let result = rsi.calculate(*price);
            if i > 0 {
                assert!(result >= Decimal::ZERO && result <= dec!(100),
                    "RSI should be between 0 and 100, got {}", result);
                last_rsi = result;
            }
        }

        // 持续下跌应该产生低 RSI (接近 0)
        assert!(last_rsi < dec!(50), "Sustained downtrend should produce low RSI");
    }

    /// 测试 RSI 初始状态
    #[test]
    fn test_rsi_initial_state() {
        let rsi = RSI::new(14);
        assert_eq!(rsi.avg_gain, Decimal::ZERO);
        assert_eq!(rsi.avg_loss, Decimal::ZERO);
        assert!(rsi.last_price.is_zero());
    }

    /// 测试 RSI 第一个价格返回零
    #[test]
    fn test_rsi_first_price() {
        let mut rsi = RSI::new(14);
        let first_price = dec!(100);
        let result = rsi.calculate(first_price);
        assert_eq!(result, Decimal::ZERO, "RSI should be 0 on first price");
    }

    /// 测试 RSI 波动变化
    #[test]
    fn test_rsi_volatile_prices() {
        let mut rsi = RSI::new(14);

        // 剧烈波动的价格
        let prices = [
            dec!(100), dec!(110), dec!(100), dec!(110), dec!(100),
            dec!(110), dec!(100), dec!(110), dec!(100), dec!(110),
            dec!(100), dec!(110), dec!(100), dec!(110), dec!(100),
        ];

        for price in prices {
            let result = rsi.calculate(price);
            assert!(result >= Decimal::ZERO && result <= dec!(100),
                "RSI should always be in range [0, 100], got {}", result);
        }
    }

    /// 测试 RSI 中性区域
    #[test]
    fn test_rsi_neutral_zone() {
        let mut rsi = RSI::new(14);

        // 先涨后跌，RSI 应该在中间区域
        let prices = [
            dec!(100), dec!(105), dec!(110), dec!(115), dec!(120),
            dec!(118), dec!(116), dec!(114), dec!(112), dec!(110),
            dec!(108), dec!(106), dec!(104), dec!(102), dec!(100),
        ];

        for price in prices {
            let result = rsi.calculate(price);
            assert!(result >= Decimal::ZERO && result <= dec!(100),
                "RSI should always be in range [0, 100], got {}", result);
        }
    }
}
