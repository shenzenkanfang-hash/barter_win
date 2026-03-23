//! Trend 策略品种限额模块
//!
//! **警告**: Pin 策略禁用此模块！

use fnv::FnvHashMap;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

/// 单品种持仓限制 (Trend 专用)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendSymbolLimit {
    /// 最大名义价值 (0 = 不限制)
    pub max_notional: Decimal,
    /// 最大数量 (0 = 不限制)
    pub max_qty: Decimal,
}

impl Default for TrendSymbolLimit {
    fn default() -> Self {
        Self {
            max_notional: dec!(5000), // 默认 5000 USDT
            max_qty: dec!(0),
        }
    }
}

/// 全局持仓限制 (Trend 专用)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendGlobalLimit {
    /// 全局最大名义价值 (0 = 不限制)
    pub max_total_notional: Decimal,
    /// 最大品种数 (0 = 不限制)
    pub max_symbol_count: u32,
}

impl Default for TrendGlobalLimit {
    fn default() -> Self {
        Self {
            max_total_notional: dec!(50000), // 默认 50000 USDT
            max_symbol_count: 10,
        }
    }
}

/// Trend策略限额守卫 (Trend 专用)
///
/// **警告**: Pin 策略不应使用此模块！
#[derive(Debug, Clone)]
pub struct TrendRiskLimitGuard {
    /// 单品种限制
    symbol_limit: TrendSymbolLimit,
    /// 全局限制
    global_limit: TrendGlobalLimit,
    /// 当前各品种名义价值 (FnvHashMap 优化)
    current_notionals: FnvHashMap<String, Decimal>,
    /// 当前各品种数量
    current_quantities: FnvHashMap<String, Decimal>,
}

impl TrendRiskLimitGuard {
    /// 创建 Trend 限额守卫
    pub fn new(symbol_limit: TrendSymbolLimit, global_limit: TrendGlobalLimit) -> Self {
        Self {
            symbol_limit,
            global_limit,
            current_notionals: FnvHashMap::default(),
            current_quantities: FnvHashMap::default(),
        }
    }

    /// 预检订单
    pub fn pre_check(
        &self,
        symbol: &str,
        order_notional: Decimal,
        _order_qty: Decimal,
    ) -> Result<(), String> {
        // 1. 检查单品种限额
        if self.symbol_limit.max_notional > dec!(0) {
            let current_notional = self.current_notionals.get(symbol).copied().unwrap_or(dec!(0));
            let new_notional = current_notional + order_notional;
            if new_notional > self.symbol_limit.max_notional {
                return Err(format!(
                    "Trend {} 名义价值 {} 超过单品种限额 {}",
                    symbol, new_notional, self.symbol_limit.max_notional
                ));
            }
        }

        // 2. 检查全局限额
        if self.global_limit.max_total_notional > dec!(0) {
            let total_notional: Decimal = self.current_notionals.values().sum();
            let new_total_notional = total_notional + order_notional;
            if new_total_notional > self.global_limit.max_total_notional {
                return Err(format!(
                    "全局名义价值 {} 超过限额 {}",
                    new_total_notional, self.global_limit.max_total_notional
                ));
            }
        }

        // 3. 检查品种数限额
        if self.global_limit.max_symbol_count > 0 {
            let current_symbols = self.current_notionals.len() as u32;
            if !self.current_notionals.contains_key(symbol) && current_symbols >= self.global_limit.max_symbol_count {
                return Err(format!(
                    "品种数 {} 达上限 {}",
                    current_symbols, self.global_limit.max_symbol_count
                ));
            }
        }

        Ok(())
    }

    /// 更新持仓
    pub fn update_position(&mut self, symbol: &str, notional: Decimal, qty: Decimal) {
        self.current_notionals.insert(symbol.to_string(), notional);
        self.current_quantities.insert(symbol.to_string(), qty);
    }

    /// 减少持仓
    pub fn reduce_position(&mut self, symbol: &str, notional: Decimal, qty: Decimal) {
        if let Some(current_notional) = self.current_notionals.get(symbol) {
            let new_notional = (*current_notional - notional).max(dec!(0));
            if new_notional <= dec!(0) {
                self.current_notionals.remove(symbol);
                self.current_quantities.remove(symbol);
            } else {
                self.current_notionals.insert(symbol.to_string(), new_notional);
                if let Some(current_qty) = self.current_quantities.get(symbol) {
                    let new_qty = (*current_qty - qty).max(dec!(0));
                    self.current_quantities.insert(symbol.to_string(), new_qty);
                }
            }
        }
    }

    /// 获取当前品种数
    pub fn symbol_count(&self) -> usize {
        self.current_notionals.len()
    }

    /// 清空所有持仓
    pub fn clear(&mut self) {
        self.current_notionals.clear();
        self.current_quantities.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pre_check_pass() {
        let guard = TrendRiskLimitGuard::new(
            TrendSymbolLimit::default(),
            TrendGlobalLimit::default(),
        );

        assert!(guard.pre_check("BTC", dec!(1000), dec!(0)).is_ok());
    }

    #[test]
    fn test_pre_check_single_symbol_limit() {
        let mut guard = TrendRiskLimitGuard::new(
            TrendSymbolLimit {
                max_notional: dec!(5000),
                max_qty: dec!(0),
            },
            TrendGlobalLimit::default(),
        );

        // 首次下单 3000，通过
        assert!(guard.pre_check("BTC", dec!(3000), dec!(0)).is_ok());
        guard.update_position("BTC", dec!(3000), dec!(0));

        // 再次下单 3000，总共 6000 > 5000，拒绝
        assert!(guard.pre_check("BTC", dec!(3000), dec!(0)).is_err());
    }

    #[test]
    fn test_pre_check_symbol_count_limit() {
        let mut guard = TrendRiskLimitGuard::new(
            TrendSymbolLimit::default(),
            TrendGlobalLimit {
                max_total_notional: dec!(0),
                max_symbol_count: 2,
            },
        );

        // 品种1 通过
        assert!(guard.pre_check("BTC", dec!(1000), dec!(0)).is_ok());
        guard.update_position("BTC", dec!(1000), dec!(0));

        // 品种2 通过
        assert!(guard.pre_check("ETH", dec!(1000), dec!(0)).is_ok());
        guard.update_position("ETH", dec!(1000), dec!(0));

        // 品种3 超过上限，拒绝
        assert!(guard.pre_check("SOL", dec!(1000), dec!(0)).is_err());
    }

    #[test]
    fn test_update_and_reduce_position() {
        let mut guard = TrendRiskLimitGuard::new(
            TrendSymbolLimit::default(),
            TrendGlobalLimit::default(),
        );

        guard.update_position("BTC", dec!(5000), dec!(0));
        assert_eq!(guard.symbol_count(), 1);

        guard.reduce_position("BTC", dec!(3000), dec!(0));
        assert_eq!(guard.current_notionals.get("BTC"), Some(&dec!(2000)));

        guard.reduce_position("BTC", dec!(2000), dec!(0));
        assert!(!guard.current_notionals.contains_key("BTC"));
    }
}
