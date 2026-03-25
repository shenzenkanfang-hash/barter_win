//! 交易规则数据类型

#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// 交易对规则数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolRulesData {
    /// 交易品种
    pub symbol: String,
    /// 价格精度
    pub price_precision: u32,
    /// 数量精度
    pub quantity_precision: u32,
    /// 最小下单数量
    pub min_qty: Decimal,
    /// 最大下单数量
    pub max_qty: Decimal,
    /// 步进数量
    pub step_size: Decimal,
    /// 最小价格
    pub min_price: Decimal,
    /// 最大价格
    pub max_price: Decimal,
    /// 价格步进
    pub tick_size: Decimal,
    /// 最大杠杆
    pub max_leverage: u32,
    /// 合约类型
    pub contract_type: String,
}

/// 解析后的交易规则
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedSymbolRules {
    /// 交易品种
    pub symbol: String,
    /// 价格精度
    pub price_precision: u32,
    /// 数量精度
    pub quantity_precision: u32,
    /// 最小下单数量
    pub min_qty: Decimal,
    /// 最大下单数量
    pub max_qty: Decimal,
    /// 步进数量
    pub step_size: Decimal,
    /// 最小价格
    pub min_price: Decimal,
    /// 最大价格
    pub max_price: Decimal,
    /// 价格步进
    pub tick_size: Decimal,
    /// 最大杠杆
    pub max_leverage: u32,
    /// 合约类型
    pub contract_type: String,
}
