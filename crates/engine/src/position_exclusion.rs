use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 持仓方向
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PositionDirection {
    /// 多头
    Long,
    /// 空头
    Short,
    /// 无持仓
    None,
}

impl Default for PositionDirection {
    fn default() -> Self {
        PositionDirection::None
    }
}

/// 持仓信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionInfo {
    /// 方向
    pub direction: PositionDirection,
    /// 数量
    pub qty: Decimal,
    /// 均价
    pub avg_price: Decimal,
}

impl Default for PositionInfo {
    fn default() -> Self {
        Self {
            direction: PositionDirection::None,
            qty: dec!(0),
            avg_price: dec!(0),
        }
    }
}

/// 仓位互斥检查器
///
/// 实现同品种同策略的持仓互斥逻辑。
/// - 同品种 + 同策略: LONG 和 SHORT 互斥
/// - 同品种 + 不同策略: 不互斥，各自独立持仓
///
/// 设计依据: 设计文档 16.8
pub struct PositionExclusionChecker {
    /// 持仓映射: (symbol, strategy_id) -> PositionInfo
    positions: HashMap<(String, String), PositionInfo>,
    /// 跨品种互斥配置
    cross_symbol_mutex: HashMap<String, Vec<String>>,
}

impl Default for PositionExclusionChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl PositionExclusionChecker {
    /// 创建仓位互斥检查器
    pub fn new() -> Self {
        Self {
            positions: HashMap::new(),
            cross_symbol_mutex: HashMap::new(),
        }
    }

    /// 更新持仓信息
    pub fn update_position(
        &mut self,
        symbol: &str,
        strategy_id: &str,
        direction: PositionDirection,
        qty: Decimal,
        avg_price: Decimal,
    ) {
        let key = (symbol.to_string(), strategy_id.to_string());
        self.positions.insert(
            key,
            PositionInfo {
                direction,
                qty,
                avg_price,
            },
        );
    }

    /// 清除持仓
    pub fn clear_position(&mut self, symbol: &str, strategy_id: &str) {
        let key = (symbol.to_string(), strategy_id.to_string());
        self.positions.remove(&key);
    }

    /// 获取持仓信息
    pub fn get_position(&self, symbol: &str, strategy_id: &str) -> PositionInfo {
        let key = &(symbol.to_string(), strategy_id.to_string());
        self.positions.get(key).cloned().unwrap_or_default()
    }

    /// 检查是否有多头持仓
    pub fn has_long_position(&self, symbol: &str, strategy_id: &str) -> bool {
        let pos = self.get_position(symbol, strategy_id);
        pos.direction == PositionDirection::Long && pos.qty > dec!(0)
    }

    /// 检查是否有空头持仓
    pub fn has_short_position(&self, symbol: &str, strategy_id: &str) -> bool {
        let pos = self.get_position(symbol, strategy_id);
        pos.direction == PositionDirection::Short && pos.qty > dec!(0)
    }

    /// 检查是否有任何持仓
    pub fn has_position(&self, symbol: &str, strategy_id: &str) -> bool {
        self.has_long_position(symbol, strategy_id) || self.has_short_position(symbol, strategy_id)
    }

    /// 多空互斥检查
    ///
    /// 检查同一品种同一策略的多空持仓是否互斥。
    /// 返回: (can_open_long, can_open_short)
    pub fn check_long_short_mutex(&self, symbol: &str, strategy_id: &str) -> (bool, bool) {
        let pos = self.get_position(symbol, strategy_id);

        match pos.direction {
            PositionDirection::Long => (false, true),   // 已有多头，只能开空
            PositionDirection::Short => (true, false),  // 已有空头，只能开多
            PositionDirection::None => (true, true),    // 无持仓，两者都可以
        }
    }

    /// 跨品种互斥检查
    ///
    /// 如果配置了跨品种互斥，检查目标品种是否可以开仓。
    /// 例如: BTC 和 ETH 互斥，持有 BTC 多头时不能开 ETH 多头。
    pub fn check_cross_symbol_mutex(
        &self,
        symbol: &str,
        direction: PositionDirection,
        target_symbol: &str,
    ) -> bool {
        // 如果没有配置跨品种互斥，默认不互斥
        if let Some(muted_symbols) = self.cross_symbol_mutex.get(symbol) {
            // 检查是否有互斥品种处于同一方向持仓（跨品种不区分策略）
            for muted_sym in muted_symbols {
                // 直接遍历 positions 查找该品种任意策略的持仓
                for ((sym, _), pos) in &self.positions {
                    if sym == muted_sym && pos.direction == direction && pos.qty > dec!(0) {
                        return false; // 互斥品种有同向持仓，不能开仓
                    }
                }
            }
        }
        true
    }

    /// 配置跨品种互斥
    ///
    /// symbol 与 mutex_symbols 互斥 (同方向持仓时不能同时开仓)
    pub fn add_cross_symbol_mutex(&mut self, symbol: &str, mutex_symbols: Vec<String>) {
        self.cross_symbol_mutex
            .insert(symbol.to_string(), mutex_symbols);
    }

    /// 检查开仓是否允许
    ///
    /// 综合多空互斥和跨品种互斥判断。
    pub fn can_open_position(
        &self,
        symbol: &str,
        strategy_id: &str,
        direction: PositionDirection,
    ) -> bool {
        match direction {
            PositionDirection::Long => {
                let (can_long, _) = self.check_long_short_mutex(symbol, strategy_id);
                can_long && self.check_cross_symbol_mutex(symbol, direction, symbol)
            }
            PositionDirection::Short => {
                let (_, can_short) = self.check_long_short_mutex(symbol, strategy_id);
                can_short && self.check_cross_symbol_mutex(symbol, direction, symbol)
            }
            PositionDirection::None => true,
        }
    }

    /// 获取所有持仓的品种
    pub fn get_all_symbols(&self) -> Vec<String> {
        let mut symbols: Vec<String> = self
            .positions
            .keys()
            .map(|(s, _)| s.clone())
            .collect();
        symbols.sort();
        symbols.dedup();
        symbols
    }

    /// 获取指定策略的所有持仓品种
    pub fn get_symbols_by_strategy(&self, strategy_id: &str) -> Vec<String> {
        self.positions
            .iter()
            .filter(|((_, s), _)| s == strategy_id)
            .map(|((sym, _), _)| sym.clone())
            .collect()
    }

    /// 计算总持仓数量 (按方向)
    pub fn total_position_by_direction(&self, direction: PositionDirection) -> Decimal {
        self.positions
            .values()
            .filter(|p| p.direction == direction && p.qty > dec!(0))
            .map(|p| p.qty)
            .sum()
    }

    /// 重置所有持仓
    pub fn reset(&mut self) {
        self.positions.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_long_short_mutex_no_position() {
        let checker = PositionExclusionChecker::new();
        let (can_long, can_short) = checker.check_long_short_mutex("BTC", "trend");
        assert!(can_long);
        assert!(can_short);
    }

    #[test]
    fn test_long_short_mutex_with_long() {
        let mut checker = PositionExclusionChecker::new();
        checker.update_position("BTC", "trend", PositionDirection::Long, dec!(1), dec!(50000));
        let (can_long, can_short) = checker.check_long_short_mutex("BTC", "trend");
        assert!(!can_long);  // 已有多头，不能再开
        assert!(can_short); // 可以开空
    }

    #[test]
    fn test_long_short_mutex_with_short() {
        let mut checker = PositionExclusionChecker::new();
        checker.update_position("BTC", "trend", PositionDirection::Short, dec!(1), dec!(50000));
        let (can_long, can_short) = checker.check_long_short_mutex("BTC", "trend");
        assert!(can_long);   // 可以开多
        assert!(!can_short); // 已有空头，不能再开
    }

    #[test]
    fn test_can_open_position() {
        let mut checker = PositionExclusionChecker::new();
        checker.update_position("BTC", "trend", PositionDirection::Long, dec!(1), dec!(50000));
        assert!(!checker.can_open_position("BTC", "trend", PositionDirection::Long));
        assert!(checker.can_open_position("BTC", "trend", PositionDirection::Short));
    }

    #[test]
    fn test_cross_symbol_mutex() {
        let mut checker = PositionExclusionChecker::new();
        // 配置 BTC 和 ETH 互斥
        checker.add_cross_symbol_mutex("BTC", vec!["ETH".to_string()]);
        checker.add_cross_symbol_mutex("ETH", vec!["BTC".to_string()]);

        // 持有 BTC 多头
        checker.update_position("BTC", "trend", PositionDirection::Long, dec!(1), dec!(50000));

        // 不能开 ETH 多头 (跨品种互斥)
        assert!(!checker.can_open_position("ETH", "trend", PositionDirection::Long));

        // 可以开 ETH 空头 (不同方向)
        assert!(checker.can_open_position("ETH", "trend", PositionDirection::Short));
    }
}
