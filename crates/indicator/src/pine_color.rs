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

#[cfg(test)]
mod tests {
    use super::*;

    /// E1.3 PineColor 判断测试 - 验证 Green/Red/Neutral 判断正确
    ///
    /// 测试场景: 输入多空趋势数据，验证颜色判断

    /// 测试 Purple (RSI 超买)
    #[test]
    fn test_pine_color_rsi_overbought() {
        // RSI >= 70 应该返回 Purple
        let result = PineColorDetector::detect(dec!(10), dec!(5), dec!(75));
        assert_eq!(result, PineColor::Purple, "RSI overbought should return Purple");
    }

    /// 测试 Purple (RSI 超卖)
    #[test]
    fn test_pine_color_rsi_oversold() {
        // RSI <= 30 应该返回 Purple
        let result = PineColorDetector::detect(dec!(-10), dec!(-5), dec!(25));
        assert_eq!(result, PineColor::Purple, "RSI oversold should return Purple");
    }

    /// 测试 PureGreen (强势多头)
    #[test]
    fn test_pine_color_pure_green() {
        // MACD >= Signal && MACD >= 0 => PureGreen
        let result = PineColorDetector::detect(dec!(10), dec!(5), dec!(50));
        assert_eq!(result, PineColor::PureGreen, "Strong bullish should return PureGreen");
    }

    /// 测试 LightGreen (弱势多头)
    #[test]
    fn test_pine_color_light_green() {
        // MACD <= Signal && MACD >= 0 => LightGreen
        let result = PineColorDetector::detect(dec!(5), dec!(10), dec!(50));
        assert_eq!(result, PineColor::LightGreen, "Weak bullish should return LightGreen");
    }

    /// 测试 PureRed (强势空头)
    #[test]
    fn test_pine_color_pure_red() {
        // MACD <= Signal && MACD <= 0 => PureRed
        let result = PineColorDetector::detect(dec!(-10), dec!(-5), dec!(50));
        assert_eq!(result, PineColor::PureRed, "Strong bearish should return PureRed");
    }

    /// 测试 LightRed (弱势空头)
    #[test]
    fn test_pine_color_light_red() {
        // MACD >= Signal && MACD <= 0 => LightRed
        let result = PineColorDetector::detect(dec!(-5), dec!(-10), dec!(50));
        assert_eq!(result, PineColor::LightRed, "Weak bearish should return LightRed");
    }

    /// 测试边界值 RSI = 70
    #[test]
    fn test_pine_color_rsi_boundary_70() {
        let result = PineColorDetector::detect(dec!(10), dec!(5), dec!(70));
        assert_eq!(result, PineColor::Purple, "RSI = 70 should return Purple");
    }

    /// 测试边界值 RSI = 30
    #[test]
    fn test_pine_color_rsi_boundary_30() {
        let result = PineColorDetector::detect(dec!(10), dec!(5), dec!(30));
        assert_eq!(result, PineColor::Purple, "RSI = 30 should return Purple");
    }

    /// 测试 MACD = 0 边界
    #[test]
    fn test_pine_color_macd_zero() {
        // MACD = 0, MACD >= Signal => LightRed
        let result = PineColorDetector::detect(dec!(0), dec!(-5), dec!(50));
        assert_eq!(result, PineColor::LightRed, "MACD = 0 with signal lower should be LightRed");
    }

    /// 测试 MACD = Signal 边界
    #[test]
    fn test_pine_color_macd_equals_signal() {
        // MACD = Signal && MACD >= 0 => PureGreen
        let result = PineColorDetector::detect(dec!(10), dec!(10), dec!(50));
        assert_eq!(result, PineColor::PureGreen, "MACD = Signal with positive should be PureGreen");
    }

    /// 测试 Neutral 区域 (中等 RSI)
    #[test]
    fn test_pine_color_neutral_zone() {
        // RSI 在 30-70 之间，不是极值，应该根据 MACD 判断
        let result = PineColorDetector::detect(dec!(5), dec!(10), dec!(55));
        assert_eq!(result, PineColor::LightGreen, "Middle RSI with MACD <= Signal should be LightGreen");
    }
}
