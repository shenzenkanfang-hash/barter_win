use parking_lot::RwLock;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

/// 持仓方向
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    /// 多头
    Long,
    /// 空头
    Short,
}

impl Default for Direction {
    fn default() -> Self {
        Direction::Long
    }
}

/// 本地持仓信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalPosition {
    /// 方向
    pub direction: Direction,
    /// 数量
    pub qty: Decimal,
    /// 均价
    pub avg_price: Decimal,
    /// 开仓时间戳
    pub open_time: i64,
    /// 持仓费用 (开仓手续费 + 资金费率)
    pub position_cost: Decimal,
}

impl Default for LocalPosition {
    fn default() -> Self {
        Self {
            direction: Direction::Long,
            qty: dec!(0),
            avg_price: dec!(0),
            open_time: 0,
            position_cost: dec!(0),
        }
    }
}

/// 本地持仓管理器
///
/// 负责管理单个品种的持仓信息。
///
/// 线程安全: 使用 RwLock 保护 position
///
/// 设计依据: 设计文档 14.6 持仓/资金更新层
pub struct LocalPositionManager {
    /// 当前持仓 (使用 RwLock 保护)
    position: RwLock<LocalPosition>,
    /// 持仓统计
    stats: RwLock<PositionStats>,
}

impl Default for LocalPositionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalPositionManager {
    /// 创建新的持仓管理器
    pub fn new() -> Self {
        Self {
            position: RwLock::new(LocalPosition::default()),
            stats: RwLock::new(PositionStats::default()),
        }
    }

    /// 开仓 (增加持仓) (写锁)
    ///
    /// qty: 新开数量
    /// price: 开仓价格
    /// timestamp: 开仓时间戳
    pub fn open_position(
        &self,
        direction: Direction,
        qty: Decimal,
        price: Decimal,
        timestamp: i64,
    ) {
        let mut pos = self.position.write();

        if qty <= dec!(0) || price <= dec!(0) {
            return;
        }

        if pos.qty <= dec!(0) {
            // 无持仓，直接开仓
            pos.direction = direction;
            pos.qty = qty;
            pos.avg_price = price;
            pos.open_time = timestamp;
        } else if pos.direction == direction {
            // 同方向加仓
            let total_value = pos.qty * pos.avg_price + qty * price;
            let total_qty = pos.qty + qty;
            pos.avg_price = total_value / total_qty;
            pos.qty = total_qty;
        } else {
            // 反方向，先平后开
            if qty >= pos.qty {
                // 平完再开反向仓
                let remaining_qty = qty - pos.qty;
                pos.qty = remaining_qty;
                pos.direction = direction;
                pos.avg_price = price;
                pos.open_time = timestamp;
            } else {
                // 部分平仓
                pos.qty = pos.qty - qty;
            }
        }

        drop(pos);
        self.stats.write().update_on_trade();
    }

    /// 平仓 (减少持仓) (写锁)
    ///
    /// qty: 平仓数量
    /// price: 平仓价格
    /// 返回: 已实现盈亏
    pub fn close_position(&self, qty: Decimal, price: Decimal) -> Decimal {
        let mut pos = self.position.write();

        if pos.qty <= dec!(0) || qty <= dec!(0) {
            return dec!(0);
        }

        // 计算已实现盈亏
        let pnl = match pos.direction {
            Direction::Long => (price - pos.avg_price) * qty,
            Direction::Short => (pos.avg_price - price) * qty,
        };

        // 更新持仓
        if qty >= pos.qty {
            pos.qty = dec!(0);
            pos.avg_price = dec!(0);
        } else {
            pos.qty = pos.qty - qty;
        }

        drop(pos);
        let mut stats = self.stats.write();
        stats.update_on_trade();
        stats.add_realized_pnl(pnl);

        pnl
    }

    /// 全平 (平掉所有持仓) (写锁)
    ///
    /// price: 平仓价格
    /// 返回: 已实现盈亏
    pub fn close_all(&self, price: Decimal) -> Decimal {
        let qty = self.position.read().qty;
        self.close_position(qty, price)
    }

    /// 更新持仓 (成交回报后调用) (写锁)
    ///
    /// 持仓变动时调用，更新本地持仓状态。
    pub fn update_on_fill(
        &self,
        direction: Direction,
        qty: Decimal,
        price: Decimal,
        timestamp: i64,
    ) {
        // 如果有反向持仓，先平后开
        let should_close_first = {
            let pos = self.position.read();
            pos.qty > dec!(0) && pos.direction != direction
        };

        if should_close_first {
            let close_qty = {
                let pos = self.position.read();
                qty.min(pos.qty)
            };
            if close_qty > dec!(0) {
                self.close_position(close_qty, price);
            }
            let remaining_qty = qty - close_qty;
            if remaining_qty > dec!(0) {
                self.open_position(direction, remaining_qty, price, timestamp);
            }
        } else {
            self.open_position(direction, qty, price, timestamp);
        }
    }

    /// 获取当前持仓 (克隆以避免生命周期问题)
    pub fn get_position(&self) -> LocalPosition {
        self.position.read().clone()
    }

    /// 获取持仓数量 (读锁)
    pub fn qty(&self) -> Decimal {
        self.position.read().qty
    }

    /// 获取持仓方向 (读锁)
    pub fn direction(&self) -> Direction {
        self.position.read().direction
    }

    /// 获取持仓均价 (读锁)
    pub fn avg_price(&self) -> Decimal {
        self.position.read().avg_price
    }

    /// 检查是否有持仓 (读锁)
    pub fn has_position(&self) -> bool {
        self.position.read().qty > dec!(0)
    }

    /// 计算未实现盈亏 (读锁)
    ///
    /// current_price: 当前市场价格
    pub fn unrealized_pnl(&self, current_price: Decimal) -> Decimal {
        let pos = self.position.read();
        if pos.qty <= dec!(0) || current_price <= dec!(0) {
            return dec!(0);
        }

        match pos.direction {
            Direction::Long => (current_price - pos.avg_price) * pos.qty,
            Direction::Short => (pos.avg_price - current_price) * pos.qty,
        }
    }

    /// 计算名义价值 (读锁)
    pub fn notional_value(&self, price: Decimal) -> Decimal {
        self.position.read().qty * price
    }

    /// 获取统计信息
    pub fn stats(&self) -> PositionStats {
        self.stats.read().clone()
    }

    /// 重置持仓 (写锁)
    pub fn reset(&self) {
        *self.position.write() = LocalPosition::default();
        *self.stats.write() = PositionStats::default();
    }
}

/// 持仓统计
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PositionStats {
    /// 交易次数
    pub trade_count: u64,
    /// 已实现盈亏总和
    pub total_realized_pnl: Decimal,
    /// 最大持仓数量
    pub max_qty: Decimal,
    /// 持仓时间 (秒)
    pub holding_time_secs: i64,
}

impl PositionStats {
    fn update_on_trade(&mut self) {
        self.trade_count += 1;
    }

    fn add_realized_pnl(&mut self, pnl: Decimal) {
        self.total_realized_pnl += pnl;
    }

    fn update_max_qty(&mut self, qty: Decimal) {
        if qty > self.max_qty {
            self.max_qty = qty;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_long() {
        let mgr = LocalPositionManager::new();
        mgr.open_position(Direction::Long, dec!(1), dec!(50000), 1000);

        assert_eq!(mgr.qty(), dec!(1));
        assert_eq!(mgr.avg_price(), dec!(50000));
        assert_eq!(mgr.direction(), Direction::Long);
    }

    #[test]
    fn test_add_to_position() {
        let mgr = LocalPositionManager::new();
        mgr.open_position(Direction::Long, dec!(1), dec!(50000), 1000);
        mgr.open_position(Direction::Long, dec!(1), dec!(51000), 1001);

        assert_eq!(mgr.qty(), dec!(2));
        //均价应该是 (1*50000 + 1*51000) / 2 = 50500
        assert_eq!(mgr.avg_price(), dec!(50500));
    }

    #[test]
    fn test_close_position() {
        let mgr = LocalPositionManager::new();
        mgr.open_position(Direction::Long, dec!(1), dec!(50000), 1000);

        let pnl = mgr.close_position(dec!(0.5), dec!(51000));
        assert_eq!(pnl, dec!(500)); // (51000 - 50000) * 0.5
        assert_eq!(mgr.qty(), dec!(0.5));
    }

    #[test]
    fn test_unrealized_pnl_long() {
        let mgr = LocalPositionManager::new();
        mgr.open_position(Direction::Long, dec!(1), dec!(50000), 1000);

        let pnl = mgr.unrealized_pnl(dec!(51000));
        assert_eq!(pnl, dec!(1000)); // (51000 - 50000) * 1
    }

    #[test]
    fn test_unrealized_pnl_short() {
        let mgr = LocalPositionManager::new();
        mgr.open_position(Direction::Short, dec!(1), dec!(50000), 1000);

        let pnl = mgr.unrealized_pnl(dec!(49000));
        assert_eq!(pnl, dec!(1000)); // (50000 - 49000) * 1
    }
}
