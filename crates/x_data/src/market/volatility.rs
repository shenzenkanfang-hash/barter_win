//! 波动率数据类型

#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

/// 品种波动率
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolVolatility {
    /// 交易品种
    pub symbol: String,
    /// 波动率
    pub volatility: Decimal,
    /// 波动率排名
    pub rank: u32,
    /// 更新时间
    pub updated_at: DateTime<Utc>,
}

/// 波动率摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolatilitySummary {
    /// 品种列表
    pub symbols: Vec<SymbolVolatility>,
    /// 计算时间
    pub calculated_at: DateTime<Utc>,
}
