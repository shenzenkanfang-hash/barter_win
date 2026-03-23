use parking_lot::RwLock;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::{HashMap, HashSet};

/// 盈亏覆盖检查结果
#[derive(Debug, Clone)]
pub struct PnlCoverageResult {
    /// 是否可以覆盖
    pub can_cover: bool,
    /// 总浮亏
    pub total_loss: Decimal,
    /// 当前盈利
    pub current_profit: Decimal,
    /// 累计盈利
    pub accumulated_profit: Decimal,
    /// 净盈利
    pub net_profit: Decimal,
    /// 低波动品种数
    pub symbol_count: u32,
}

/// 解救结果
#[derive(Debug, Clone)]
pub struct RescueResult {
    /// 是否成功
    pub success: bool,
    /// 是否可以解救
    pub can_rescue: bool,
    /// 总浮亏
    pub total_loss: Decimal,
    /// 可用盈利总额
    pub total_available_profit: Decimal,
    /// 被解救的品种列表
    pub rescued_symbols: Vec<String>,
    /// 剩余盈利
    pub remaining_profit: Decimal,
}

/// 盈亏管理器
///
/// 负责计算和管理已实现盈亏、未实现盈亏，以及累计盈利。
/// 支持低波动/高波动品种互斥机制和解救机制。
///
/// 线程安全: 使用 RwLock 保护所有字段
///
/// 设计依据: 设计文档 17.3.8
///
/// 注: 不实现 Clone/Serialize/Deserialize，因为 RwLock 不支持这些 trait
pub struct PnlManager {
    /// 累计盈利 (浮盈) (RwLock 保护)
    cumulative_profit: RwLock<Decimal>,
    /// 已结算盈利 (实盈) (RwLock 保护)
    realized_profit: RwLock<Decimal>,
    /// 未实现盈亏映射 (使用 RwLock 保护)
    unrealized_pnl: RwLock<HashMap<String, Decimal>>,
    /// 低波动品种集合 (HashSet: O(1) 查找)
    low_volatility_symbols: RwLock<HashSet<String>>,
    /// 高波动品种集合 (HashSet: O(1) 查找)
    high_volatility_symbols: RwLock<HashSet<String>>,
    /// 最后更新时间戳 (RwLock 保护)
    last_update_ts: RwLock<i64>,
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
            cumulative_profit: RwLock::new(dec!(0)),
            realized_profit: RwLock::new(dec!(0)),
            unrealized_pnl: RwLock::new(HashMap::new()),
            low_volatility_symbols: RwLock::new(HashSet::new()),
            high_volatility_symbols: RwLock::new(HashSet::new()),
            last_update_ts: RwLock::new(0),
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
        let mut guard = self.cumulative_profit.write();
        *guard += pnl;
    }

    /// 获取累计盈利
    pub fn get_cumulative_profit(&self) -> Decimal {
        *self.cumulative_profit.read()
    }

    /// 获取已实现盈利
    pub fn get_realized_profit(&self) -> Decimal {
        *self.realized_profit.read()
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
        let low_vol_symbols: HashSet<String> = self.low_volatility_symbols.read().clone();
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

    /// 结算已实现盈亏 (写锁)
    ///
    /// 将已实现盈亏加入实盈，累计盈利相应调整。
    pub fn settle_realized_pnl(&self, pnl: Decimal) {
        let mut realized = self.realized_profit.write();
        *realized += pnl;
        let mut cumulative = self.cumulative_profit.write();
        *cumulative += pnl;
    }

    /// 设置更新时间戳 (写锁)
    pub fn set_last_update_ts(&self, ts: i64) {
        *self.last_update_ts.write() = ts;
    }

    /// 获取更新时间戳 (读锁)
    pub fn get_last_update_ts(&self) -> i64 {
        *self.last_update_ts.read()
    }

    /// 重置管理器 (写锁)
    pub fn reset(&self) {
        self.unrealized_pnl.write().clear();
        self.low_volatility_symbols.write().clear();
        self.high_volatility_symbols.write().clear();
        *self.cumulative_profit.write() = dec!(0);
        *self.realized_profit.write() = dec!(0);
        *self.last_update_ts.write() = 0;
    }

    /// 获取总盈亏 (已实现 + 未实现)
    pub fn total_pnl(&self) -> Decimal {
        *self.realized_profit.read() + self.total_unrealized_pnl()
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

    /// 计算低波动品种总浮亏
    pub fn calculate_low_volatility_total_loss(&self) -> Decimal {
        let low_vol = self.low_volatility_symbols.read();
        low_vol
            .iter()
            .map(|s| (-self.get_unrealized_pnl(s)).max(dec!(0)))
            .sum()
    }

    /// 检查盈亏覆盖
    pub fn check_pnl_coverage(
        &self,
        high_vol_profit: Decimal,
        is_realized: bool,
    ) -> PnlCoverageResult {
        let accumulated = self.get_cumulative_profit();
        let total_available = if is_realized {
            high_vol_profit + accumulated
        } else {
            accumulated
        };

        let total_loss = self.calculate_low_volatility_total_loss();
        let can_cover = total_available >= total_loss;
        let net_profit = if can_cover { total_available - total_loss } else { dec!(0) };

        PnlCoverageResult {
            can_cover,
            total_loss,
            current_profit: high_vol_profit,
            accumulated_profit: accumulated,
            net_profit,
            symbol_count: self.low_volatility_symbols.read().len() as u32,
        }
    }

    /// 解救低波动品种
    pub fn rescue_low_volatility(&mut self, high_vol_profit: Decimal) -> RescueResult {
        let coverage = self.check_pnl_coverage(high_vol_profit, true);

        if !coverage.can_cover {
            return RescueResult {
                success: false,
                can_rescue: false,
                total_loss: dec!(0),
                total_available_profit: dec!(0),
                rescued_symbols: vec![],
                remaining_profit: dec!(0),
            };
        }

        let rescued_symbols: Vec<String> = self.low_volatility_symbols.read().iter().cloned().collect();

        // 清空低波动品种
        {
            let mut low_vol = self.low_volatility_symbols.write();
            low_vol.clear();
        }
        {
            let mut unrealized = self.unrealized_pnl.write();
            for sym in &rescued_symbols {
                unrealized.remove(sym);
            }
        }

        // 更新累计盈利
        *self.cumulative_profit.write() = coverage.net_profit;

        RescueResult {
            success: true,
            can_rescue: true,
            total_loss: coverage.total_loss,
            total_available_profit: coverage.current_profit + coverage.accumulated_profit,
            rescued_symbols,
            remaining_profit: coverage.net_profit,
        }
    }

    /// 获取低波动品种数
    pub fn get_low_vol_count(&self) -> u32 {
        self.low_volatility_symbols.read().len() as u32
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

    #[test]
    fn test_rescue_mechanism() {
        let mut manager = PnlManager::new();

        // 添加低波动品种和浮亏
        manager.add_low_volatility_symbol("BTC".to_string());
        manager.update_unrealized_pnl("BTC", dec!(-500)); // 浮亏 500

        // 无盈利，无法解救
        let result = manager.rescue_low_volatility(dec!(0));
        assert!(!result.can_rescue);

        // 有盈利但不足
        manager.update_cumulative_profit(dec!(300)); // 只有 300
        let result = manager.rescue_low_volatility(dec!(0));
        assert!(!result.can_rescue);

        // 盈利足够，解救
        manager.update_cumulative_profit(dec!(300)); // 现在累计 600
        let result = manager.rescue_low_volatility(dec!(0));
        assert!(result.can_rescue);
        assert_eq!(result.rescued_symbols, vec!["BTC"]);
    }
}
