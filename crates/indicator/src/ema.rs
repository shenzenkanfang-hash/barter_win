use rust_decimal::Decimal;
use rust_decimal_macros::dec;

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
