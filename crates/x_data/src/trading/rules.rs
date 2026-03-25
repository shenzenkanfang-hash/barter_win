//! 交易规则数据类型

#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

// ============================================================================
// SymbolRulesData
// ============================================================================

/// 交易对规则数据（原始API响应结构）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolRulesData {
    /// 交易品种
    pub symbol: String,
    /// 价格精度
    pub price_precision: u32,
    /// 数量精度
    pub quantity_precision: u32,
    /// 步长（价格最小变动）
    pub tick_size: Decimal,
    /// 最小数量
    pub min_qty: Decimal,
    /// 步进数量
    pub step_size: Decimal,
    /// 最小名义价值
    pub min_notional: Decimal,
    /// 最大名义价值
    pub max_notional: Decimal,
    /// 杠杆
    pub leverage: i32,
    /// 最大可用杠杆
    pub max_leverage: i32,
    /// 做市商费率
    pub maker_fee: Decimal,
    /// 吃单费率
    pub taker_fee: Decimal,
}

// ============================================================================
// ParsedSymbolRules
// ============================================================================

/// 解析后的交易规则（完整规则）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedSymbolRules {
    /// 交易品种
    pub symbol: String,
    /// 价格精度
    pub price_precision: i32,
    /// 数量精度
    pub quantity_precision: i32,
    /// 步长（价格最小变动）
    pub tick_size: Decimal,
    /// 最小数量
    pub min_qty: Decimal,
    /// 步进数量
    pub step_size: Decimal,
    /// 最小名义价值
    pub min_notional: Decimal,
    /// 最大名义价值
    pub max_notional: Decimal,
    /// 杠杆倍数
    pub leverage: i32,
    /// Maker 手续费率
    pub maker_fee: Decimal,
    /// Taker 手续费率
    pub taker_fee: Decimal,
    /// 平仓最小盈亏比阈值
    pub close_min_ratio: Decimal,
    /// 下单最小名义价值阈值
    pub min_value_threshold: Decimal,
    /// 规则最后更新时间戳
    pub update_ts: i64,
}

impl ParsedSymbolRules {
    /// 有效最小数量
    pub fn effective_min_qty(&self) -> Decimal {
        let min_notional = self.min_notional;
        if min_notional > Decimal::ZERO && self.tick_size > Decimal::ZERO {
            let price_for_min_notional = min_notional / self.tick_size;
            let qty = price_for_min_notional.ceil();
            if qty < self.min_qty {
                return self.min_qty;
            }
            qty
        } else {
            self.min_qty
        }
    }
}
