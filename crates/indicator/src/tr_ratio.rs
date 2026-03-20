use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::VecDeque;

/// TR-Ratio 指标
///
/// TR-Ratio 是两个不同周期 TR (True Range) 的比值。
/// 用于判断波动率的极端程度:
///
/// - TR-Ratio > 1: 当前周期波动大于参照周期
/// - TR-Ratio > 2: 极端波动
///
/// 计算公式: TR-Ratio = TR(short) / TR(long)
///
/// 注: 完整实现需根据 indicator_1m/indicator_calc.py 调整。
/// 当前为增量计算框架。
#[derive(Debug, Clone)]
pub struct TRRatio {
    /// 短周期 TR 队列
    tr_short_history: VecDeque<Decimal>,
    /// 长周期 TR 队列
    tr_long_history: VecDeque<Decimal>,
    /// 短周期窗口
    short_window: usize,
    /// 长周期窗口
    long_window: usize,
    /// 当前 TR 值
    current_tr: Decimal,
    /// 上一个收盘价
    prev_close: Decimal,
}

impl TRRatio {
    /// 创建新的 TR-Ratio 计算器
    pub fn new(short_window: usize, long_window: usize) -> Self {
        Self {
            tr_short_history: VecDeque::with_capacity(long_window),
            tr_long_history: VecDeque::with_capacity(long_window),
            short_window,
            long_window,
            current_tr: dec!(0),
            prev_close: dec!(0),
        }
    }

    /// 计算 True Range
    ///
    /// TR = max(H - L, |H - PC|, |L - PC|)
    /// 其中 PC 是上一个收盘价
    fn calculate_tr(&self, high: Decimal, low: Decimal) -> Decimal {
        if self.prev_close <= dec!(0) {
            return high - low;
        }

        let hl = high - low;
        let hpc = (high - self.prev_close).abs();
        let lpc = (low - self.prev_close).abs();

        hl.max(hpc).max(lpc)
    }

    /// 增量计算 TR-Ratio
    ///
    /// 输入: 当前 K 线的高低价
    /// 返回: (tr_ratio, is_extreme)
    pub fn calculate(&mut self, high: Decimal, low: Decimal, close: Decimal) -> (Decimal, bool) {
        // 计算当前 TR
        let tr = self.calculate_tr(high, low);
        self.current_tr = tr;

        // 更新短周期队列
        if self.tr_short_history.len() >= self.short_window {
            self.tr_short_history.pop_front();
        }
        self.tr_short_history.push_back(tr);

        // 更新长周期队列
        if self.tr_long_history.len() >= self.long_window {
            self.tr_long_history.pop_front();
        }
        self.tr_long_history.push_back(tr);

        // 更新上一个收盘价
        self.prev_close = close;

        // 计算 TR 均值
        let tr_short_avg = self.average(&self.tr_short_history);
        let tr_long_avg = self.average(&self.tr_long_history);

        // 计算 TR-Ratio
        let ratio = if tr_long_avg > dec!(0) {
            tr_short_avg / tr_long_avg
        } else {
            dec!(0)
        };

        // 判断是否极端 (> 1 为极端，> 2 为很强极端)
        let is_extreme = ratio > dec!(1);

        (ratio, is_extreme)
    }

    /// 计算队列平均值
    fn average(&self, deque: &VecDeque<Decimal>) -> Decimal {
        if deque.is_empty() {
            return dec!(0);
        }
        let sum: Decimal = deque.iter().sum();
        sum / Decimal::from(deque.len())
    }

    /// 获取当前 TR 值
    pub fn current_tr(&self) -> Decimal {
        self.current_tr
    }

    /// 获取短周期 TR 均值
    pub fn tr_short_avg(&self) -> Decimal {
        self.average(&self.tr_short_history)
    }

    /// 获取长周期 TR 均值
    pub fn tr_long_avg(&self) -> Decimal {
        self.average(&self.tr_long_history)
    }
}

impl Default for TRRatio {
    fn default() -> Self {
        // 60min / 5h 比率 (60分钟 vs 300分钟)
        Self::new(60, 300)
    }
}

/// TR-Ratio 信号
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TRRatioSignal {
    /// 极端波动 - TR-Ratio > 2
    Extreme,
    /// 波动较大 - TR-Ratio > 1
    High,
    /// 正常波动 - TR-Ratio <= 1
    Normal,
}

/// 分析 TR-Ratio 信号
pub fn analyze_tr_ratio(ratio: Decimal) -> TRRatioSignal {
    if ratio > dec!(2) {
        TRRatioSignal::Extreme
    } else if ratio > dec!(1) {
        TRRatioSignal::High
    } else {
        TRRatioSignal::Normal
    }
}

/// 常用 TR 周期组合
pub mod windows {
    use super::*;

    /// 10分钟 / 1小时 (用于分钟级)
    pub fn min10_hour1() -> TRRatio {
        TRRatio::new(10, 60)
    }

    /// 60分钟 / 5小时 (用于马丁策略)
    pub fn hour1_5hour() -> TRRatio {
        TRRatio::new(60, 300)
    }

    /// 5分钟 / 1小时 (高频)
    pub fn min5_hour1() -> TRRatio {
        TRRatio::new(5, 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tr_ratio_basic() {
        let mut tr_ratio = TRRatio::new(3, 5);
        let (ratio, is_extreme) = tr_ratio.calculate(
            dec!(105),  // high
            dec!(95),   // low
            dec!(100),  // close
        );
        // 第一个数据，ratio 应该是 0
        assert_eq!(ratio, dec!(0));
    }

    #[test]
    fn test_tr_ratio_stable() {
        let mut tr_ratio = TRRatio::new(2, 3);

        // 喂入相似数据，TR 比率应该接近 1
        for i in 0..5 {
            let price = dec!(100) + Decimal::from(i);
            tr_ratio.calculate(price + dec!(1), price - dec!(1), price);
        }

        let (ratio, _) = tr_ratio.calculate(dec!(103), dec!(97), dec!(100));
        // 稳定价格下，ratio 应该接近 1
        assert!(ratio > dec!(0));
    }

    #[test]
    fn test_analyze_tr_ratio() {
        assert_eq!(analyze_tr_ratio(dec!(2.5)), TRRatioSignal::Extreme);
        assert_eq!(analyze_tr_ratio(dec!(1.5)), TRRatioSignal::High);
        assert_eq!(analyze_tr_ratio(dec!(0.8)), TRRatioSignal::Normal);
    }
}
