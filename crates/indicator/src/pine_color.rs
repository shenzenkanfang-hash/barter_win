use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PineColor {
    PureGreen,
    LightGreen,
    PureRed,
    LightRed,
    Purple,
}

pub struct PineColorDetector;

/// Pine颜色检测器
///
/// 根据 MACD 和 RSI 判断市场趋势颜色:
/// - Purple: RSI 极值区域 (>=70 或 <=30)
/// - PureGreen: 强势多头 (MACD >= Signal && MACD >= 0)
/// - LightGreen: 弱势多头 (MACD <= Signal && MACD >= 0)
/// - PureRed: 强势空头 (MACD <= Signal && MACD <= 0)
/// - LightRed: 弱势空头 (MACD >= Signal && MACD <= 0)
impl PineColorDetector {
    pub fn detect(macd: Decimal, signal: Decimal, rsi: Decimal) -> PineColor {
        // RSI 极值优先判断
        if rsi >= dec!(70) || rsi <= dec!(30) {
            return PineColor::Purple;
        }

        // 按 MACD 判断趋势颜色
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
