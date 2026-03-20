use chrono::{DateTime, Utc};
use fnv::FnvHashMap;
use parking_lot::RwLock;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use strategy::types::Signal;

/// Check 表项 - 记录策略判断结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckEntry {
    /// 品种ID: "BTC", "ETH"
    pub symbol: String,
    /// 策略ID: "trend", "martin"
    pub strategy_id: String,
    /// 周期: "1m", "5m", "15m", "1d"
    pub period: String,

    // 指标信号
    /// EMA 信号
    pub ema_signal: Signal,
    /// RSI 数值: 0-100
    pub rsi_value: Decimal,
    /// Pine 颜色
    pub pine_color: indicator::PineColor,
    /// 价格位置: 0-100 (close-low)/(high-low)
    pub price_position: Decimal,

    // 最终判断
    /// 最终信号
    pub final_signal: Signal,
    /// 目标价格（下单用）
    pub target_price: Decimal,
    /// 目标数量（下单用）
    pub quantity: Decimal,
    /// 风险标记: true=需关注
    pub risk_flag: bool,

    // 元数据
    /// 时间戳
    pub timestamp: DateTime<Utc>,
    /// 轮次ID
    pub round_id: u64,
    /// 是否高速通道
    pub is_high_freq: bool,
}

/// Check 表 - 统一记录各流水线结果
///
/// 线程安全: 使用 RwLock 保护 entries
pub struct CheckTable {
    /// (品种, 策略, 周期) -> CheckEntry (使用 RwLock 保护)
    entries: RwLock<FnvHashMap<(String, String, String), CheckEntry>>,
    /// 当前轮次ID
    round_id: RwLock<u64>,
}

impl CheckTable {
    /// 创建新的 Check 表
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(FnvHashMap::default()),
            round_id: RwLock::new(0),
        }
    }

    /// 获取下一轮次ID (原子递增)
    pub fn next_round_id(&self) -> u64 {
        let mut round_id = self.round_id.write();
        *round_id += 1;
        *round_id
    }

    /// 填入 CheckEntry (写锁)
    pub fn fill(&self, entry: CheckEntry) {
        let key = (entry.symbol.clone(), entry.strategy_id.clone(), entry.period.clone());
        self.entries.write().insert(key, entry);
    }

    /// 获取 CheckEntry (读锁)
    pub fn get(&self, symbol: &str, strategy_id: &str, period: &str) -> Option<CheckEntry> {
        let key = (symbol.to_string(), strategy_id.to_string(), period.to_string());
        self.entries.read().get(&key).cloned()
    }

    /// 获取所有品种的 CheckEntry (按策略ID过滤) (克隆)
    pub fn get_by_strategy(&self, strategy_id: &str) -> Vec<CheckEntry> {
        self.entries
            .read()
            .values()
            .filter(|e| e.strategy_id == strategy_id)
            .cloned()
            .collect()
    }

    /// 获取所有高风险 Entry (克隆)
    pub fn get_high_risk(&self) -> Vec<CheckEntry> {
        self.entries
            .read()
            .values()
            .filter(|e| e.risk_flag)
            .cloned()
            .collect()
    }

    /// 清空所有 Entry (保留 round_id) (写锁)
    pub fn clear(&self) {
        self.entries.write().clear();
    }

    /// 当前轮次ID (读锁)
    pub fn current_round_id(&self) -> u64 {
        *self.round_id.read()
    }
}

impl Default for CheckTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_table_basic() {
        let table = CheckTable::new();
        assert_eq!(table.current_round_id(), 0);

        let round_id = table.next_round_id();
        assert_eq!(round_id, 1);

        let entry = CheckEntry {
            symbol: "BTCUSDT".to_string(),
            strategy_id: "trend".to_string(),
            period: "1m".to_string(),
            ema_signal: Signal::LongEntry,
            rsi_value: Decimal::from(60),
            pine_color: indicator::PineColor::PureGreen,
            price_position: Decimal::from(75),
            final_signal: Signal::LongEntry,
            target_price: Decimal::from(50000),
            quantity: dec!(0.1),
            risk_flag: false,
            timestamp: Utc::now(),
            round_id,
            is_high_freq: false,
        };

        table.fill(entry);
        assert!(table.get("BTCUSDT", "trend", "1m").is_some());
    }
}
