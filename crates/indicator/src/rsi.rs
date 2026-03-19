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

        let gain = if change > Decimal::ZERO { change } else { -change };
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
