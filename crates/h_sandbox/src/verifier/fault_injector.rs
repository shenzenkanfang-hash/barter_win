//! Fault Injector - 故障注入框架
//!
//! 支持的故障类型：网络延迟、数据丢失、数据乱序、数据损坏、交易所故障、部分成交、价格跳空

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use parking_lot::RwLock;
use chrono::{DateTime, Utc, Duration};
use rand::Rng;

/// 故障类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FaultType {
    NetworkDelay,      // 网络延迟
    DataDrop,           // 数据丢失
    DataCorruption,     // 数据损坏
    ExchangeError,     // 交易所故障 (503/504)
    PartialFill,       // 部分成交
    PriceGap,          // 价格跳空
    OutOfOrder,        // 数据乱序
}

/// 故障配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaultConfig {
    /// 是否启用故障注入
    pub enabled: bool,
    /// 故障类型
    pub fault_type: FaultType,
    /// 故障参数（根据类型不同含义不同）
    /// - NetworkDelay: 延迟毫秒数
    /// - DataDrop: 丢弃概率 (0-1)
    /// - DataCorruption: 损坏概率 (0-1)
    /// - ExchangeError: 错误概率 (0-1)
    /// - PartialFill: 成交率 (0-1)
    /// - PriceGap: 跳空百分比 (0-1)
    /// - OutOfOrder: 乱序概率 (0-1)
    pub param: f64,
    /// 故障触发间隔（秒）
    pub interval_secs: u64,
    /// 持续时间（秒），0 表示持续生效
    pub duration_secs: u64,
    /// 随机性：是否添加随机波动
    pub random: bool,
}

impl Default for FaultConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            fault_type: FaultType::NetworkDelay,
            param: 100.0,  // 100ms 延迟
            interval_secs: 60,
            duration_secs: 0,
            random: true,
        }
    }
}

impl FaultConfig {
    /// 网络延迟配置
    pub fn network_delay(param: f64) -> Self {
        Self {
            enabled: true,
            fault_type: FaultType::NetworkDelay,
            param,
            interval_secs: 10,
            duration_secs: 0,
            random: true,
        }
    }

    /// 数据丢失配置
    pub fn data_drop(probability: f64) -> Self {
        Self {
            enabled: true,
            fault_type: FaultType::DataDrop,
            param: probability,
            interval_secs: 30,
            duration_secs: 0,
            random: true,
        }
    }

    /// 部分成交配置
    pub fn partial_fill(fill_rate: f64) -> Self {
        Self {
            enabled: true,
            fault_type: FaultType::PartialFill,
            param: fill_rate,
            interval_secs: 0,  // 每个订单
            duration_secs: 0,
            random: false,
        }
    }

    /// 价格跳空配置
    pub fn price_gap(gap_percent: f64) -> Self {
        Self {
            enabled: true,
            fault_type: FaultType::PriceGap,
            param: gap_percent,
            interval_secs: 300,
            duration_secs: 10,
            random: true,
        }
    }
}

/// 故障事件记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaultEvent {
    pub timestamp: DateTime<Utc>,
    pub fault_type: FaultType,
    pub param: f64,
    pub affected_data: String,
    pub result: FaultResult,
}

/// 故障处理结果
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FaultResult {
    Applied,       // 故障已应用
    Skipped,        // 跳过（未触发）
    Error,          // 故障应用出错
}

/// 故障注入器
pub struct FaultInjector {
    /// 故障配置
    config: RwLock<FaultConfig>,
    /// 故障计数器
    fault_count: AtomicU64,
    /// 故障事件历史
    events: RwLock<Vec<FaultEvent>>,
    /// 上次故障时间
    last_fault_time: AtomicU64,
    /// 故障持续结束时间
    fault_end_time: AtomicU64,
    /// 随机数生成器（需要 Mutex 保护）
    rng: RwLock<rand::rngs::StdRng>,
}

impl FaultInjector {
    /// 创建故障注入器
    pub fn new(config: FaultConfig) -> Self {
        Self {
            config: RwLock::new(config),
            fault_count: AtomicU64::new(0),
            events: RwLock::new(Vec::new()),
            last_fault_time: AtomicU64::new(0),
            fault_end_time: AtomicU64::new(0),
            rng: RwLock::new(rand::rngs::StdRng::from_entropy()),
        }
    }

    /// 更新配置
    pub fn update_config(&self, config: FaultConfig) {
        let mut cfg = self.config.write();
        *cfg = config;
    }

    /// 检查是否应该触发故障
    pub fn should_inject(&self) -> bool {
        let config = self.config.read();
        if !config.enabled {
            return false;
        }

        let now = Utc::now().timestamp() as u64;

        // 检查持续时间
        let end_time = self.fault_end_time.load(Ordering::Relaxed);
        if end_time > 0 && now > end_time {
            // 故障持续时间已过
            return false;
        }

        // 检查间隔
        let last_time = self.last_fault_time.load(Ordering::Relaxed);
        if now - last_time < config.interval_secs {
            return false;
        }

        // 随机检查
        let mut rng = self.rng.write();
        let random_value: f64 = rng.gen();
        let threshold = config.param / 100.0;  // param 作为概率

        random_value < threshold
    }

    /// 注入网络延迟
    pub fn inject_delay(&self) -> std::time::Duration {
        let config = self.config.read();
        let mut rng = self.rng.write();

        let delay_ms = if config.random {
            let base = config.param as u64;
            let jitter = rng.gen_range(0..base/2);
            base + jitter
        } else {
            config.param as u64
        };

        std::time::Duration::from_millis(delay_ms)
    }

    /// 注入数据丢失（返回是否丢弃）
    pub fn should_drop_data(&self) -> bool {
        let config = self.config.read();
        if config.fault_type != FaultType::DataDrop {
            return false;
        }

        let mut rng = self.rng.write();
        rng.gen::<f64>() < config.param
    }

    /// 注入数据损坏（返回是否损坏）
    pub fn should_corrupt_data(&self) -> bool {
        let config = self.config.read();
        if config.fault_type != FaultType::DataCorruption {
            return false;
        }

        let mut rng = self.rng.write();
        rng.gen::<f64>() < config.param
    }

    /// 损坏数据
    pub fn corrupt_price(&self, price: Decimal) -> Option<Decimal> {
        if self.should_corrupt_data() {
            let config = self.config.read();
            let mut rng = self.rng.write();

            // 随机损坏：设置为 0、负数、或极大值
            let corrupt_type = rng.gen_range(0..3);
            match corrupt_type {
                0 => Some(Decimal::ZERO),  // 价格为 0
                1 => Some(dec!(-1)),       // 负价格
                _ => Some(dec!(1e18)),     // 极大值
            }
        } else {
            None
        }
    }

    /// 注入部分成交（返回实际成交数量）
    pub fn calculate_actual_qty(&self, requested_qty: Decimal) -> Decimal {
        let config = self.config.read();
        if config.fault_type != FaultType::PartialFill {
            return requested_qty;
        }

        let fill_rate = config.param;  // 成交率
        let mut rng = self.rng.write();

        // 根据成交率计算实际成交数量
        let actual_rate = if config.random {
            rng.gen_range(fill_rate * 0.5..fill_rate * 1.5).min(1.0).max(0.0)
        } else {
            fill_rate
        };

        (requested_qty * Decimal::from_f64_retain(actual_rate).unwrap_or(dec!(1)))
            .round_dp(4)
    }

    /// 注入价格跳空（返回跳空后的价格）
    pub fn apply_price_gap(&self, current_price: Decimal) -> Option<Decimal> {
        if !self.should_inject() {
            return None;
        }

        let config = self.config.read();
        if config.fault_type != FaultType::PriceGap {
            return None;
        }

        let gap_percent = config.param / 100.0;
        let mut rng = self.rng.write();

        // 随机方向
        let direction = if rng.gen::<bool>() { 1.0 } else { -1.0 };
        let gap = current_price * Decimal::from_f64_retain(gap_percent * direction).unwrap_or(dec!(0));

        // 设置故障持续时间
        if config.duration_secs > 0 {
            let end_time = Utc::now().timestamp() as u64 + config.duration_secs;
            self.fault_end_time.store(end_time, Ordering::Relaxed);
        }

        self.last_fault_time.store(Utc::now().timestamp() as u64, Ordering::Relaxed);
        self.fault_count.fetch_add(1, Ordering::Relaxed);

        Some(current_price + gap)
    }

    /// 注入交易所错误（返回是否模拟错误）
    pub fn should_simulate_error(&self) -> bool {
        let config = self.config.read();
        if config.fault_type != FaultType::ExchangeError {
            return false;
        }

        let mut rng = self.rng.write();
        rng.gen::<f64>() < config.param
    }

    /// 记录故障事件
    pub fn record_event(&self, fault_type: FaultType, affected_data: &str, result: FaultResult) {
        let event = FaultEvent {
            timestamp: Utc::now(),
            fault_type,
            param: self.config.read().param,
            affected_data: affected_data.to_string(),
            result,
        };

        self.events.write().push(event);
    }

    /// 获取故障统计
    pub fn get_statistics(&self) -> FaultStatistics {
        let events = self.events.read();
        let total = events.len();
        let applied = events.iter().filter(|e| e.result == FaultResult::Applied).count();
        let skipped = events.iter().filter(|e| e.result == FaultResult::Skipped).count();
        let errors = events.iter().filter(|e| e.result == FaultResult::Error).count();

        FaultStatistics {
            total_events: total,
            applied_count: applied,
            skipped_count: skipped,
            error_count: errors,
            fault_count: self.fault_count.load(Ordering::Relaxed),
        }
    }

    /// 获取故障历史
    pub fn get_history(&self) -> Vec<FaultEvent> {
        self.events.read().clone()
    }

    /// 重置
    pub fn reset(&self) {
        self.fault_count.store(0, Ordering::Relaxed);
        self.last_fault_time.store(0, Ordering::Relaxed);
        self.fault_end_time.store(0, Ordering::Relaxed);
        self.events.write().clear();
    }
}

/// 故障统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaultStatistics {
    pub total_events: usize,
    pub applied_count: usize,
    pub skipped_count: usize,
    pub error_count: usize,
    pub fault_count: u64,
}

/// 故障场景预设
pub struct FaultScenarios;

impl FaultScenarios {
    /// 高延迟场景
    pub fn high_latency() -> FaultConfig {
        FaultConfig::network_delay(1000.0)  // 1秒延迟
    }

    /// 随机丢包场景
    pub fn random_packet_loss() -> FaultConfig {
        FaultConfig::data_drop(0.05)  // 5% 丢包
    }

    /// 部分成交场景
    pub fn partial_fill_scenario() -> FaultConfig {
        FaultConfig::partial_fill(0.5)  // 50% 成交
    }

    /// 价格跳空场景
    pub fn price_gap_scenario() -> FaultConfig {
        FaultConfig::price_gap(10.0)  // 10% 跳空
    }

    /// 组合故障：延迟 + 丢包
    pub fn combined_latency_and_loss() -> Vec<FaultConfig> {
        vec![
            FaultConfig::network_delay(500.0),
            FaultConfig::data_drop(0.03),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fault_injection() {
        let config = FaultConfig::network_delay(100.0);
        let injector = FaultInjector::new(config);

        // 启用故障
        injector.update_config(FaultConfig {
            enabled: true,
            fault_type: FaultType::NetworkDelay,
            param: 100.0,
            interval_secs: 0,
            duration_secs: 0,
            random: false,
        });

        let delay = injector.inject_delay();
        assert!(delay.as_millis() >= 100);
    }

    #[test]
    fn test_partial_fill() {
        let config = FaultConfig::partial_fill(0.5);
        let injector = FaultInjector::new(config);

        let actual = injector.calculate_actual_qty(dec!(1.0));
        // 应该大约是 0.5，可能有一些随机波动
        assert!(actual > Decimal::ZERO);
        assert!(actual <= dec!(1.0));
    }

    #[test]
    fn test_statistics() {
        let config = FaultConfig::default();
        let injector = FaultInjector::new(config);

        let stats = injector.get_statistics();
        assert_eq!(stats.total_events, 0);
    }
}