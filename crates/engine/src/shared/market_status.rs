use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

/// 市场状态枚举
///
/// 检测优先级: INVALID > PIN > RANGE > TREND
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarketStatus {
    /// 趋势状态 - 有明确方向
    TREND,
    /// 震荡状态 - 低波动、低动能
    RANGE,
    /// 插针状态 - 极端波动
    PIN,
    /// 数据无效 - 超时/异常
    INVALID,
}

/// 插针强度等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PinIntensity {
    /// 弱插针 (>= 2 条件)
    WEAK,
    /// 中插针 (>= 3 条件)
    MODERATE,
    /// 强插针 (>= 4 条件)
    STRONG,
    /// 无插针
    NONE,
}

/// 插针检测结果
#[derive(Debug, Clone)]
pub struct PinDetection {
    /// 插针强度
    pub intensity: PinIntensity,
    /// 插针条件满足数量
    pub conditions_met: u8,
    /// 是否为向上插针
    pub is_up: bool,
    /// 是否为向下插针
    pub is_down: bool,
}

/// 市场状态检测器
///
/// 根据多指标判断市场状态 (PIN/RANGE/TREND/INVALID)。
/// 检测优先级: INVALID > PIN > RANGE > TREND
///
/// 设计依据: 设计文档 16.10.3
pub struct MarketStatusDetector {
    /// TR 比率阈值 (判断极端波动)
    tr_ratio_threshold: Decimal,
    /// Z-Score 阈值 (判断价格偏离)
    zscore_threshold: Decimal,
    /// 波动率阈值
    volatility_threshold: Decimal,
    /// 数据超时时间 (秒)
    data_timeout_seconds: i64,
}

impl Default for MarketStatusDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl MarketStatusDetector {
    /// 创建市场状态检测器
    pub fn new() -> Self {
        Self {
            tr_ratio_threshold: dec!(1.0),      // TR 比率 > 1 表示极端波动
            zscore_threshold: dec!(2.0),         // Z-Score > 2 表示极端偏离
            volatility_threshold: dec!(0.03),    // 3% 波动率阈值
            data_timeout_seconds: 180,           // 180 秒超时
        }
    }

    /// 检测市场状态
    ///
    /// 综合 TR 比率、波动率、价格位置等指标判断市场状态。
    pub fn detect(
        &self,
        tr_ratio: Decimal,           // TR 比率
        zscore: Decimal,             // Z-Score
        volatility: Decimal,        // 波动率
        price_position: Decimal,     // 价格位置 0-100
        is_data_valid: bool,         // 数据是否有效
        last_update_ts: i64,         // 最后更新时间戳
        current_ts: i64,             // 当前时间戳
    ) -> MarketStatus {
        // 1. 数据有效性检查 (最高优先级)
        if !is_data_valid || current_ts - last_update_ts > self.data_timeout_seconds {
            return MarketStatus::INVALID;
        }

        // 2. 插针检测 (第二优先级)
        let pin_detection = self.detect_pin(zscore, volatility, price_position, tr_ratio);
        if pin_detection.intensity != PinIntensity::NONE {
            return MarketStatus::PIN;
        }

        // 3. 震荡检测
        if self.is_range_market(zscore, volatility, tr_ratio) {
            return MarketStatus::RANGE;
        }

        // 4. 趋势检测 (默认)
        MarketStatus::TREND
    }

    /// 检测插针状态
    ///
    /// 插针条件:
    /// - Z-Score > 2 或 < -2
    /// - 波动率 > 阈值
    /// - TR 比率 > 1
    /// - 价格位置极端 (接近 0 或 100)
    pub fn detect_pin(
        &self,
        zscore: Decimal,
        volatility: Decimal,
        price_position: Decimal,
        tr_ratio: Decimal,
    ) -> PinDetection {
        let mut conditions_met: u8 = 0;
        let mut is_up = false;
        let mut is_down = false;

        // 条件1: Z-Score 极端
        if zscore > self.zscore_threshold || zscore < -self.zscore_threshold {
            conditions_met += 1;
            if zscore > dec!(0) {
                is_up = true;
            } else {
                is_down = true;
            }
        }

        // 条件2: 波动率极端
        if volatility > self.volatility_threshold * dec!(2) {
            conditions_met += 1;
        }

        // 条件3: TR 比率极端
        if tr_ratio > self.tr_ratio_threshold {
            conditions_met += 1;
        }

        // 条件4: 价格位置极端
        if price_position < dec!(10) || price_position > dec!(90) {
            conditions_met += 1;
            if price_position < dec!(10) {
                is_down = true;
            } else {
                is_up = true;
            }
        }

        // 确定插针强度
        let intensity = match conditions_met {
            0 => PinIntensity::NONE,
            1 | 2 => PinIntensity::WEAK,
            3 => PinIntensity::MODERATE,
            _ => PinIntensity::STRONG,
        };

        PinDetection {
            intensity,
            conditions_met,
            is_up,
            is_down,
        }
    }

    /// 判断是否为震荡市场
    ///
    /// 震荡条件:
    /// - Z-Score 接近 0
    /// - 波动率较低
    /// - TR 比率 < 1
    fn is_range_market(&self, zscore: Decimal, volatility: Decimal, tr_ratio: Decimal) -> bool {
        // Z-Score 接近 0 (绝对值 < 0.5)
        let zscore_near_zero = zscore.abs() < dec!(0.5);

        // 波动率较低
        let low_volatility = volatility < self.volatility_threshold;

        // TR 比率 < 1
        let low_tr_ratio = tr_ratio < self.tr_ratio_threshold;

        zscore_near_zero && low_volatility && low_tr_ratio
    }

    /// 检测趋势方向
    ///
    /// 返回: (is_trending, trend_direction)
    /// - is_trending: 是否处于趋势状态
    /// - trend_direction: "up", "down", "neutral"
    pub fn detect_trend(
        &self,
        ema_fast: Decimal,
        ema_slow: Decimal,
        pine_color: &str,  // "green", "red", "neutral"
    ) -> (bool, &'static str) {
        let is_trending = (ema_fast - ema_slow).abs() > dec!(0); // 有差值

        let trend_direction = if ema_fast > ema_slow * dec!(1.001) {
            "up"
        } else if ema_fast < ema_slow * dec!(0.999) {
            "down"
        } else {
            "neutral"
        };

        // 如果 Pine 颜色指示方向，与 EMA 趋势结合判断
        let confirmed_direction = match (trend_direction, pine_color) {
            ("up", "green") | ("down", "red") => trend_direction,
            ("down", "green") | ("up", "red") => "neutral", // 矛盾时认为是震荡
            _ => trend_direction,
        };

        (is_trending, confirmed_direction)
    }

    /// 设置 TR 比率阈值
    pub fn set_tr_ratio_threshold(&mut self, threshold: Decimal) {
        self.tr_ratio_threshold = threshold;
    }

    /// 设置 Z-Score 阈值
    pub fn set_zscore_threshold(&mut self, threshold: Decimal) {
        self.zscore_threshold = threshold;
    }

    /// 设置波动率阈值
    pub fn set_volatility_threshold(&mut self, threshold: Decimal) {
        self.volatility_threshold = threshold;
    }

    /// 获取插针强度描述
    pub fn pin_intensity_to_str(intensity: PinIntensity) -> &'static str {
        match intensity {
            PinIntensity::NONE => "none",
            PinIntensity::WEAK => "weak",
            PinIntensity::MODERATE => "moderate",
            PinIntensity::STRONG => "strong",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_pin_weak() {
        let detector = MarketStatusDetector::new();
        let pin = detector.detect_pin(
            dec!(2.5),     // zscore (超过阈值)
            dec!(0.01),    // volatility (低)
            dec!(5),       // price_position (极端)
            dec!(0.5),     // tr_ratio (正常)
        );
        assert_eq!(pin.intensity, PinIntensity::WEAK);
        assert!(pin.is_down);
    }

    #[test]
    fn test_detect_pin_strong() {
        let detector = MarketStatusDetector::new();
        let pin = detector.detect_pin(
            dec!(3.0),     // zscore (极端)
            dec!(0.08),    // volatility (极端)
            dec!(3),       // price_position (极端)
            dec!(1.5),     // tr_ratio (极端)
        );
        assert_eq!(pin.intensity, PinIntensity::STRONG);
    }

    #[test]
    fn test_detect_market_status_trend() {
        let detector = MarketStatusDetector::new();
        let status = detector.detect(
            dec!(0.5),     // tr_ratio
            dec!(1.0),     // zscore
            dec!(0.02),    // volatility
            dec!(50),      // price_position (中间)
            true,          // is_data_valid
            0,             // last_update_ts
            100,           // current_ts
        );
        assert_eq!(status, MarketStatus::TREND);
    }

    #[test]
    fn test_detect_market_status_invalid() {
        let detector = MarketStatusDetector::new();
        let status = detector.detect(
            dec!(0.5),
            dec!(1.0),
            dec!(0.02),
            dec!(50),
            false,         // is_data_valid = false
            0,
            100,
        );
        assert_eq!(status, MarketStatus::INVALID);
    }

    #[test]
    fn test_detect_trend() {
        let detector = MarketStatusDetector::new();
        let (is_trending, direction) = detector.detect_trend(
            dec!(50500),   // ema_fast
            dec!(50000),   // ema_slow
            "green",       // pine_color
        );
        assert!(is_trending);
        assert_eq!(direction, "up");
    }
}
