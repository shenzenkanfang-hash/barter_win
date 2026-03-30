//! TickContext - 全链路唯一状态容器（业务顺序 b→f→d→c→e）

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;

// ============================================================================
// 常量
// ============================================================================

pub const INITIAL_BALANCE: Decimal = rust_decimal_macros::dec!(10000);
pub const SYMBOL: &str = "HOTUSDT";
pub const DB_PATH: &str = "D:/RusProject/barter-rs-main/data/trade_records.db";
pub const DATA_FILE: &str = "D:/RusProject/barter-rs-main/data/HOTUSDT_1m_20251009_20251011.csv";

// ============================================================================
// 数据结构
// ============================================================================

#[derive(Debug, Clone)]
pub struct TickContext {
    pub tick_id: u64,
    pub timestamp: DateTime<Utc>,
    pub kline: RawKline,
    pub b_data: Option<BDataResult>,
    pub f_engine: Option<FEngineResult>,
    pub d_check: Option<DCheckResult>,
    pub c_data: Option<CDataResult>,
    pub e_risk: Option<ERiskResult>,
    pub visited: Vec<&'static str>,
    pub errors: Vec<StageError>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RawKline {
    pub open: Decimal,
    pub close: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub volume: Decimal,
    pub is_closed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct BDataResult {
    pub kline_id: u64,
    pub valid: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct FEngineResult {
    pub price_updated: bool,
    pub account_synced: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DCheckResult {
    pub decision: String,
    pub qty: Option<Decimal>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CDataResult {
    pub zscore_14: Option<f64>,
    pub tr_base: Option<Decimal>,
    pub pos_norm: Option<f64>,
    pub signal: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ERiskResult {
    pub balance_passed: bool,
    pub order_passed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct StageError {
    pub stage: String,
    pub code: String,
    pub detail: String,
}

impl TickContext {
    pub fn new(tick_id: u64, kline: RawKline) -> Self {
        Self {
            tick_id,
            timestamp: Utc::now(),
            kline,
            b_data: None,
            f_engine: None,
            d_check: None,
            c_data: None,
            e_risk: None,
            visited: vec![],
            errors: vec![],
        }
    }

    pub fn to_report(&self) -> serde_json::Value {
        serde_json::json!({
            "tick_id": self.tick_id,
            "timestamp": self.timestamp.to_rfc3339(),
            "complete": self.is_complete(),
            "visited_stages": self.visited,
            "errors": self.errors,
            "kline": {
                "close": self.kline.close.to_string(),
                "high": self.kline.high.to_string(),
                "low": self.kline.low.to_string(),
                "volume": self.kline.volume.to_string(),
            },
            "b_data": self.b_data,
            "f_engine": self.f_engine,
            "d_check": self.d_check,
            "c_data": self.c_data,
            "e_risk": self.e_risk,
        })
    }

    pub fn is_complete(&self) -> bool {
        let required = ["b", "f", "d", "c", "e"];
        required.iter().all(|s| self.visited.contains(s))
    }
}
