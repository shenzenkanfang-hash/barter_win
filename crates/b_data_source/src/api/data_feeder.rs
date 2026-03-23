//! 统一数据接口 - DataFeeder
//!
//! 提供所有市场数据查询接口，其他模块必须通过这里获取数据
//! 不能直接访问内部内存或 WS

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

/// 统一数据提供者 - 所有数据查询的单一入口
pub struct DataFeeder {
    /// K线合成器（1m）
    kline_1m: Arc<RwLock<Option<crate::ws::kline_1m::Kline1mStream>>>,
    /// 订单簿
    depth_stream: Arc<RwLock<Option<DepthStream>>>,
    /// 波动率管理器
    volatility_manager: Arc<VolatilityManager>,
    /// 账户同步器
    account_syncer: FuturesDataSyncer,
    /// 缓存的最新Tick（按symbol索引）
    latest_ticks: RwLock<HashMap<String, Tick>>,
}

impl DataFeeder {
    pub fn new() -> Self {
        Self {
            kline_1m: Arc::new(RwLock::new(None)),
            depth_stream: Arc::new(RwLock::new(None)),
            volatility_manager: Arc::new(VolatilityManager::new()),
            account_syncer: FuturesDataSyncer::new(),
            latest_ticks: RwLock::new(HashMap::new()),
        }
    }

    // ==================== WS 数据查询接口 ====================

    /// 获取 1m K线数据（最新收盘的1根）
    pub fn ws_get_1m(&self, symbol: &str) -> Option<KLine> {
        let ticks = self.latest_ticks.read();
        ticks.get(&symbol.to_uppercase()).and_then(|t| t.kline_1m.clone())
    }

    /// 获取 15m K线数据
    pub fn ws_get_15m(&self, symbol: &str) -> Option<KLine> {
        let ticks = self.latest_ticks.read();
        ticks.get(&symbol.to_uppercase()).and_then(|t| t.kline_15m.clone())
    }

    /// 获取 1d K线数据
    pub fn ws_get_1d(&self, symbol: &str) -> Option<KLine> {
        let ticks = self.latest_ticks.read();
        ticks.get(&symbol.to_uppercase()).and_then(|t| t.kline_1d.clone())
    }

    /// 获取订单簿深度
    pub fn ws_get_depth_book(&self, symbol: &str) -> Option<OrderBook> {
        let depth = self.depth_stream.read();
        depth.as_ref()?.get_latest_orderbook(symbol)
    }

    /// 获取波动率统计（单个品种）
    pub fn ws_get_volatility(&self, symbol: &str) -> Option<VolatilityEntry> {
        let rank = self.volatility_manager.rank_by_1m();
        rank.into_iter().find(|e| e.symbol.eq_ignore_ascii_case(symbol))
    }

    /// 获取 1m 波动率排行
    pub fn ws_rank_by_1m(&self) -> Vec<VolatilityEntry> {
        self.volatility_manager.rank_by_1m()
    }

    /// 获取 15m 波动率排行
    pub fn ws_rank_by_15m(&self) -> Vec<VolatilityEntry> {
        self.volatility_manager.rank_by_15m()
    }

    /// 获取 1m 高波动品种
    pub fn ws_high_vol_1m(&self) -> Vec<VolatilityEntry> {
        self.volatility_manager.high_vol_1m()
    }

    /// 获取 15m 高波动品种
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
    pub(crate) fn update_tick(&self, tick: Tick) {
        let symbol = tick.symbol.clone();
        let mut ticks = self.latest_ticks.write();
        ticks.insert(symbol, tick);
    }

    /// 获取波动率管理器（内部使用）
    pub(crate) fn get_volatility_manager(&self) -> Arc<VolatilityManager> {
        self.volatility_manager.clone()
    }
}

impl Default for DataFeeder {
    fn default() -> Self {
        Self::new()
    }
}
