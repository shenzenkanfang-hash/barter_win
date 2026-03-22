use chrono::{DateTime, Utc};
use fnv::FnvHashMap;
use parking_lot::RwLock;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use c_data_process::Signal;
use c_data_process::types::PineColor;

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
    pub pine_color: PineColor,
    /// 价格位置: 0-100
    pub price_position: Decimal,

    // 最终判断
    /// 最终信号
    pub final_signal: Signal,
    /// 目标价格
    pub target_price: Decimal,
    /// 目标数量
    pub quantity: Decimal,
    /// 风险标记
    pub risk_flag: bool,

    // 元数据
    pub timestamp: DateTime<Utc>,
    pub round_id: u64,
    pub is_high_freq: bool,
}

/// Check 表 - 统一记录各流水线结果
pub struct CheckTable {
    entries: RwLock<FnvHashMap<(String, String, String), CheckEntry>>,
    round_id: RwLock<u64>,
}

impl CheckTable {
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(FnvHashMap::default()),
            round_id: RwLock::new(0),
        }
    }

    pub fn next_round_id(&self) -> u64 {
        let mut round_id = self.round_id.write();
        *round_id += 1;
        *round_id
    }

    pub fn fill(&self, entry: CheckEntry) {
        let key = (entry.symbol.clone(), entry.strategy_id.clone(), entry.period.clone());
        self.entries.write().insert(key, entry);
    }

    pub fn get(&self, symbol: &str, strategy_id: &str, period: &str) -> Option<CheckEntry> {
        let key = (symbol.to_string(), strategy_id.to_string(), period.to_string());
        self.entries.read().get(&key).cloned()
    }

    pub fn get_by_strategy(&self, strategy_id: &str) -> Vec<CheckEntry> {
        self.entries
            .read()
            .values()
            .filter(|e| e.strategy_id == strategy_id)
            .cloned()
            .collect()
    }

    pub fn get_high_risk(&self) -> Vec<CheckEntry> {
        self.entries
            .read()
            .values()
            .filter(|e| e.risk_flag)
            .cloned()
            .collect()
    }

    pub fn clear(&self) {
        self.entries.write().clear();
    }

    pub fn current_round_id(&self) -> u64 {
        *self.round_id.read()
    }
}

impl Default for CheckTable {
    fn default() -> Self {
        Self::new()
    }
}
