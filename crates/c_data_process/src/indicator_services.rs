//! 指标服务层 - 独立事件驱动 + 串行批量
//!
//! - MinIndicatorService: 事件触发计算（策略协程按需触发）
//! - DayIndicatorService: 串行批量计算（5分钟循环）
//!
//! 对齐设计规格 第四章"指标层详细设计"

#![forbid(unsafe_code)]

use async_trait::async_trait;
use parking_lot::RwLock;
use rust_decimal::Decimal;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;

use crate::indicator_store::{Indicator1dOutput, Indicator1mOutput, IndicatorStore};
use crate::processor::SignalProcessor;
use x_data::state::StateCenter;

// ============================================================================
// MinIndicatorService - 分钟级指标事件触发服务
// ============================================================================

/// Kline 输入参数（来自 SharedStore）
#[derive(Debug, Clone)]
pub struct KlineInput {
    /// 最高价
    pub high: Decimal,
    /// 最低价
    pub low: Decimal,
    /// 收盘价
    pub close: Decimal,
    /// 成交量
    pub volume: Decimal,
}

/// MinIndicatorService - 事件触发型分钟级指标服务
///
/// 策略协程按需触发计算，非自循环。
/// 计算是同步的，不单独 spawn 协程。
///
/// # 设计原则
/// - 被动触发：策略协程拉数据后主动调用 `compute()`
/// - 不主动轮询，不预计算所有品种
/// - 指标计算直接 await 结果，不异步分发
pub struct MinIndicatorService {
    /// 信号处理器
    processor: Arc<SignalProcessor>,
}

impl MinIndicatorService {
    /// 创建新的 MinIndicatorService
    pub fn new(processor: Arc<SignalProcessor>) -> Self {
        Self { processor }
    }

    /// 事件触发计算：策略协程按需调用
    ///
    /// 计算完成后缓存结果，可通过 `get_latest()` 读取。
    /// 如果品种未注册，返回 `Err`。
    pub fn compute(&self, symbol: &str, kline: KlineInput) -> Result<Indicator1mOutput, String> {
        let symbol_upper = symbol.to_uppercase();
        self.processor.min_update(&symbol_upper, kline.high, kline.low, kline.close, kline.volume)?;
        self.processor
            .min_get_output(&symbol_upper)
            .ok_or_else(|| format!("No output for symbol {}", symbol_upper))
    }

    /// 读取最新计算结果（如果有缓存）
    pub fn get_latest(&self, symbol: &str) -> Option<Indicator1mOutput> {
        self.processor.min_get_output(&symbol.to_uppercase())
    }
}

#[async_trait]
impl IndicatorStore for MinIndicatorService {
    async fn get_min(&self, symbol: &str) -> Option<Indicator1mOutput> {
        self.get_latest(symbol)
    }

    async fn get_day(&self, _symbol: &str) -> Option<Indicator1dOutput> {
        None
    }

    async fn get_all_min(&self) -> HashMap<String, Indicator1mOutput> {
        let symbols = self.processor.registered_symbols();
        let mut result = HashMap::new();
        for symbol in symbols {
            if let Some(output) = self.processor.min_get_output(&symbol) {
                result.insert(symbol, output);
            }
        }
        result
    }

    async fn is_min_ready(&self, symbol: &str) -> bool {
        self.processor.min_is_ready(&symbol.to_uppercase())
    }

    async fn is_day_ready(&self, _symbol: &str) -> bool {
        false
    }
}

// ============================================================================
// DayIndicatorService - 日线级指标串行批量服务
// ============================================================================

/// DayIndicatorService - 串行批量型日线级指标服务
///
/// 每 5 分钟批量计算所有品种的日线指标。
/// 使用 tokio Mutex 确保同一时刻只有一个计算任务（串行）。
///
/// # 设计原则
/// - 低频批量：5分钟一次全量计算
/// - 串行锁：同一时刻只有一个计算任务，防止资源竞争
/// - 日线指标时效性要求低，可接受分钟级更新延迟
pub struct DayIndicatorService {
    /// 信号处理器
    processor: Arc<SignalProcessor>,
    /// 状态中心（用于心跳报到）
    state_center: Arc<dyn StateCenter>,
    /// 日线缓存（计算结果缓存）
    cache: RwLock<HashMap<String, Indicator1dOutput>>,
    /// 最后更新时间索引（用于批量处理时的排序，可选优化）
    last_update: RwLock<BTreeMap<i64, String>>,
    /// 串行锁（确保同一时刻只有一个计算任务）
    compute_lock: tokio::sync::Mutex<()>,
    /// shutdown 信号接收器
    shutdown_rx: tokio::sync::watch::Receiver<()>,
}

impl DayIndicatorService {
    /// 创建新的 DayIndicatorService
    ///
    /// - `processor`: 信号处理器（用于实际计算）
    /// - `state_center`: 状态中心（用于心跳报到）
    /// - `shutdown_rx`: shutdown 广播接收器
    pub fn new(
        processor: Arc<SignalProcessor>,
        state_center: Arc<dyn StateCenter>,
        shutdown_rx: tokio::sync::watch::Receiver<()>,
    ) -> Self {
        Self {
            processor,
            state_center,
            cache: RwLock::new(HashMap::new()),
            last_update: RwLock::new(BTreeMap::new()),
            compute_lock: tokio::sync::Mutex::new(()),
            shutdown_rx,
        }
    }

    /// 自循环：每 5 分钟批量计算一次
    ///
    /// 启动后持续运行，直到收到 shutdown 信号。
    /// 在此 `Arc<Self>` 上调用以获得所有权。
    pub async fn run(self: Arc<Self>) {
        // Clone receiver before loop (Receiver is Clone, but Arc<T> is not DerefMut)
        let mut shutdown_rx = tokio::sync::watch::Receiver::clone(&self.shutdown_rx);
        loop {
            tokio::select! {
                biased;
                _ = shutdown_rx.changed() => {
                    // shutdown 信号到达
                    break;
                }
                _ = tokio::time::sleep(Duration::from_secs(300)) => {
                    // 5 分钟定时器触发
                    self.compute_batch().await;
                    self.report_alive();
                }
            }
        }
    }

    /// 串行批量计算所有 symbol 的日线指标
    async fn compute_batch(&self) {
        // 获取串行锁（等待其他计算任务完成）
        let _lock = self.compute_lock.lock().await;

        // 从 SharedStore 获取所有品种
        // 实际从 processor 读取已注册的品种（由数据层注册）
        let symbols = self.processor.registered_day_symbols();

        let timestamp_ms = chrono::Utc::now().timestamp_millis();

        for symbol in symbols {
            // 尝试计算日线指标（如果数据足够）
            if let Some(indicators) = self.processor.day_get_pine(&symbol) {
                self.cache.write().insert(symbol.clone(), indicators.clone());

                // 更新最后更新时间索引
                let mut last_update = self.last_update.write();
                last_update.insert(timestamp_ms, symbol);
            }
        }
    }

    /// 向 StateCenter 报到（心跳）
    fn report_alive(&self) {
        let _ = self.state_center.report_alive("DayIndicatorService");
    }

    /// 获取单个品种的日线指标（从缓存）
    pub fn get_day_cached(&self, symbol: &str) -> Option<Indicator1dOutput> {
        self.cache.read().get(symbol).cloned()
    }
}

#[async_trait]
impl IndicatorStore for DayIndicatorService {
    async fn get_min(&self, _symbol: &str) -> Option<Indicator1mOutput> {
        None
    }

    async fn get_day(&self, symbol: &str) -> Option<Indicator1dOutput> {
        self.get_day_cached(symbol)
    }

    async fn get_all_min(&self) -> HashMap<String, Indicator1mOutput> {
        HashMap::new()
    }

    async fn is_min_ready(&self, _symbol: &str) -> bool {
        false
    }

    async fn is_day_ready(&self, symbol: &str) -> bool {
        self.cache.read().contains_key(symbol)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_min_indicator_service_compute() {
        let processor = Arc::new(SignalProcessor::new());
        processor.register_symbol("BTCUSDT");

        let service = MinIndicatorService::new(processor.clone());

        let kline = KlineInput {
            high: dec!(50000),
            low: dec!(49000),
            close: dec!(49500),
            volume: dec!(1000),
        };

        let result = service.compute("BTCUSDT", kline);
        assert!(result.is_ok());
        assert!(service.get_latest("BTCUSDT").is_some());
    }

    #[test]
    fn test_min_indicator_service_unregistered_symbol() {
        let processor = Arc::new(SignalProcessor::new());
        let service = MinIndicatorService::new(processor);

        let kline = KlineInput {
            high: dec!(50000),
            low: dec!(49000),
            close: dec!(49500),
            volume: dec!(1000),
        };

        // 未注册的品种应返回 Err
        let result = service.compute("ETHUSDT", kline);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not registered"));
    }

    #[tokio::test]
    async fn test_day_indicator_service_cache() {
        let processor = Arc::new(SignalProcessor::new());
        let state_center = x_data::state::StateCenterImpl::new_arc(60);
        let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(());
        let service = Arc::new(DayIndicatorService::new(processor.clone(), state_center, shutdown_rx));

        // 手动写入日线数据
        let _ = processor.day_update("BTCUSDT", dec!(50000), dec!(49000), dec!(49500));

        // 手动触发计算
        service.compute_batch().await;

        // 检查缓存
        let cached = service.get_day_cached("BTCUSDT");
        assert!(cached.is_some());
    }

    #[tokio::test]
    async fn test_day_indicator_service_indicator_store() {
        let processor = Arc::new(SignalProcessor::new());
        let state_center = x_data::state::StateCenterImpl::new_arc(60);
        let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(());
        let service = Arc::new(DayIndicatorService::new(processor.clone(), state_center, shutdown_rx));

        let _ = processor.day_update("BTCUSDT", dec!(50000), dec!(49000), dec!(49500));
        service.compute_batch().await;

        // 通过 IndicatorStore trait 访问
        let day_output = service.get_day("BTCUSDT").await;
        assert!(day_output.is_some());

        // 分钟级返回 None
        let min_output = service.get_min("BTCUSDT").await;
        assert!(min_output.is_none());
    }

    #[tokio::test]
    async fn test_day_indicator_service_shutdown() {
        let processor = Arc::new(SignalProcessor::new());
        let state_center = x_data::state::StateCenterImpl::new_arc(60);
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(());

        let service = Arc::new(DayIndicatorService::new(processor, state_center, shutdown_rx));

        // 立即发送 shutdown
        let _ = shutdown_tx.send(());

        // run() 应该在收到 shutdown 后立即返回
        service.run().await;
        // 如果 reach here, shutdown works correctly
    }
}
