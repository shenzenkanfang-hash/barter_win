use rust_decimal::Decimal;
use rust_decimal_macros::dec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PineColor {
    PureGreen,
    LightGreen,
    PureRed,
    LightRed,
    Purple,
}

pub struct PineColorDetector;

impl PineColorDetector {
    pub fn detect(macd: Decimal, signal: Decimal, rsi: Decimal) -> PineColor {
        if rsi >= dec!(70) || rsi <= dec!(30) {
            return PineColor::Purple;
        }

        if macd >= signal && macd >= Decimal::ZERO {
            PineColor::PureGreen
        } else if macd <= signal && macd >= Decimal::ZERO {
            PineColor::LightGreen
        } else if macd <= signal && macd <= Decimal::ZERO {
            PineColor::PureRed
        } else {
            PineColor::LightRed
        }
    }
}
