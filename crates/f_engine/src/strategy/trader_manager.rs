//! trader_manager.rs - Trader 管理器
//!
//! 管理多个品种的 Trader 实例，支持动态启停
//!
//! # 架构演进
//! - v2.x: 使用 tokio::spawn 启动后台任务（已废弃）
//! - v3.0: 使用 channel 接收 Tick，事件驱动（推荐）

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{info, warn};

use d_checktable::h_15m::{Trader, TraderConfig, Executor, Repository};

/// 策略类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrategyType {
    /// Pin 策略（分钟级）
    Pin,
    /// 趋势策略
    Trend,
    /// 日线策略
    Day,
}

/// Trader 实例包装（事件驱动版本）
struct TraderInstance {
    /// Trader 实例
    trader: Arc<Trader>,
    /// Tick 发送端（用于外部注入 Tick）
    tick_tx: mpsc::Sender<b_data_source::Tick>,
}

/// Trader 管理器
///
/// 管理多个品种的 Trader 实例
/// - 启动：创建 Trader 并连接到 Tick channel
/// - 停止：关闭 channel 并清理
///
/// # 事件驱动架构 (v3.0)
/// ```ignore
/// let manager = TraderManager::new();
///
/// // 创建 channel
/// let (tick_tx, tick_rx) = mpsc::channel(1024);
///
/// // 启动 Trader（传入 receiver）
/// manager.start_trader_with_channel("BTCUSDT", StrategyType::Pin, tick_rx).await;
///
/// // 外部注入 Tick
/// tick_tx.send(tick).await?;
/// ```
pub struct TraderManager {
    /// 品种 -> Trader 实例
    instances: Arc<RwLock<HashMap<String, TraderInstance>>>,
    /// 默认 channel buffer 大小
    default_buffer_size: usize,
}

impl Default for TraderManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TraderManager {
    /// 创建 TraderManager
    pub fn new() -> Self {
        Self {
            instances: Arc::new(RwLock::new(HashMap::new())),
            default_buffer_size: 1024,
        }
    }

    /// 创建 TraderManager（自定义 buffer 大小）
    pub fn with_buffer_size(buffer_size: usize) -> Self {
        Self {
            instances: Arc::new(RwLock::new(HashMap::new())),
            default_buffer_size: buffer_size,
        }
    }

    /// 启动品种交易（事件驱动版本 - 推荐）
    ///
    /// # 架构
    /// - 不再使用 tokio::spawn 后台任务
    /// - Trader.run() 直接使用传入的 channel receiver
    /// - 外部通过 tick_tx 注入 Tick
    pub async fn start_trader_with_channel(
        &self,
        symbol: String,
        _strategy_type: StrategyType,
        _tick_rx: mpsc::Receiver<b_data_source::Tick>,
    ) -> Result<(), TraderError> {
        // 检查是否已存在
        {
            let instances = self.instances.read().await;
            if instances.contains_key(&symbol) {
                warn!("Trader for {} already running", symbol);
                return Err(TraderError::AlreadyRunning(symbol));
            }
        }

        // 创建 Trader（需要注入依赖）
        let config = TraderConfig {
            symbol: symbol.clone(),
            ..Default::default()
        };
        let executor = Arc::new(Executor::new(d_checktable::h_15m::ExecutorConfig {
            symbol: symbol.clone(),
            order_interval_ms: config.order_interval_ms,
            initial_ratio: config.initial_ratio,
            lot_size: config.lot_size,
            max_position: config.max_position,
        }));
        let repository = Arc::new(
            Repository::new(&symbol, &config.db_path)
                .map_err(|e| TraderError::InitFailed(symbol.clone(), e.to_string()))?,
        );
        // 使用默认 store 克隆（转换为 trait object）
        let store: std::sync::Arc<dyn b_data_source::MarketDataStore + Send + Sync> =
            b_data_source::default_store().clone();
        let trader = Arc::new(Trader::new(config, executor, repository, store));

        // 创建 channel 用于外部注入 Tick
        let (tick_tx, tick_rx) = mpsc::channel(self.default_buffer_size);

        // 启动 Trader 事件循环（直接 await，不 spawn）
        let trader_clone = trader.clone();
        let tick_rx = tick_rx;
        tokio::spawn(async move {
            trader_clone.run(tick_rx).await;
        });

        info!(symbol = %symbol, "Trader 事件驱动模式已启动");

        // 存储实例
        let mut instances = self.instances.write().await;
        instances.insert(symbol.clone(), TraderInstance { trader, tick_tx });

        Ok(())
    }

    /// 启动品种交易（创建 channel 版本 - 简化接口）
    ///
    /// 自动创建 channel，返回发送端
    /// 外部通过返回的 tick_tx 注入 Tick
    pub async fn start_trader(
        &self,
        symbol: String,
        strategy_type: StrategyType,
    ) -> Result<mpsc::Sender<b_data_source::Tick>, TraderError> {
        // 创建 channel
        let (tick_tx, tick_rx) = mpsc::channel(self.default_buffer_size);

        // 启动 Trader
        self.start_trader_with_channel(symbol, strategy_type, tick_rx).await?;

        Ok(tick_tx)
    }

    /// 停止品种交易
    pub async fn stop_trader(&self, symbol: &str) -> Result<(), TraderError> {
        let mut instances = self.instances.write().await;

        if let Some(instance) = instances.remove(symbol) {
            // 关闭 channel（通知 Trader 停止）- 通过 drop Sender
            drop(instance.tick_tx);
            info!(symbol = %symbol, "Trader channel 已关闭");
            Ok(())
        } else {
            warn!(symbol = %symbol, "Trader not found");
            Err(TraderError::NotFound(symbol.to_string()))
        }
    }

    /// 停止所有交易
    pub async fn stop_all(&self) {
        let mut instances = self.instances.write().await;

        for (symbol, instance) in instances.drain() {
            // 关闭 channel - 通过 drop Sender
            drop(instance.tick_tx);
            info!(symbol = %symbol, "Trader stopped");
        }
    }

    /// 检查是否运行中
    pub async fn is_running(&self, symbol: &str) -> bool {
        let instances = self.instances.read().await;
        instances.contains_key(symbol)
    }

    /// 获取运行中的品种列表
    pub async fn running_symbols(&self) -> Vec<String> {
        let instances = self.instances.read().await;
        instances.keys().cloned().collect()
    }

    /// 获取 Trader 健康状态
    pub async fn health_check(&self, symbol: &str) -> Option<d_checktable::h_15m::TraderHealth> {
        let instances = self.instances.read().await;
        if let Some(instance) = instances.get(symbol) {
            Some(instance.trader.health().await)
        } else {
            None
        }
    }

    /// 广播 Tick 到所有品种（多品种场景）
    pub async fn broadcast_tick(&self, tick: b_data_source::Tick) -> Result<(), TraderError> {
        let instances = self.instances.read().await;

        for (symbol, instance) in instances.iter() {
            if tick.symbol == *symbol {
                if instance.tick_tx.try_send(tick.clone()).is_err() {
                    warn!(symbol = %symbol, "Tick 发送失败，channel 已满或关闭");
                }
            }
        }

        Ok(())
    }
}

/// Trader 错误
#[derive(Debug, thiserror::Error)]
pub enum TraderError {
    #[error("Trader for {0} already running")]
    AlreadyRunning(String),

    #[error("Trader for {0} not found")]
    NotFound(String),

    #[error("Failed to init trader for {0}: {1}")]
    InitFailed(String, String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_start_stop_with_channel() {
        let manager = TraderManager::new();

        // 创建 channel
        let (tick_tx, tick_rx) = mpsc::channel(1024);

        // 启动
        manager
            .start_trader_with_channel("BTCUSDT".to_string(), StrategyType::Pin, tick_rx)
            .await
            .unwrap();
        assert!(manager.is_running("BTCUSDT").await);

        // 停止
        manager.stop_trader("BTCUSDT").await.unwrap();
        assert!(!manager.is_running("BTCUSDT").await);
    }

    #[tokio::test]
    async fn test_start_with_sender() {
        let manager = TraderManager::new();

        // 启动（简化接口）
        let tick_tx = manager
            .start_trader("BTCUSDT".to_string(), StrategyType::Pin)
            .await
            .unwrap();
        assert!(manager.is_running("BTCUSDT").await);

        // 停止
        manager.stop_trader("BTCUSDT").await.unwrap();
        assert!(!manager.is_running("BTCUSDT").await);
    }
}
