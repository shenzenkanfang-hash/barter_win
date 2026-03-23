#![forbid(unsafe_code)]

//! 交易对规则服务模块
//!
//! 参考 Python 版本 symbol_rule_service.py 实现
//! 功能：
//! - 交易规则缓存（有过期时间）
//! - 价格/数量取整
//! - 订单验证
//! - 基于名义价值计算合规开仓数量

use a_common::api::{BinanceApiGateway, SymbolRulesData};
use a_common::MarketError;
use chrono::Utc;
use parking_lot::RwLock;
use rust_decimal::Decimal;
use rust_decimal::MathematicalOps;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 规则缓存条目
#[derive(Debug, Clone)]
struct RuleCacheEntry {
    rules: ParsedSymbolRules,
    fetched_at: Instant,
}

/// 解析后的交易对规则（完整版，对应 Python 的 SymbolRules）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedSymbolRules {
    pub symbol: String,
    /// 价格精度
    pub price_precision: i32,
    /// 数量精度
    pub quantity_precision: i32,
    /// 步长（价格最小变动）
    pub tick_size: Decimal,
    /// 交易所原始最小数量
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
    /// 下单最小名义价值阈值（略高于交易所限制）
    pub min_value_threshold: Decimal,
    /// 规则最后更新时间戳
    pub update_ts: i64,
}

impl ParsedSymbolRules {
    /// 有效最小开仓数量（自动计算，取代原 min_qty 直接使用）
    pub fn effective_min_qty(&self) -> Decimal {
        // 第一步：基于最小名义价值计算理论最小数量
        let theoretical_min_qty = if self.tick_size > dec!(0) {
            self.min_notional / self.tick_size
        } else {
            self.min_notional
        };

        // 第二步：确保不低于交易所原始最小数量
        let base_min_qty = std::cmp::max(theoretical_min_qty, self.min_qty);

        // 第三步：按数量精度取整
        let rounded = Self::round_to_precision(base_min_qty, self.quantity_precision);

        // 第四步：最终校验，确保取整后仍满足最小名义价值
        let mut final_qty = rounded;
        let tick = if self.tick_size > dec!(0) { self.tick_size } else { dec!(1) };
        while final_qty * tick < self.min_notional {
            final_qty = final_qty + self.step_size;
        }

        final_qty
    }

    /// 按精度取整
    fn round_to_precision(value: Decimal, precision: i32) -> Decimal {
        if precision <= 0 {
            return value.round_dp(0);
        }
        let factor = dec!(10).powd(-Decimal::from(precision));
        (value / factor).round() * factor
    }

    /// 价格取整
    pub fn round_price(&self, price: Decimal) -> Decimal {
        if price < dec!(0) {
            panic!("价格不能为负数：{}", price);
        }
        if self.tick_size > dec!(0) {
            let rounded = (price / self.tick_size).round() * self.tick_size;
            rounded.round_dp(self.price_precision as u32)
        } else {
            price.round_dp(self.price_precision as u32)
        }
    }

    /// 数量取整（自动使用 effective_min_qty）
    pub fn round_qty(&self, qty: Decimal) -> Decimal {
        if qty < dec!(0) {
            panic!("数量不能为负数：{}", qty);
        }
        // 使用实际有效最小数量替代原始 min_qty
        let valid_qty = std::cmp::max(qty, self.effective_min_qty());
        Self::round_to_precision(valid_qty, self.quantity_precision)
    }

    /// 验证订单是否符合最小名义价值要求
    pub fn validate_order(&self, price: Decimal, qty: Decimal) -> bool {
        if price < dec!(0) || qty < dec!(0) {
            return false;
        }
        let order_notional = price * qty;
        qty >= self.effective_min_qty() && order_notional >= self.min_value_threshold
    }

    /// 基于名义价值计算合规开仓数量
    /// 使用 Decimal 保证高精度计算
    pub fn calculate_open_qty(&self, open_notional: Decimal, open_price: Decimal) -> Decimal {
        // 1. 参数合法性校验
        if open_notional <= dec!(0) {
            panic!("开仓名义价值必须大于0：{}", open_notional);
        }
        if open_price <= dec!(0) {
            panic!("开仓价格必须大于0：{}", open_price);
        }

        // 2. 基础数量计算：数量 = 名义价值 / 价格
        let base_qty = open_notional / open_price;

        // 3. 确保数量不低于有效最小数量
        let valid_qty = std::cmp::max(base_qty, self.effective_min_qty());

        // 4. 按数量精度取整（四舍五入）
        let rounded_qty = Self::round_to_precision(valid_qty, self.quantity_precision);

        // 5. 最终校验：确保取整后的数量仍满足最小名义价值要求
        let mut final_qty = rounded_qty;
        let tick = if self.tick_size > dec!(0) { self.tick_size } else { dec!(1) };
        while final_qty * tick < self.min_value_threshold {
            final_qty = final_qty + self.step_size;
        }

        final_qty
    }
}

/// 交易对规则服务
pub struct SymbolRuleService {
    /// 规则缓存（按 symbol 索引）
    cache: RwLock<HashMap<String, Arc<RuleCacheEntry>>>,
    /// 缓存过期时间
    cache_ttl: Duration,
    /// API 网关
    gateway: BinanceApiGateway,
}

impl SymbolRuleService {
    /// 创建规则服务（默认 1 小时缓存）
    pub fn new() -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
            cache_ttl: Duration::from_secs(3600),
            gateway: BinanceApiGateway::new(),
        }
    }

    /// 创建规则服务（自定义缓存时间）
    pub fn with_ttl(ttl_secs: u64) -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
            cache_ttl: Duration::from_secs(ttl_secs),
            gateway: BinanceApiGateway::new(),
        }
    }

    /// 获取交易对规则（带缓存，过期自动重新获取）
    pub async fn get_rules(&self, symbol: &str) -> Result<ParsedSymbolRules, MarketError> {
        let symbol_upper = symbol.to_uppercase();

        // 1. 检查缓存
        {
            let cache = self.cache.read();
            if let Some(entry) = cache.get(&symbol_upper) {
                if entry.fetched_at.elapsed() < self.cache_ttl {
                    return Ok(entry.rules.clone());
                }
            }
        }

        // 2. 缓存过期或不存在，重新获取
        let raw_rules = self.gateway.fetch_symbol_rules(&symbol_upper).await
            .map_err(|e| MarketError::NetworkError(e.to_string()))?;

        // 3. 解析为完整规则
        let parsed = self.parse_rules(raw_rules)?;

        // 4. 更新缓存
        {
            let mut cache = self.cache.write();
            cache.insert(symbol_upper.clone(), Arc::new(RuleCacheEntry {
                rules: parsed.clone(),
                fetched_at: Instant::now(),
            }));
        }

        Ok(parsed)
    }

    /// 获取规则（同步版本，使用缓存）
    pub fn get_rules_sync(&self, symbol: &str) -> Option<ParsedSymbolRules> {
        let symbol_upper = symbol.to_uppercase();
        let cache = self.cache.read();
        cache.get(&symbol_upper).map(|entry| entry.rules.clone())
    }

    /// 解析原始规则为完整规则
    fn parse_rules(&self, raw: SymbolRulesData) -> Result<ParsedSymbolRules, MarketError> {
        let tick_size = raw.tick_size;
        let min_qty = raw.min_qty;
        let quantity_precision = raw.quantity_precision;

        // 计算 step_size
        let step_size = if quantity_precision > 0 {
            Decimal::from(10).powd(-Decimal::from(quantity_precision))
        } else {
            dec!(1)
        };

        // 平仓最小盈亏比阈值
        let taker_fee = raw.taker_fee;
        let close_min_ratio = ((dec!(1) + taker_fee) / (dec!(1) - taker_fee) - dec!(1)).abs() * dec!(1.5);

        Ok(ParsedSymbolRules {
            symbol: raw.symbol,
            price_precision: raw.price_precision as i32,
            quantity_precision: raw.quantity_precision as i32,
            tick_size,
            min_qty,
            step_size,
            min_notional: raw.min_notional,
            max_notional: raw.max_notional,
            leverage: raw.leverage,
            maker_fee: raw.maker_fee,
            taker_fee,
            close_min_ratio,
            min_value_threshold: raw.min_notional + dec!(2),
            update_ts: Utc::now().timestamp(),
        })
    }

    /// 清除缓存
    pub fn clear_cache(&self) {
        let mut cache = self.cache.write();
        cache.clear();
    }

    /// 移除单个 symbol 缓存
    pub fn invalidate(&self, symbol: &str) {
        let mut cache = self.cache.write();
        cache.remove(&symbol.to_uppercase());
    }
}

impl Default for SymbolRuleService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_effective_min_qty() {
        let rules = ParsedSymbolRules {
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
            min_value_threshold: dec!(7),
            update_ts: 0,
        };

        let effective = rules.effective_min_qty();
        assert!(effective >= dec!(0.001));
        assert!(effective * rules.tick_size >= rules.min_notional);
    }

    #[test]
    fn test_round_price() {
        let rules = ParsedSymbolRules {
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
            min_value_threshold: dec!(7),
            update_ts: 0,
        };

        let rounded = rules.round_price(dec!(68000.456));
        assert_eq!(rounded, dec!(68000.46));
    }

    #[test]
    fn test_round_qty() {
        let rules = ParsedSymbolRules {
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
            min_value_threshold: dec!(7),
            update_ts: 0,
        };

        let rounded = rules.round_qty(dec!(0.0001));
        // 应该不小于 effective_min_qty
        assert!(rounded >= rules.effective_min_qty());
    }

    #[test]
    fn test_validate_order() {
        let rules = ParsedSymbolRules {
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
            min_value_threshold: dec!(7),
            update_ts: 0,
        };

        // 有效订单
        assert!(rules.validate_order(dec!(68000), dec!(0.001)));

        // 无效订单（数量太小）
        assert!(!rules.validate_order(dec!(68000), dec!(0.00001)));
    }
}
