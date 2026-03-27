//! 多品种 Tick 路由层
//! 
//! 架构：HashMap<String, EngineHandle> + Arc<Tick>
//! - Arc<Tick>: 克隆开销从 ~200ns 降到 ~2ns
//! - 每品种独立 channel: 隔离不同品种的处理

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

use rust_decimal::Decimal;
use chrono::{DateTime, Utc};

// ============================================================================
// Types
// ============================================================================

/// Tick 类型别名（使用 Arc 避免克隆开销）
pub type ArcTick = Arc<Tick>;

/// Tick 结构（简化版，用于测试）
#[derive(Debug, Clone)]
pub struct Tick {
    pub symbol: String,
    pub price: Decimal,
    pub qty: Decimal,
    pub timestamp: DateTime<Utc>,
    pub sequence_id: u64,
    pub kline_1m: Option<KLine>,
    pub kline_15m: Option<KLine>,
    pub kline_1d: Option<KLine>,
}

/// K线结构
#[derive(Debug, Clone)]
pub struct KLine {
    pub symbol: String,
    pub period: String,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
    pub timestamp: DateTime<Utc>,
    pub is_closed: bool,
}

/// 引擎配置
#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub symbol: String,
    pub initial_fund: rust_decimal::Decimal,
    // ... 其他配置
}

/// 引擎句柄（只存 channel，Engine 已 move 到 spawn）
struct EngineHandle {
    /// 该品种的 Tick 专用通道发送端
    tick_tx: mpsc::Sender<ArcTick>,
}

// ============================================================================
// TickRouter
// ============================================================================

/// 多品种路由器 - 管理多个品种的 Engine
/// 
/// 关键设计：
/// - HashMap<String, EngineHandle>: O(1) 查找品种
/// - Arc<Tick>: 零拷贝分发
/// - 每品种独立 channel: 隔离处理
pub struct TickRouter {
    /// 按品种的 Engine 句柄
    engines: HashMap<String, EngineHandle>,
    /// Tick 接收端（从 DataSource 接收广播）
    tick_rx: mpsc::Receiver<ArcTick>,
}

impl TickRouter {
    /// 创建路由 + 启动所有品种引擎
    /// 
    /// 返回 (TickRouter, broadcaster_tx)
    /// - TickRouter: 用于运行主循环
    /// - broadcaster_tx: 给 DataSource 发送 Tick
    pub async fn new(symbols: Vec<String>) -> (Self, mpsc::Sender<ArcTick>) {
        let (broadcaster_tx, tick_rx) = mpsc::channel(1024);
        let mut engines = HashMap::new();
        
        for symbol in symbols {
            let (tx, rx) = mpsc::channel(256);  // 每品种独立 channel
            
            // 启动引擎（独立事件循环）
            // 注意：engine move 到 spawn 任务里，不再需要存到 HashMap
            let symbol_clone = symbol.clone();
            tokio::spawn(async move {
                // 这里会调用 TradingEngine::run(rx)
                // 需要在 sandbox_main.rs 中实现对应的 run 函数
                tracing::info!("[Router] Engine started for {}", symbol_clone);
                while let Some(tick) = rx.recv().await {
                    tracing::trace!(
                        symbol = %tick.symbol,
                        seq = %tick.sequence_id,
                        "[Router] Tick received"
                    );
                }
                tracing::info!("[Router] Engine stopped for {}", symbol_clone);
            });
            
            engines.insert(symbol, EngineHandle { tick_tx: tx });
        }
        
        (Self { engines, tick_rx }, broadcaster_tx)
    }
    
    /// 分发 Tick 到对应品种（O(1) 查找）
    async fn dispatch(&mut self, tick: ArcTick) {
        match self.engines.get(&tick.symbol) {
            Some(handle) => {
                // Arc 克隆极快（只复制引用计数）
                if handle.tick_tx.send(tick).await.is_err() {
                    tracing::warn!(
                        symbol = %tick.symbol,
                        "[Router] Engine channel closed, remove it"
                    );
                    self.engines.remove(&tick.symbol);
                }
            }
            None => {
                tracing::debug!(
                    symbol = %tick.symbol,
                    "[Router] Unknown symbol, skip tick"
                );
            }
        }
    }
    
    /// 主循环：接收广播 Tick，分发到各品种
    pub async fn run(&mut self) {
        tracing::info!("[Router] TickRouter started with {} symbols", self.engines.len());
        
        while let Some(tick) = self.tick_rx.recv().await {
            self.dispatch(tick).await;
        }
        
        tracing::info!("[Router] TickRouter stopped");
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_router_single_symbol() {
        let symbols = vec!["BTCUSDT".to_string()];
        let (mut router, tx) = TickRouter::new(symbols).await;
        
        // 发送一个 Tick
        let tick = Arc::new(Tick {
            symbol: "BTCUSDT".to_string(),
            price: dec!(50000),
            qty: dec!(0.001),
            timestamp: chrono::Utc::now(),
            sequence_id: 1,
            kline_1m: None,
            kline_15m: None,
            kline_1d: None,
        });
        
        tx.send(tick).await.unwrap();
        
        // 短暂等待后 router 会处理
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    #[tokio::test]
    async fn test_router_unknown_symbol() {
        let symbols = vec!["BTCUSDT".to_string()];
        let (mut router, tx) = TickRouter::new(symbols).await;
        
        // 发送 ETHUSDT Tick，但路由只知道 BTCUSDT
        let tick = Arc::new(Tick {
            symbol: "ETHUSDT".to_string(),
            price: dec!(3000),
            qty: dec!(0.01),
            timestamp: chrono::Utc::now(),
            sequence_id: 1,
            kline_1m: None,
            kline_15m: None,
            kline_1d: None,
        });
        
        tx.send(tick).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}
