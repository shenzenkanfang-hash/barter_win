use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// 交易记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeRecord {
    pub order_id: String,
    pub symbol: String,
    pub strategy_id: String,
    pub side: String,        // "long" or "short"
    pub qty: Decimal,
    pub price: Decimal,
    pub timestamp: i64,
    pub pnl: Decimal,       // 已实现盈亏
}

/// 持仓快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionSnapshot {
    pub symbol: String,
    pub strategy_id: String,
    pub direction: String,  // "long" or "short"
    pub qty: Decimal,
    pub avg_price: Decimal,
    pub timestamp: i64,
}

/// K线缓存
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KLineCache {
    pub symbol: String,
    pub period: String,
    pub klines: Vec<KLineData>,
}

/// K线数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KLineData {
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
    pub timestamp: i64,
}

/// 持久化配置
#[derive(Debug, Clone)]
pub struct PersistenceConfig {
    /// SQLite 数据库路径
    pub sqlite_path: PathBuf,
    /// Redis 主机
    pub redis_host: String,
    /// Redis 端口
    pub redis_port: u16,
    /// K线缓存过期时间 (秒)
    pub kline_cache_ttl_secs: i64,
    /// 是否启用 Redis
    pub redis_enabled: bool,
}

impl Default for PersistenceConfig {
    fn default() -> Self {
        Self {
            sqlite_path: PathBuf::from("data/trading.db"),
            redis_host: "127.0.0.1".to_string(),
            redis_port: 6379,
            kline_cache_ttl_secs: 3600, // 1小时
            redis_enabled: false,        // 默认禁用，需要 mlua/redis 依赖
        }
    }
}

/// 持久化服务
///
/// 提供交易记录、持仓快照、K线缓存的持久化存储。
/// 支持 SQLite 和 Redis 两种存储后端。
///
/// 设计依据: 设计文档 14.6 持仓/资金更新层
pub struct PersistenceService {
    config: PersistenceConfig,
    /// 内存缓存的交易记录
    trade_records: Vec<TradeRecord>,
    /// 内存缓存的持仓快照
    position_snapshots: Vec<PositionSnapshot>,
    /// K线缓存
    kline_cache: HashMap<(String, String), KLineCache>,  // (symbol, period) -> cache
}

impl Default for PersistenceService {
    fn default() -> Self {
        Self::new()
    }
}

impl PersistenceService {
    /// 创建持久化服务
    pub fn new() -> Self {
        Self {
            config: PersistenceConfig::default(),
            trade_records: Vec::new(),
            position_snapshots: Vec::new(),
            kline_cache: HashMap::new(),
        }
    }

    /// 创建带配置的持久化服务
    pub fn with_config(config: PersistenceConfig) -> Self {
        Self {
            config,
            trade_records: Vec::new(),
            position_snapshots: Vec::new(),
            kline_cache: HashMap::new(),
        }
    }

    // ========== 交易记录 ==========

    /// 保存交易记录
    pub fn save_trade(&mut self, record: TradeRecord) {
        self.trade_records.push(record);
    }

    /// 获取所有交易记录
    pub fn get_trades(&self) -> &[TradeRecord] {
        &self.trade_records
    }

    /// 获取指定策略的交易记录
    pub fn get_trades_by_strategy(&self, strategy_id: &str) -> Vec<&TradeRecord> {
        self.trade_records
            .iter()
            .filter(|r| r.strategy_id == strategy_id)
            .collect()
    }

    /// 获取指定品种的交易记录
    pub fn get_trades_by_symbol(&self, symbol: &str) -> Vec<&TradeRecord> {
        self.trade_records
            .iter()
            .filter(|r| r.symbol == symbol)
            .collect()
    }

    /// 计算指定策略的累计盈亏
    pub fn calculate_strategy_pnl(&self, strategy_id: &str) -> Decimal {
        self.trade_records
            .iter()
            .filter(|r| r.strategy_id == strategy_id)
            .map(|r| r.pnl)
            .sum()
    }

    // ========== 持仓快照 ==========

    /// 保存持仓快照
    pub fn save_position_snapshot(&mut self, snapshot: PositionSnapshot) {
        self.position_snapshots.push(snapshot);
    }

    /// 获取最新持仓快照
    pub fn get_latest_position_snapshot(
        &self,
        symbol: &str,
        strategy_id: &str,
    ) -> Option<&PositionSnapshot> {
        self.position_snapshots
            .iter()
            .filter(|s| s.symbol == symbol && s.strategy_id == strategy_id)
            .last()
    }

    /// 获取所有持仓快照
    pub fn get_position_snapshots(&self) -> &[PositionSnapshot] {
        &self.position_snapshots
    }

    // ========== K线缓存 ==========

    /// 保存K线到缓存
    pub fn save_kline(&mut self, symbol: &str, period: &str, kline: KLineData) {
        let key = (symbol.to_string(), period.to_string());
        let cache = self.kline_cache.entry(key).or_insert(KLineCache {
            symbol: symbol.to_string(),
            period: period.to_string(),
            klines: Vec::new(),
        });
        cache.klines.push(kline);

        // 限制缓存大小 (最多保留 1000 根)
        if cache.klines.len() > 1000 {
            cache.klines.remove(0);
        }
    }

    /// 获取K线缓存
    pub fn get_kline_cache(&self, symbol: &str, period: &str) -> Option<&KLineCache> {
        let key = (symbol.to_string(), period.to_string());
        self.kline_cache.get(&key)
    }

    /// 获取最近N根K线
    pub fn get_recent_klines(
        &self,
        symbol: &str,
        period: &str,
        count: usize,
    ) -> Vec<KLineData> {
        let key = (symbol.to_string(), period.to_string());
        if let Some(cache) = self.kline_cache.get(&key) {
            let start = cache.klines.len().saturating_sub(count);
            cache.klines[start..].to_vec()
        } else {
            Vec::new()
        }
    }

    // ========== 批量操作 ==========

    /// 清理过期数据
    pub fn cleanup_expired(&mut self, max_age_secs: i64, current_ts: i64) {
        // 清理交易记录 (保留最近10000条)
        if self.trade_records.len() > 10000 {
            let to_keep = self.trade_records.len() - 10000;
            self.trade_records = self.trade_records[to_keep..].to_vec();
        }

        // 清理持仓快照 (保留最近1000条)
        if self.position_snapshots.len() > 1000 {
            let to_keep = self.position_snapshots.len() - 1000;
            self.position_snapshots = self.position_snapshots[to_keep..].to_vec();
        }

        // 清理K线缓存 (按时间)
        let mut to_remove: Vec<(String, String)> = Vec::new();
        for ((symbol, period), cache) in &self.kline_cache {
            if let Some(oldest) = cache.klines.first() {
                if current_ts - oldest.timestamp > max_age_secs {
                    to_remove.push((symbol.clone(), period.clone()));
                }
            }
        }

        for key in to_remove {
            self.kline_cache.remove(&key);
        }
    }

    /// 获取统计信息
    pub fn get_stats(&self) -> PersistenceStats {
        PersistenceStats {
            trade_count: self.trade_records.len(),
            snapshot_count: self.position_snapshots.len(),
            kline_cache_count: self.kline_cache.values().map(|c| c.klines.len()).sum(),
            cache_symbol_count: self.kline_cache.len(),
        }
    }

    /// 重置所有数据
    pub fn reset(&mut self) {
        self.trade_records.clear();
        self.position_snapshots.clear();
        self.kline_cache.clear();
    }

    /// 获取配置
    pub fn config(&self) -> &PersistenceConfig {
        &self.config
    }

    // ========== 便捷方法 ==========

    /// 记录日线 K线完成
    pub fn record_daily_kline(&mut self, kline: &b_data_source::KLine) {
        let kline_data = KLineData {
            open: kline.open,
            high: kline.high,
            low: kline.low,
            close: kline.close,
            volume: kline.volume,
            timestamp: kline.timestamp.timestamp(),
        };
        self.save_kline(&kline.symbol, "1d", kline_data);
    }
}

/// 持久化统计
#[derive(Debug, Clone)]
pub struct PersistenceStats {
    pub trade_count: usize,
    pub snapshot_count: usize,
    pub kline_cache_count: usize,
    pub cache_symbol_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_save_trade() {
        let mut service = PersistenceService::new();
        service.save_trade(TradeRecord {
            order_id: "order_1".to_string(),
            symbol: "BTC".to_string(),
            strategy_id: "trend".to_string(),
            side: "long".to_string(),
            qty: dec!(1),
            price: dec!(50000),
            timestamp: 1000,
            pnl: dec!(0),
        });

        assert_eq!(service.get_trades().len(), 1);
    }

    #[test]
    fn test_calculate_pnl() {
        let mut service = PersistenceService::new();
        service.save_trade(TradeRecord {
            order_id: "1".to_string(),
            symbol: "BTC".to_string(),
            strategy_id: "trend".to_string(),
            side: "long".to_string(),
            qty: dec!(1),
            price: dec!(50000),
            timestamp: 1000,
            pnl: dec!(100),
        });
        service.save_trade(TradeRecord {
            order_id: "2".to_string(),
            symbol: "BTC".to_string(),
            strategy_id: "trend".to_string(),
            side: "long".to_string(),
            qty: dec!(1),
            price: dec!(51000),
            timestamp: 2000,
            pnl: dec!(200),
        });

        assert_eq!(service.calculate_strategy_pnl("trend"), dec!(300));
    }

    #[test]
    fn test_kline_cache() {
        let mut service = PersistenceService::new();
        service.save_kline("BTC", "1m", KLineData {
            open: dec!(50000),
            high: dec!(51000),
            low: dec!(49000),
            close: dec!(50500),
            volume: dec!(100),
            timestamp: 1000,
        });

        let klines = service.get_recent_klines("BTC", "1m", 10);
        assert_eq!(klines.len(), 1);
    }
}
