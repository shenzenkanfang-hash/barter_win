//! 模拟交易对规则服务
//!
//! 不依赖真实 API，提供固定规则

use parking_lot::RwLock;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// 解析后的交易对规则
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedSymbolRules {
    pub symbol: String,
    pub price_precision: i32,
    pub quantity_precision: i32,
    pub tick_size: Decimal,
    pub min_qty: Decimal,
    pub step_size: Decimal,
    pub min_notional: Decimal,
    pub max_notional: Decimal,
    pub leverage: i32,
    pub maker_fee: Decimal,
    pub taker_fee: Decimal,
    pub close_min_ratio: Decimal,
    pub min_value_threshold: Decimal,
    pub update_ts: i64,
}

impl ParsedSymbolRules {
    /// 获取有效最小数量
    pub fn effective_min_qty(&self) -> Decimal {
        std::cmp::max(self.min_qty, dec!(0.001))
    }

    /// 价格取整
    pub fn round_price(&self, price: Decimal) -> Decimal {
        if self.tick_size > dec!(0) {
            (price / self.tick_size).round() * self.tick_size
        } else {
            price
        }
    }

    /// 数量取整
    pub fn round_qty(&self, qty: Decimal) -> Decimal {
        let valid_qty = std::cmp::max(qty, self.effective_min_qty());
        (valid_qty / self.step_size).round() * self.step_size
    }

    /// 验证订单
    pub fn validate_order(&self, price: Decimal, qty: Decimal) -> bool {
        if price < dec!(0) || qty < dec!(0) {
            return false;
        }
        let order_notional = price * qty;
        qty >= self.effective_min_qty() && order_notional >= self.min_value_threshold
    }
}

/// 模拟规则服务
pub struct SymbolRuleService {
    rules: RwLock<HashMap<String, Arc<ParsedSymbolRules>>>,
}

impl SymbolRuleService {
    pub fn new() -> Self {
        let rules = RwLock::new(HashMap::new());

        // 预置 BTCUSDT 规则
        let btc_rules = ParsedSymbolRules {
            symbol: "BTCUSDT".to_string(),
            price_precision: 2,
            quantity_precision: 3,
            tick_size: dec!(0.01),
            min_qty: dec!(0.001),
            step_size: dec!(0.001),
            min_notional: dec!(5),
            max_notional: dec!(1000000),
            leverage: 20,
            maker_fee: dec!(0.0002),
            taker_fee: dec!(0.0005),
            close_min_ratio: dec!(0.0015),
            min_value_threshold: dec!(10),
            update_ts: chrono::Utc::now().timestamp(),
        };

        let eth_rules = ParsedSymbolRules {
            symbol: "ETHUSDT".to_string(),
            price_precision: 2,
            quantity_precision: 3,
            tick_size: dec!(0.01),
            min_qty: dec!(0.01),
            step_size: dec!(0.01),
            min_notional: dec!(5),
            max_notional: dec!(500000),
            leverage: 20,
            maker_fee: dec!(0.0002),
            taker_fee: dec!(0.0005),
            close_min_ratio: dec!(0.0015),
            min_value_threshold: dec!(10),
            update_ts: chrono::Utc::now().timestamp(),
        };

        rules.write().insert("BTCUSDT".to_string(), Arc::new(btc_rules));
        rules.write().insert("ETHUSDT".to_string(), Arc::new(eth_rules));

        Self { rules }
    }

    pub fn get_rules(&self, symbol: &str) -> Option<Arc<ParsedSymbolRules>> {
        self.rules.read().get(&symbol.to_uppercase()).cloned()
    }

    pub fn register_rules(&self, rules: ParsedSymbolRules) {
        self.rules.write().insert(rules.symbol.clone(), Arc::new(rules));
    }
}

impl Default for SymbolRuleService {
    fn default() -> Self {
        Self::new()
    }
}
