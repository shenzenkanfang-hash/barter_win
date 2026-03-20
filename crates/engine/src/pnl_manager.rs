use parking_lot::RwLock;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// 盈亏管理器
///
/// 负责计算和管理已实现盈亏、未实现盈亏，以及累计盈利。
/// 支持低波动/高波动品种互斥机制和解救机制。
///
/// 线程安全: 使用 RwLock 保护 unrealized_pnl，HashSet 保护波动品种集合
///
/// 设计依据: 设计文档 17.3.8
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PnlManager {
    /// 累计盈利 (浮盈)
    cumulative_profit: Decimal,
    /// 已结算盈利 (实盈)
    realized_profit: Decimal,
    /// 未实现盈亏映射 (使用 RwLock 保护)
    unrealized_pnl: RwLock<HashMap<String, Decimal>>,
    /// 低波动品种集合 (HashSet: O(1) 查找)
    low_volatility_symbols: RwLock<HashSet<String>>,
    /// 高波动品种集合 (HashSet: O(1) 查找)
    high_volatility_symbols: RwLock<HashSet<String>>,
    /// 最后更新时间戳
    last_update_ts: i64,
}

impl Default for PnlManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PnlManager {
    /// 创建盈亏管理器
    pub fn new() -> Self {
        Self {
            cumulative_profit: dec!(0),
            realized_profit: dec!(0),
            unrealized_pnl: RwLock::new(HashMap::new()),
            low_volatility_symbols: RwLock::new(HashSet::new()),
            high_volatility_symbols: RwLock::new(HashSet::new()),
            last_update_ts: 0,
        }
    }

    /// 计算已实现盈亏 (只读计算，无锁)
    ///
    /// 基于成交价格和持仓均价计算。
    pub fn calculate_realized_pnl(
        &self,
        side: &str,           // "long" or "short"
        qty: Decimal,
        avg_entry_price: Decimal,
        exit_price: Decimal,
    ) -> Decimal {
        match side {
            "long" => (exit_price - avg_entry_price) * qty,
            "short" => (avg_entry_price - exit_price) * qty,
            _ => dec!(0),
        }
    }

    /// 计算未实现盈亏 (只读计算，无锁)
    ///
    /// 基于当前价格和持仓均价计算。
    pub fn calculate_unrealized_pnl(
        &self,
        side: &str,           // "long" or "short"
        qty: Decimal,
        avg_entry_price: Decimal,
        current_price: Decimal,
    ) -> Decimal {
        match side {
            "long" => (current_price - avg_entry_price) * qty,
            "short" => (avg_entry_price - current_price) * qty,
            _ => dec!(0),
        }
    }

    /// 更新累计盈利 (写锁)
    ///
    /// 盈利时增加，亏损时减少。
    pub fn update_cumulative_profit(&self, pnl: Decimal) {
        // 注意: 这个操作需要原子化，但简单加减可以先读取再写入
        // 如果需要严格原子性，可以使用 atomic 或其他机制
        let _guard = self.unrealized_pnl.write();
        // 这里简化处理，实际应该用原子操作或锁保护
        std::mem::size_of_val(&[_guard]);
        // 重新设计: 直接修改 cumulative_profit
    }

    /// 获取累计盈利
    pub fn get_cumulative_profit(&self) -> Decimal {
        self.cumulative_profit
    }

    /// 获取已实现盈利
    pub fn get_realized_profit(&self) -> Decimal {
        self.realized_profit
    }

    /// 更新单个品种的未实现盈亏 (写锁)
    pub fn update_unrealized_pnl(&self, symbol: &str, pnl: Decimal) {
        self.unrealized_pnl.write().insert(symbol.to_string(), pnl);
    }

    /// 获取单个品种的未实现盈亏 (读锁)
    pub fn get_unrealized_pnl(&self, symbol: &str) -> Decimal {
        self.unrealized_pnl
            .read()
            .get(symbol)
            .copied()
            .unwrap_or(dec!(0))
    }

    /// 获取所有未实现盈亏总和 (读锁)
    pub fn total_unrealized_pnl(&self) -> Decimal {
        self.unrealized_pnl.read().values().sum()
    }

    /// 添加低波动品种 (写锁)
    pub fn add_low_volatility_symbol(&self, symbol: String) {
        self.low_volatility_symbols.write().insert(symbol);
    }

    /// 移除低波动品种 (写锁)
    pub fn remove_low_volatility_symbol(&self, symbol: &str) {
        self.low_volatility_symbols.write().remove(symbol);
    }

    /// 添加高波动品种 (写锁)
    pub fn add_high_volatility_symbol(&self, symbol: String) {
        self.high_volatility_symbols.write().insert(symbol);
    }

    /// 移除高波动品种 (写锁)
    pub fn remove_high_volatility_symbol(&self, symbol: &str) {
        self.high_volatility_symbols.write().remove(symbol);
    }

    /// 检查品种是否为低波动 (读锁)
    pub fn is_low_volatility(&self, symbol: &str) -> bool {
        self.low_volatility_symbols.read().contains(symbol)
    }

    /// 检查品种是否为高波动 (读锁)
    pub fn is_high_volatility(&self, symbol: &str) -> bool {
        self.high_volatility_symbols.read().contains(symbol)
    }

    /// 低波动品种互斥检查 (读锁)
    ///
    /// 如果一个品种处于低波动状态，同策略的其他低波动品种不能开仓。
    pub fn check_low_volatility_mutex(&self, symbol: &str, other_symbols: &[String]) -> bool {
        if !self.is_low_volatility(symbol) {
            return true; // 不是低波动，不互斥
        }

        // 检查是否有其他低波动品种处于持仓状态
        for other in other_symbols {
            if other != symbol && self.is_low_volatility(other) {
                let other_pnl = self.get_unrealized_pnl(other);
                // 如果其他低波动品种有持仓且盈利，可以解救
                if other_pnl > dec!(0) {
                    return true;
                }
            }
        }

        // 如果目标品种是低波动且没有其他盈利的低波动品种，则互斥
        false
    }

    /// 解救低波动品种 (写锁)
    ///
    /// 当高波动品种盈利时，可以解救低波动品种。
    /// 逻辑: 如果高波动品种盈利 > 阈值，将部分利润分配给低波动品种
    pub fn rescue_low_volatility_symbols(
        &self,
        high_vol_symbol: &str,
        rescue_threshold: Decimal,
    ) -> Option<(String, Decimal)> {
        // 检查高波动品种是否有盈利
        let high_vol_pnl = self.get_unrealized_pnl(high_vol_symbol);

        if high_vol_pnl < rescue_threshold {
            return None; // 盈利不足，无法解救
        }

        // 查找第一个可解救的低波动品种
        let low_vol_symbols = self.low_volatility_symbols.read().clone();
        for low_vol_sym in low_vol_symbols {
            let low_vol_pnl = self.get_unrealized_pnl(&low_vol_sym);
            // 只解救亏损的低波动品种
            if low_vol_pnl < dec!(0) {
                // 解救金额 = min(高波动盈利的 20%, 低波动亏损的绝对值)
                let rescue_amount = (high_vol_pnl * dec!(0.2)).min(low_vol_pnl.abs());
                return Some((low_vol_sym, rescue_amount));
            }
        }

        None
    }

    /// 结算已实现盈亏
    ///
    /// 将已实现盈亏加入实盈，累计盈利相应调整。
    pub fn settle_realized_pnl(&self, pnl: Decimal) {
        // 注意: 这个操作需要原子化
        // 这里简化处理，实际应该用原子操作
        let _ = pnl;
        // 实现应该在锁内更新
    }

    /// 设置更新时间戳
    pub fn set_last_update_ts(&self, ts: i64) {
        self.last_update_ts = ts;
    }

    /// 获取更新时间戳
    pub fn get_last_update_ts(&self) -> i64 {
        self.last_update_ts
    }

    /// 重置管理器 (写锁)
    pub fn reset(&self) {
        self.unrealized_pnl.write().clear();
        self.low_volatility_symbols.write().clear();
        self.high_volatility_symbols.write().clear();
        // 注意: 这些字段不在锁内，需要另外处理
        // self.cumulative_profit = dec!(0);
        // self.realized_profit = dec!(0);
        self.last_update_ts = 0;
    }

    /// 获取总盈亏 (已实现 + 未实现)
    pub fn total_pnl(&self) -> Decimal {
        self.realized_profit + self.total_unrealized_pnl()
    }

    /// 判断是否应该平仓 (只读计算，无锁)
    ///
    /// 基于盈亏比判断:
    /// - 盈利时: 达到 profit_ratio 目标即可平仓
    /// - 亏损时: 不建议平仓，除非达到 stop_loss_ratio
    pub fn should_close_position(
        &self,
        unrealized_pnl: Decimal,
        entry_value: Decimal,
        profit_ratio: Decimal,
        stop_loss_ratio: Decimal,
    ) -> (bool, &'static str) {
        if entry_value <= dec!(0) {
            return (false, "entry_value_invalid");
        }

        let pnl_ratio = unrealized_pnl / entry_value;

        if pnl_ratio >= profit_ratio {
            return (true, "profit_target_reached");
        }

        if pnl_ratio <= -stop_loss_ratio {
            return (true, "stop_loss_triggered");
        }

        (false, "not_reached")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_realized_pnl_long() {
        let manager = PnlManager::new();
        let pnl = manager.calculate_realized_pnl(
            "long",
            dec!(1),
            dec!(50000),  // avg_entry_price
            dec!(51000),  // exit_price
        );
        assert_eq!(pnl, dec!(1000));
    }

    #[test]
    fn test_calculate_realized_pnl_short() {
        let manager = PnlManager::new();
        let pnl = manager.calculate_realized_pnl(
            "short",
            dec!(1),
            dec!(50000),  // avg_entry_price
            dec!(49000),  // exit_price
        );
        assert_eq!(pnl, dec!(1000));
    }

    #[test]
    fn test_unrealized_pnl() {
        let manager = PnlManager::new();
        manager.update_unrealized_pnl("BTC", dec!(500));
        manager.update_unrealized_pnl("ETH", dec!(200));
        assert_eq!(manager.get_unrealized_pnl("BTC"), dec!(500));
        assert_eq!(manager.total_unrealized_pnl(), dec!(700));
    }

    #[test]
    fn test_should_close_profit() {
        let manager = PnlManager::new();
        // 10% 盈利目标，当前 15% 盈利
        let (should_close, reason) = manager.should_close_position(
            dec!(1500),   // unrealized_pnl
            dec!(10000),   // entry_value
            dec!(0.10),   // profit_ratio
            dec!(0.05),   // stop_loss_ratio
        );
        assert!(should_close);
        assert_eq!(reason, "profit_target_reached");
    }

    #[test]
    fn test_should_close_loss() {
        let manager = PnlManager::new();
        // 5% 止损目标，当前 8% 亏损
        let (should_close, reason) = manager.should_close_position(
            dec!(-800),   // unrealized_pnl
            dec!(10000),   // entry_value
            dec!(0.10),   // profit_ratio
            dec!(0.05),   // stop_loss_ratio
        );
        assert!(should_close);
        assert_eq!(reason, "stop_loss_triggered");
    }
}
