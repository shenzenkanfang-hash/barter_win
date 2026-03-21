//! 波动率检测器 - 1m O-C 和 15m Close-Close 变化率

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use crate::kline_1m::KLineSynthesizer;
use crate::models::types::{KLine, Period, VolatilityStats, Tick};

pub struct VolatilityDetector {
    /// 1m K线合成器
    kline_1m: KLineSynthesizer,
    /// 15m K线合成器 (内部合成)
    kline_15m: KLineSynthesizer,
    /// 15m 滑动窗口 (保留最近2根)
    kline_15m_window: Vec<KLine>,
    /// 1m K线计数器 (用于判断是否满15根)
    kline_1m_count: u32,
    /// 阈值: 1m 3%
    threshold_1m: Decimal,
    /// 阈值: 15m 6%
    threshold_15m: Decimal,
}

impl VolatilityDetector {
    pub fn new(symbol: String) -> Self {
        Self {
            kline_1m: KLineSynthesizer::new(symbol.clone(), Period::Minute(1)),
            kline_15m: KLineSynthesizer::new(symbol.clone(), Period::Minute(15)),
            kline_15m_window: Vec::with_capacity(2),
            kline_1m_count: 0,
            threshold_1m: dec!(0.03),
            threshold_15m: dec!(0.06),
        }
    }

    /// 更新并检测波动率
    pub fn update(&mut self, price: Decimal, timestamp: chrono::DateTime<chrono::Utc>) -> VolatilityStats {
        // 1. 创建 Tick
        let tick = Tick {
            symbol: self.kline_1m.symbol.clone(),
            price,
            qty: dec!(1),
            timestamp,
            kline_1m: None,
            kline_15m: None,
            kline_1d: None,
        };

        // 2. 更新 1m K线
        let completed_1m = self.kline_1m.update(&tick);
        let kline_1m = match self.kline_1m.current_kline() {
            Some(k) => k.clone(),
            None => return VolatilityStats::default(),
        };

        // 3. 计算 1m O-C 变化率
        let vol_1m = self.calc_1m_volatility(&kline_1m);

        // 4. 更新 15m K线 (每15根1m K线合成1根15m)
        if completed_1m.is_some() {
            self.kline_1m_count += 1;
            if self.kline_1m_count >= 15 {
                let _ = self.kline_15m.update(&tick);
                self.kline_1m_count = 0;
            }
        }

        // 5. 检查15m是否完成，更新窗口
        if let Some(completed_15m) = self.kline_15m.update(&tick) {
            self.update_15m_window(completed_15m);
        }

        // 6. 计算 15m Close-Close 变化率
        let vol_15m = self.calc_15m_volatility();

        // 7. 判断是否高波动
        let is_high = vol_1m >= self.threshold_1m || vol_15m >= self.threshold_15m;

        VolatilityStats {
            is_high_volatility: is_high,
            vol_1m,
            vol_15m,
        }
    }

    fn calc_1m_volatility(&self, kline: &KLine) -> Decimal {
        if kline.open > dec!(0) {
            (kline.close - kline.open).abs() / kline.open
        } else {
            dec!(0)
        }
    }

    fn calc_15m_volatility(&self) -> Decimal {
        if self.kline_15m_window.len() < 2 {
            return dec!(0);
        }

        let prev = &self.kline_15m_window[0];
        let curr = &self.kline_15m_window[1];

        if prev.close > dec!(0) {
            (curr.close - prev.close).abs() / prev.close
        } else {
            dec!(0)
        }
    }

    fn update_15m_window(&mut self, kline: KLine) {
        if self.kline_15m_window.len() >= 2 {
            self.kline_15m_window.remove(0);
        }
        self.kline_15m_window.push(kline);
    }

    pub fn thresholds(&self) -> (Decimal, Decimal) {
        (self.threshold_1m, self.threshold_15m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    #[test]
    fn test_volatility_detector_default() {
        let detector = VolatilityDetector::new("BTCUSDT".to_string());
        let (th1m, th15m) = detector.thresholds();
        assert_eq!(th1m, dec!(0.03));
        assert_eq!(th15m, dec!(0.06));
    }

    #[test]
    fn test_volatility_detector_update() {
        let mut detector = VolatilityDetector::new("BTCUSDT".to_string());
        let timestamp = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();

        // 初始更新 - 返回默认stats因为没有当前kline
        let stats = detector.update(dec!(100), timestamp);
        assert!(!stats.is_high_volatility);
        assert_eq!(stats.vol_1m, dec!(0));
        assert_eq!(stats.vol_15m, dec!(0));
    }

    #[test]
    fn test_volatility_high_1m() {
        let mut detector = VolatilityDetector::new("BTCUSDT".to_string());
        let timestamp = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();

        // 价格从100涨到104 (4% > 3% threshold)
        detector.update(dec!(100), timestamp);
        let stats = detector.update(dec!(104), timestamp);

        // 1m波动率应该很高
        assert!(stats.vol_1m >= dec!(0.03));
    }

    #[test]
    fn test_volatility_stats_default() {
        let stats = VolatilityStats::default();
        assert!(!stats.is_high_volatility);
        assert_eq!(stats.vol_1m, dec!(0));
        assert_eq!(stats.vol_15m, dec!(0));
    }
}