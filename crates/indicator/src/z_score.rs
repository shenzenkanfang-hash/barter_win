use rust_decimal::Decimal;
use rust_decimal::MathematicalOps;
use rust_decimal_macros::dec;
use std::collections::VecDeque;

/// Z-Score 指标
///
/// Z-Score 衡量当前值与均值的标准差偏移量。
/// 用于检测价格极端偏离，可用于:
/// - 极端波动检测 (Z-Score > 2 或 < -2)
/// - 趋势转折信号
/// - 统计套利
///
/// 计算公式: Z = (x - μ) / σ
///
/// 注: 完整实现需要历史数据窗口，当前为增量计算框架。
/// 实际计算逻辑需根据 indicator_1m/indicator_calc.py 调整。
#[derive(Debug, Clone)]
pub struct ZScore {
    /// 窗口大小
    window: usize,
    /// 历史价格队列
    history: VecDeque<Decimal>,
    /// 当前均值
    mean: Decimal,
    /// 当前标准差
    std: Decimal,
    /// 样本数
    count: usize,
}

impl ZScore {
    /// 创建新的 Z-Score 计算器
    pub fn new(window: usize) -> Self {
        Self {
            window,
            history: VecDeque::with_capacity(window),
            mean: dec!(0),
            std: dec!(0),
            count: 0,
        }
    }

    /// 增量计算 Z-Score
    ///
    /// 返回: (z_score, is_extreme)
    /// - z_score: Z-Score 值
    /// - is_extreme: 是否为极端值 (|Z| > 2)
    pub fn calculate(&mut self, price: Decimal) -> (Decimal, bool) {
        // 添加新价格
        if self.history.len() >= self.window {
            // 移除最旧的值
            self.history.pop_front();
        }
        self.history.push_back(price);
        self.count = self.history.len();

        if self.count < 2 {
            return (dec!(0), false);
        }

        // 计算均值 (Welford's 在线算法)
        let n = Decimal::from(self.count);
        let old_mean = self.mean;

        // 增量更新均值
        let delta = price - old_mean;
        self.mean = old_mean + delta / n;

        // 增量更新标准差 (Welford's 算法)
        let delta2 = price - self.mean;
        let old_std_sq = self.std * self.std * Decimal::from(self.count - 1);
        let new_std_sq = if self.count > 1 {
            old_std_sq + delta * delta2
        } else {
            dec!(0)
        };

        if self.count > 1 {
            self.std = (new_std_sq / Decimal::from(self.count - 1)).sqrt().unwrap_or(dec!(0));
        } else {
            self.std = dec!(0);
        }

        // 计算 Z-Score
        let z_score = if self.std > dec!(0) {
            (price - self.mean) / self.std
        } else {
            dec!(0)
        };

        // 判断是否极端 (|Z| > 2)
        let is_extreme = z_score.abs() > dec!(2);

        (z_score, is_extreme)
    }

    /// 获取当前均值
    pub fn mean(&self) -> Decimal {
        self.mean
    }

    /// 获取当前标准差
    pub fn std(&self) -> Decimal {
        self.std
    }

    /// 获取样本数
    pub fn count(&self) -> usize {
        self.count
    }

    /// 获取历史数据长度
    pub fn len(&self) -> usize {
        self.history.len()
    }

    /// 判断是否为空
    pub fn is_empty(&self) -> bool {
        self.history.is_empty()
    }
}

impl Default for ZScore {
    fn default() -> Self {
        Self::new(14)
    }
}

/// Z-Score 阈值常量
pub struct ZScoreThreshold {
    /// 极端值阈值 (默认 2.0)
    pub extreme_threshold: Decimal,
    /// 警告阈值 (默认 1.5)
    pub warning_threshold: Decimal,
    /// Z-Score 最大限制
    pub max_limit: Decimal,
}

impl Default for ZScoreThreshold {
    fn default() -> Self {
        Self {
            extreme_threshold: dec!(2.0),
            warning_threshold: dec!(1.5),
            max_limit: dec!(100.0),
        }
    }
}

/// 分析 Z-Score 信号
pub fn analyze_zscore(z: Decimal, threshold: &ZScoreThreshold) -> ZScoreSignal {
    let z_clamped = z.abs().min(threshold.max_limit);

    if z.abs() > threshold.extreme_threshold {
        if z > dec!(0) {
            ZScoreSignal::ExtremeUp
        } else {
            ZScoreSignal::ExtremeDown
        }
    } else if z.abs() > threshold.warning_threshold {
        if z > dec!(0) {
            ZScoreSignal::WarningUp
        } else {
            ZScoreSignal::WarningDown
        }
    } else if z.abs() < dec!(0.5) {
        ZScoreSignal::Neutral
    } else {
        ZScoreSignal::Normal
    }
}

/// Z-Score 信号
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZScoreSignal {
    /// 极端向上 - Z > 2
    ExtremeUp,
    /// 极端向下 - Z < -2
    ExtremeDown,
    /// 警告向上 - Z > 1.5
    WarningUp,
    /// 警告向下 - Z < -1.5
    WarningDown,
    /// 正常范围
    Normal,
    /// 中性 - Z 接近 0
    Neutral,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zscore_basic() {
        let mut zscore = ZScore::new(5);
        let (z, extreme) = zscore.calculate(dec!(100));
        assert_eq!(extreme, false);
    }

    #[test]
    fn test_zscore_extreme() {
        let mut zscore = ZScore::new(5);
        // 喂入稳定数据
        for _ in 0..4 {
            zscore.calculate(dec!(100));
        }
        // 最后一个极端值
        let (z, extreme) = zscore.calculate(dec!(200));
        // 200 相对 100 会有很大的 Z-Score
        assert!(z > dec!(0));
        // extreme 可能 true 或 false，取决于窗口内数据
    }

    #[test]
    fn test_analyze_zscore() {
        let threshold = ZScoreThreshold::default();
        assert_eq!(analyze_zscore(dec!(3.0), &threshold), ZScoreSignal::ExtremeUp);
        assert_eq!(analyze_zscore(dec!(-3.0), &threshold), ZScoreSignal::ExtremeDown);
        assert_eq!(analyze_zscore(dec!(1.6), &threshold), ZScoreSignal::WarningUp);
        assert_eq!(analyze_zscore(dec!(0.3), &threshold), ZScoreSignal::Neutral);
    }
}
