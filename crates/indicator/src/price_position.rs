use rust_decimal::Decimal;

#[derive(Debug, Clone)]
pub struct PricePosition {
    #[allow(dead_code)]
    period: usize,
}

impl PricePosition {
    pub fn new(period: usize) -> Self {
        Self { period }
    }

    pub fn calculate(&self, close: Decimal, high: Decimal, low: Decimal) -> Decimal {
        if high == low {
            return Decimal::ZERO;
        }
        (close - low) / (high - low)
    }
}
