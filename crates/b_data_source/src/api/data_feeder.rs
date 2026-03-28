//! 统一数据接口 - DataFeeder
//!
//! 提供所有市场数据查询接口，其他模块必须通过这里获取数据
//! 不能直接访问内部内存或 WS
//!
//! ## 事件驱动架构
//! 
//! 推荐使用 channel 订阅模式，而非轮询模式：
//! - `subscribe_1m(symbol, tx)`: 订阅 1m Tick 流
//! - `subscribe_15m(symbol, tx)`: 订阅 15m Tick 流
//!
//! 旧接口 `ws_get_1m()` 已标记为 deprecated，将在未来版本移除。

use crate::models::{KLine, Tick};
use crate::ws::volatility::VolatilityManager;
use crate::ws::order_books::{DepthStream, OrderBook};
use crate::api::FuturesDataSyncer;
use crate::api::position::FuturesPositionData;
use crate::api::account::FuturesAccountData;
use a_common::volatility::VolatilityEntry;
use a_common::MarketError;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

/// 订阅者条目
struct Subscriber {
    tx: mpsc::Sender<Tick>,
    symbol: String,
}

/// 统一数据提供者 - 所有数据查询的单一入口
pub struct DataFeeder {
    /// K线合成器（1m）
    #[allow(dead_code)]
    kline_1m: Arc<RwLock<Option<crate::ws::kline_1m::Kline1mStream>>>,
    /// 订单簿
    depth_stream: Arc<RwLock<Option<DepthStream>>>,
    /// 波动率管理器
    volatility_manager: Arc<VolatilityManager>,
    /// 账户同步器
    account_syncer: FuturesDataSyncer,
    /// 缓存的最新Tick（按symbol索引）
    latest_ticks: RwLock<HashMap<String, Tick>>,
    /// 1m 订阅者列表
    subscribers_1m: RwLock<Vec<Subscriber>>,
    /// 15m 订阅者列表
    subscribers_15m: RwLock<Vec<Subscriber>>,
}

impl DataFeeder {
    pub fn new() -> Self {
        Self {
            kline_1m: Arc::new(RwLock::new(None)),
            depth_stream: Arc::new(RwLock::new(None)),
            volatility_manager: Arc::new(VolatilityManager::new()),
            account_syncer: FuturesDataSyncer::new(),
            latest_ticks: RwLock::new(HashMap::new()),
            subscribers_1m: RwLock::new(Vec::new()),
            subscribers_15m: RwLock::new(Vec::new()),
        }
    }

    // ==================== Channel 订阅接口（推荐使用） ====================

    /// 订阅 1m Tick 数据流
    ///
    /// # 示例
    /// ```ignore
    /// let (tx, rx) = mpsc::channel(1024);
    /// feeder.subscribe_1m("BTCUSDT", tx);
    /// // 在另一个任务中: while let Some(tick) = rx.recv().await { ... }
    /// ```
    pub fn subscribe_1m(&self, symbol: &str, tx: mpsc::Sender<Tick>) {
        let subscriber = Subscriber {
            tx,
            symbol: symbol.to_uppercase(),
        };
        self.subscribers_1m.write().push(subscriber);
        tracing::debug!("订阅 1m Tick: {}", symbol);
    }

    /// 订阅 15m Tick 数据流
    pub fn subscribe_15m(&self, symbol: &str, tx: mpsc::Sender<Tick>) {
        let subscriber = Subscriber {
            tx,
            symbol: symbol.to_uppercase(),
        };
        self.subscribers_15m.write().push(subscriber);
        tracing::debug!("订阅 15m Tick: {}", symbol);
    }

    /// 退订 1m Tick
    pub fn unsubscribe_1m(&self, symbol: &str) {
        let symbol = symbol.to_uppercase();
        self.subscribers_1m.write().retain(|s| s.symbol != symbol);
    }

    /// 退订 15m Tick
    pub fn unsubscribe_15m(&self, symbol: &str) {
        let symbol = symbol.to_uppercase();
        self.subscribers_15m.write().retain(|s| s.symbol != symbol);
    }

    /// 广播 Tick 给所有订阅者（内部使用）
    ///
    /// 使用 try_send 非阻塞发送，避免慢订阅者阻塞整个数据层
    fn broadcast_to_subscribers(&self, tick: &Tick, subscribers: &[Subscriber]) {
        let symbol_upper = tick.symbol.to_uppercase();
        
        // 收集需要移除的订阅者（channel 已关闭）
        let mut to_remove = Vec::new();
        
        for subscriber in subscribers.iter() {
            if subscriber.symbol == symbol_upper {
                // 尝试发送，非阻塞
                match subscriber.tx.try_send(tick.clone()) {
                    Ok(()) => {
                        // 发送成功
                    }
                    Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                        // 慢订阅者告警：channel 满了，丢旧 Tick
                        tracing::warn!(
                            symbol = %symbol_upper,
                            tick_ts = %tick.timestamp,
                            "[Broadcast] 慢订阅者 channel 已满，丢弃 Tick"
                        );
                    }
                    Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                        // 订阅者已关闭，标记移除
                        tracing::debug!(
                            symbol = %symbol_upper,
                            "[Broadcast] 订阅者已关闭，清理"
                        );
                        to_remove.push(subscriber.symbol.clone());
                    }
                }
            }
        }
        
        // 移除已关闭的订阅者
        if !to_remove.is_empty() {
            let mut subs = self.subscribers_1m.write();
            subs.retain(|s| !to_remove.contains(&s.symbol));
        }
    }

    // ==================== WS 数据查询接口（已废弃 - 使用 channel 替代） ====================
    // 
    // ⚠️ 警告：这些接口是轮询模式，违反事件驱动原则
    // ⚠️ 正确做法：通过 channel 接收 Tick，不主动拉取
    // 
    // 事件驱动原则：
    // - 引擎从 channel 接收 Tick（tick_rx.recv().await）
    // - 不主动调用 ws_get_1m() 拉取数据
    // - 背压机制由 channel send().await 自动处理

    /// 获取 1m K线数据（最新收盘的1根）- 已废弃
    #[deprecated(since = "1.0.0", note = "使用 channel 接收 Tick 替代")]
    pub fn ws_get_1m(&self, symbol: &str) -> Option<KLine> {
        let ticks = self.latest_ticks.read();
        ticks.get(&symbol.to_uppercase()).and_then(|t| t.kline_1m.clone())
    }

    /// 获取 15m K线数据 - 已废弃
    #[deprecated(since = "1.0.0", note = "使用 channel 接收 Tick 替代")]
    pub fn ws_get_15m(&self, symbol: &str) -> Option<KLine> {
        let ticks = self.latest_ticks.read();
        ticks.get(&symbol.to_uppercase()).and_then(|t| t.kline_15m.clone())
    }

    /// 获取 1d K线数据 - 已废弃
    #[deprecated(since = "1.0.0", note = "使用 channel 接收 Tick 替代")]
    pub fn ws_get_1d(&self, symbol: &str) -> Option<KLine> {
        let ticks = self.latest_ticks.read();
        ticks.get(&symbol.to_uppercase()).and_then(|t| t.kline_1d.clone())
    }

    /// 获取订单簿深度 - 已废弃
    #[deprecated(since = "1.0.0", note = "使用 channel 接收 Tick 替代")]
    pub fn ws_get_depth_book(&self, symbol: &str) -> Option<OrderBook> {
        let depth = self.depth_stream.read();
        depth.as_ref()?.get_latest_orderbook(symbol)
    }

    /// 获取波动率统计（单个品种）- 已废弃
    #[deprecated(since = "1.0.0", note = "使用 channel 接收 Tick 替代")]
    pub fn ws_get_volatility(&self, symbol: &str) -> Option<VolatilityEntry> {
        let rank = self.volatility_manager.rank_by_1m();
        rank.into_iter().find(|e| e.symbol.eq_ignore_ascii_case(symbol))
    }

    /// 获取 1m 波动率排行 - 已废弃
    #[deprecated(since = "1.0.0", note = "使用 channel 接收 Tick 替代")]
    pub fn ws_rank_by_1m(&self) -> Vec<VolatilityEntry> {
        self.volatility_manager.rank_by_1m()
    }

    /// 获取 15m 波动率排行 - 已废弃
    #[deprecated(since = "1.0.0", note = "使用 channel 接收 Tick 替代")]
    pub fn ws_rank_by_15m(&self) -> Vec<VolatilityEntry> {
        self.volatility_manager.rank_by_15m()
    }

    /// 获取 1m 高波动品种 - 已废弃
    #[deprecated(since = "1.0.0", note = "使用 channel 接收 Tick 替代")]
    pub fn ws_high_vol_1m(&self) -> Vec<VolatilityEntry> {
        self.volatility_manager.high_vol_1m()
    }

    /// 获取 15m 高波动品种 - 已废弃
    #[deprecated(since = "1.0.0", note = "使用 channel 接收 Tick 替代")]
    pub fn ws_high_vol_15m(&self) -> Vec<VolatilityEntry> {
        self.volatility_manager.high_vol_15m()
    }

    // ==================== API 数据查询接口 ====================

    /// 获取账户信息
    pub async fn api_get_account(&self) -> Result<FuturesAccountData, MarketError> {
        self.account_syncer.fetch_account().await
    }

    /// 获取持仓信息
    pub async fn api_get_positions(&self) -> Result<Vec<FuturesPositionData>, MarketError> {
        self.account_syncer.fetch_positions().await
    }

    // ==================== 内部方法（供 WS 回调使用） ====================

    /// 更新 Tick（由 K线合成器调用）
    /// 
    /// 注意：此方法只更新缓存，不会广播给订阅者
    /// 如需广播，请使用 push_tick()
    #[allow(dead_code)]
    pub(crate) fn update_tick(&self, tick: Tick) {
        let symbol = tick.symbol.clone();
        let mut ticks = self.latest_ticks.write();
        ticks.insert(symbol, tick);
    }

    /// 推送 Tick（公开接口，用于模拟数据注入）
    /// 
    /// 会自动广播给所有订阅者
    pub fn push_tick(&self, tick: Tick) {
        let symbol = tick.symbol.clone();
        
        // 1. 更新内部缓存
        {
            let mut ticks = self.latest_ticks.write();
            ticks.insert(symbol.clone(), tick.clone());
        }
        
        // 2. 广播给 1m 订阅者
        {
            let subs = self.subscribers_1m.read();
            self.broadcast_to_subscribers(&tick, &subs);
        }
        
        // 3. 如果有 15m K 线，也广播给 15m 订阅者
        if tick.kline_15m.is_some() {
            let subs = self.subscribers_15m.read();
            self.broadcast_to_subscribers(&tick, &subs);
        }
        
        tracing::trace!("推送 Tick: {}", symbol);
    }

    /// 获取波动率管理器（内部使用）
    #[allow(dead_code)]
    pub(crate) fn get_volatility_manager(&self) -> Arc<VolatilityManager> {
        self.volatility_manager.clone()
    }
}

impl Default for DataFeeder {
    fn default() -> Self {
        Self::new()
    }
}
