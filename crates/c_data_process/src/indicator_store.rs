//! IndicatorStore - 指标存储统一访问接口
//!
//! 统一访问分钟级和日线级指标数据
//!
//! 与 SignalProcessor 集成，提供统一的异步查询接口。

#![forbid(unsafe_code)]

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

// Re-export actual indicator types from existing modules
pub use crate::min::trend::Indicator1mOutput;
pub use crate::day::trend::BigCycleIndicators as Indicator1dOutput;

/// IndicatorStore trait - 统一访问接口
///
/// 提供分钟级和日线级指标的异步查询接口。
#[async_trait]
pub trait IndicatorStore: Send + Sync {
    /// 获取分钟级指标
    async fn get_min(&self, symbol: &str) -> Option<Indicator1mOutput>;

    /// 获取日线级指标
    async fn get_day(&self, symbol: &str) -> Option<Indicator1dOutput>;

    /// 获取所有分钟级指标
    async fn get_all_min(&self) -> HashMap<String, Indicator1mOutput>;

    /// 检查分钟级指标是否就绪
    async fn is_min_ready(&self, symbol: &str) -> bool;

    /// 检查日线级指标是否就绪
    async fn is_day_ready(&self, symbol: &str) -> bool;
}

/// SignalProcessorIndicatorStore - SignalProcessor 的 IndicatorStore 适配器
///
/// 将 SignalProcessor 适配为 IndicatorStore trait，
/// 提供统一的异步指标查询接口。
pub struct SignalProcessorIndicatorStore {
    inner: Arc<crate::processor::SignalProcessor>,
}

impl SignalProcessorIndicatorStore {
    /// 创建新的适配器
    pub fn new(inner: Arc<crate::processor::SignalProcessor>) -> Self {
        Self { inner }
    }

    /// 从 Arc<SignalProcessor> 创建
    pub fn from_arc(inner: Arc<crate::processor::SignalProcessor>) -> Self {
        Self::new(inner)
    }
}

#[async_trait]
impl IndicatorStore for SignalProcessorIndicatorStore {
    async fn get_min(&self, symbol: &str) -> Option<Indicator1mOutput> {
        self.inner.min_get_output(symbol)
    }

    async fn get_day(&self, symbol: &str) -> Option<Indicator1dOutput> {
        // 日级指标通过 day_get_pine 获取
        self.inner.day_get_pine(symbol)
    }

    async fn get_all_min(&self) -> HashMap<String, Indicator1mOutput> {
        // 获取所有注册的 symbol
        let symbols = self.inner.registered_symbols();
        let mut result = HashMap::new();

        for symbol in symbols {
            if let Some(output) = self.inner.min_get_output(&symbol) {
                result.insert(symbol, output);
            }
        }

        result
    }

    async fn is_min_ready(&self, symbol: &str) -> bool {
        self.inner.min_is_ready(symbol)
    }

    async fn is_day_ready(&self, symbol: &str) -> bool {
        self.inner.day_is_ready(symbol)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn create_test_min_output() -> Indicator1mOutput {
        Indicator1mOutput {
            mid: dec!(50000),
            velocity: dec!(0.001),
            acceleration: dec!(0.0001),
            a_smooth: dec!(0.0002),
            power: dec!(0.0000002),
            velocity_percentile: dec!(60),
            acc_percentile: dec!(50),
            power_percentile: dec!(55),
            zscore_1h_1m: dec!(0.5),
            zscore_14_1m: dec!(0.3),
            pos_norm_60: dec!(55),
            tr_base_10min: dec!(0.001),
            tr_ratio_10min_1h: dec!(1.0),
            tr_ratio_zscore_10min_1h: dec!(0.2),
            jerk: dec!(0.00001),
            norm_jerk: dec!(0.1),
            market_force: dec!(0.05),
            acc_efficiency: dec!(0.8),
            acc_div_signal: dec!(0),
            trend_dir: dec!(1),
        }
    }

    #[test]
    fn test_indicator_1m_output_fields() {
        let output = create_test_min_output();
        assert_eq!(output.mid, dec!(50000));
        assert_eq!(output.velocity, dec!(0.001));
        assert!(output.pos_norm_60 > dec!(0));
    }

    #[tokio::test]
    async fn test_signal_processor_indicator_store_integration() {
        // 创建 SignalProcessor
        let processor = Arc::new(crate::processor::SignalProcessor::new());
        processor.register_symbol("BTCUSDT");

        // 创建适配器
        let store = SignalProcessorIndicatorStore::from_arc(processor.clone());

        // 注册后可以查询
        let min_output = store.get_min("BTCUSDT").await;
        assert!(min_output.is_some());

        let all_min = store.get_all_min().await;
        assert!(all_min.contains_key("BTCUSDT"));

        // 检查就绪状态
        let min_ready = store.is_min_ready("BTCUSDT").await;
        assert!(min_ready);
    }
}
