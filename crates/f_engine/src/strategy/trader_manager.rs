//! trader_manager.rs - Trader 管理器
//!
//! 管理多个品种的 Trader 实例，支持动态启停

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
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

/// Trader 实例包装
struct TraderInstance {
    trader: Arc<Trader>,
}

/// Trader 管理器
///
/// 管理多个品种的 Trader 实例
/// - 启动：创建 Trader 并启动自循环协程
/// - 停止：取消协程并清理
pub struct TraderManager {
    /// 品种 -> Trader 实例
    instances: Arc<RwLock<HashMap<String, TraderInstance>>>,
}

impl Default for TraderManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TraderManager {
    pub fn new() -> Self {
        Self {
            instances: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 启动品种交易
    pub async fn start_trader(&self, symbol: String, _strategy_type: StrategyType) -> Result<(), TraderError> {
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
        let trader = Arc::new(Trader::new(config, executor, repository));

        // 启动异步循环（后台任务）
        let trader_clone = trader.clone();
        tokio::spawn(async move {
            trader_clone.start().await;
        });

        info!("Started trader for {}", symbol);

        // 存储实例
        let mut instances = self.instances.write().await;
        instances.insert(symbol.clone(), TraderInstance { trader });

        Ok(())
    }

    /// 停止品种交易
    pub async fn stop_trader(&self, symbol: &str) -> Result<(), TraderError> {
        let mut instances = self.instances.write().await;
        
        if let Some(instance) = instances.remove(symbol) {
            instance.trader.stop();
            info!("Stopped trader for {}", symbol);
            Ok(())
        } else {
            warn!("Trader for {} not found", symbol);
            Err(TraderError::NotFound(symbol.to_string()))
        }
    }

    /// 停止所有交易
    pub async fn stop_all(&self) {
        let mut instances = self.instances.write().await;

        for (symbol, instance) in instances.drain() {
            instance.trader.stop();
            info!("Stopped trader for {}", symbol);
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
    async fn test_start_stop() {
        let manager = TraderManager::new();
        
        // 启动
        manager.start_trader("BTCUSDT".to_string(), StrategyType::Pin).await.unwrap();
        assert!(manager.is_running("BTCUSDT").await);
        
        // 重复启动应失败
        let result = manager.start_trader("BTCUSDT".to_string(), StrategyType::Pin).await;
        assert!(result.is_err());
        
        // 停止
        manager.stop_trader("BTCUSDT").await.unwrap();
        assert!(!manager.is_running("BTCUSDT").await);
    }
}
