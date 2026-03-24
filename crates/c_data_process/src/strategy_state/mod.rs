//! 策略状态管理模块
//!
//! 提供完整的策略状态维护能力，支持 SQLite 持久化
//!
//! # 核心功能
//! - 持仓状态管理（开仓、平仓、方向）
//! - 盈亏统计（累计、每日、最大回撤）
//! - 交易记录（胜率、利润因子）
//! - 风控状态（止损、止盈、熔断）

pub mod db;
pub mod error;
pub mod state;

pub use db::StrategyStateDb;
pub use error::{Result, StrategyStateError};
pub use state::{PositionSide, PositionState, PnlState, TradingStats, RiskState, StrategyParams, TradeRecord, DailyPnl, StrategyState};

use std::sync::Arc;
use parking_lot::RwLock;
use rust_decimal::Decimal;
use fnv::FnvHashMap;

/// 策略状态管理器
pub struct StrategyStateManager {
    db: StrategyStateDb,
    cache: Arc<RwLock<FnvHashMap<String, StrategyState>>>,
}

impl StrategyStateManager {
    pub fn new(db: StrategyStateDb) -> Self {
        Self {
            db,
            cache: Arc::new(RwLock::new(FnvHashMap::default())),
        }
    }

    pub fn with_db_path<P: AsRef<std::path::Path>>(db_path: P) -> Result<Self> {
        let db = StrategyStateDb::new(db_path)?;
        Ok(Self::new(db))
    }

    /// 获取或创建策略状态
    pub fn get_or_create(
        &self,
        instrument_id: &str,
        strategy_id: &str,
        exchange: &str,
        channel: &str,
    ) -> Result<StrategyState> {
        let id = format!("{}:{}", instrument_id, strategy_id);
        
        // 先查缓存
        {
            let cache = self.cache.read();
            if let Some(state) = cache.get(&id) {
                return Ok(state.clone());
            }
        }
        
        // 查数据库
        if let Some(state) = self.db.load(&id)? {
            let mut cache = self.cache.write();
            cache.insert(id, state.clone());
            return Ok(state);
        }
        
        // 创建新的
        let state = StrategyState::new(
            instrument_id.to_string(),
            strategy_id.to_string(),
            exchange.to_string(),
            channel.to_string(),
        );
        
        let mut cache = self.cache.write();
        cache.insert(id, state.clone());
        
        Ok(state)
    }

    /// 获取策略状态（只读）
    pub fn get(&self, instrument_id: &str, strategy_id: &str) -> Result<Option<StrategyState>> {
        let id = format!("{}:{}", instrument_id, strategy_id);
        
        // 先查缓存
        {
            let cache = self.cache.read();
            if let Some(state) = cache.get(&id) {
                return Ok(Some(state.clone()));
            }
        }
        
        // 查数据库
        let state = self.db.load(&id)?;
        Ok(state)
    }

    /// 获取所有策略状态
    pub fn get_all(&self) -> Result<Vec<StrategyState>> {
        self.db.load_all()
    }

    /// 获取指定品种的所有策略
    pub fn get_by_instrument(&self, instrument_id: &str) -> Result<Vec<StrategyState>> {
        self.db.load_by_instrument(instrument_id)
    }

    /// 更新缓存中的状态（内存操作）
    pub fn update_cache(&self, state: StrategyState) -> Result<()> {
        let id = state.id();
        let mut cache = self.cache.write();
        cache.insert(id, state);
        Ok(())
    }

    /// 更新持仓
    pub fn update_position(
        &self,
        instrument_id: &str,
        strategy_id: &str,
        side: PositionSide,
        qty: Decimal,
        price: Decimal,
    ) -> Result<()> {
        let id = format!("{}:{}", instrument_id, strategy_id);
        let mut cache = self.cache.write();
        
        if let Some(state) = cache.get_mut(&id) {
            state.update_position(side, qty, price);
        }
        
        Ok(())
    }

    /// 更新浮动盈亏
    pub fn update_unrealized_pnl(
        &self,
        instrument_id: &str,
        strategy_id: &str,
        current_price: Decimal,
    ) -> Result<()> {
        let id = format!("{}:{}", instrument_id, strategy_id);
        let mut cache = self.cache.write();
        
        if let Some(state) = cache.get_mut(&id) {
            state.update_unrealized_pnl(current_price);
        }
        
        Ok(())
    }

    /// 记录已平仓盈亏
    pub fn record_realized_pnl(
        &self,
        instrument_id: &str,
        strategy_id: &str,
        pnl: Decimal,
    ) -> Result<()> {
        let id = format!("{}:{}", instrument_id, strategy_id);
        let mut cache = self.cache.write();
        
        if let Some(state) = cache.get_mut(&id) {
            state.record_realized_pnl(pnl);
        }
        
        Ok(())
    }

    /// 更新风控参数
    pub fn update_risk(
        &self,
        instrument_id: &str,
        strategy_id: &str,
        stop_loss: Decimal,
        take_profit: Decimal,
    ) -> Result<()> {
        let id = format!("{}:{}", instrument_id, strategy_id);
        let mut cache = self.cache.write();
        
        if let Some(state) = cache.get_mut(&id) {
            state.update_risk_levels(stop_loss, take_profit);
        }
        
        Ok(())
    }

    /// 设置交易开关
    pub fn set_trading(&self, instrument_id: &str, strategy_id: &str, enabled: bool) -> Result<()> {
        let id = format!("{}:{}", instrument_id, strategy_id);
        let mut cache = self.cache.write();
        
        if let Some(state) = cache.get_mut(&id) {
            state.set_trading(enabled);
        }
        
        Ok(())
    }

    /// 增加错误计数
    pub fn increment_error(&self, instrument_id: &str, strategy_id: &str) -> Result<()> {
        let id = format!("{}:{}", instrument_id, strategy_id);
        let mut cache = self.cache.write();
        
        if let Some(state) = cache.get_mut(&id) {
            state.increment_error();
        }
        
        Ok(())
    }

    /// 重置错误计数
    pub fn reset_error(&self, instrument_id: &str, strategy_id: &str) -> Result<()> {
        let id = format!("{}:{}", instrument_id, strategy_id);
        let mut cache = self.cache.write();
        
        if let Some(state) = cache.get_mut(&id) {
            state.reset_error();
        }
        
        Ok(())
    }

    /// 同步到数据库
    pub fn sync_to_db(&self) -> Result<()> {
        let cache = self.cache.read();
        let states: Vec<StrategyState> = cache.values().cloned().collect();
        self.db.save_batch(&states)?;
        Ok(())
    }

    /// 从数据库加载到缓存
    pub fn load_from_db(&self) -> Result<()> {
        let states = self.db.load_all()?;
        let mut cache = self.cache.write();
        
        for state in states {
            cache.insert(state.id(), state);
        }
        
        Ok(())
    }

    /// 删除策略状态
    pub fn delete(&self, instrument_id: &str, strategy_id: &str) -> Result<bool> {
        let id = format!("{}:{}", instrument_id, strategy_id);
        
        // 从缓存删除
        {
            let mut cache = self.cache.write();
            cache.remove(&id);
        }
        
        // 从数据库删除
        self.db.delete(&id)
    }

    /// 获取缓存大小
    pub fn cache_size(&self) -> usize {
        let cache = self.cache.read();
        cache.len()
    }
}

impl Clone for StrategyStateManager {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            cache: Arc::clone(&self.cache),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manager_get_or_create() {
        let db = StrategyStateDb::in_memory().unwrap();
        let manager = StrategyStateManager::new(db);
        
        let state = manager.get_or_create("BTC-USDT", "trend_v1", "binance", "1h").unwrap();
        assert_eq!(state.instrument_id, "BTC-USDT");
        assert_eq!(state.strategy_id, "trend_v1");
    }

    #[test]
    fn test_manager_update_position() {
        let db = StrategyStateDb::in_memory().unwrap();
        let manager = StrategyStateManager::new(db);
        
        manager.get_or_create("BTC-USDT", "trend_v1", "binance", "1h").unwrap();
        manager.update_position("BTC-USDT", "trend_v1", PositionSide::Long, dec!(0.1), dec!(50000)).unwrap();
        
        let state = manager.get("BTC-USDT", "trend_v1").unwrap().unwrap();
        assert_eq!(state.position.current, dec!(0.1));
        assert_eq!(state.position.side, PositionSide::Long);
    }

    #[test]
    fn test_manager_sync() {
        let db = StrategyStateDb::in_memory().unwrap();
        let manager = StrategyStateManager::new(db);
        
        manager.get_or_create("BTC-USDT", "trend_v1", "binance", "1h").unwrap();
        manager.update_position("BTC-USDT", "trend_v1", PositionSide::Long, dec!(0.1), dec!(50000)).unwrap();
        manager.sync_to_db().unwrap();
        
        // 新建管理器从数据库加载
        let db2 = StrategyStateDb::in_memory().unwrap();
        db2.save_batch(&manager.get_all().unwrap()).unwrap();
        
        let manager2 = StrategyStateManager::new(db2);
        manager2.load_from_db().unwrap();
        
        let state = manager2.get("BTC-USDT", "trend_v1").unwrap().unwrap();
        assert_eq!(state.position.current, dec!(0.1));
    }
}
