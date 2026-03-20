use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 订单检查结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderCheckResult {
    /// 是否通过检查
    pub passed: bool,
    /// 冻结金额 (如果通过)
    pub frozen_amount: Decimal,
    /// 拒绝原因
    pub reject_reason: Option<String>,
    /// 检查时间戳
    pub timestamp: i64,
}

/// 订单预占记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderReservation {
    /// 订单ID
    pub order_id: String,
    /// 品种
    pub symbol: String,
    /// 策略ID
    pub strategy_id: String,
    /// 冻结金额
    pub frozen_amount: Decimal,
    /// 创建时间戳
    pub created_at: i64,
    /// 状态: pending, confirmed, cancelled
    pub status: String,
}

/// 订单检查器
///
/// 实现设计文档 17.3.7 描述的订单风控检查器。
/// 支持:
/// - 订单预占 (冻结保证金)
/// - Lua 脚本原子预占 (可选)
/// - 持仓比例检查
/// - 名义价值检查
///
/// 注: Lua 脚本功能需要集成 mlua crate，此处提供基础实现
pub struct OrderCheck {
    /// 最大持仓比例
    max_position_ratio: Decimal,
    /// 最低订单名义价值
    min_order_notional: Decimal,
    /// 预占记录: order_id -> OrderReservation
    reservations: HashMap<String, OrderReservation>,
    /// 总冻结金额
    total_frozen: Decimal,
}

impl Default for OrderCheck {
    fn default() -> Self {
        Self::new()
    }
}

impl OrderCheck {
    /// 创建订单检查器
    pub fn new() -> Self {
        Self {
            max_position_ratio: dec!(0.95),
            min_order_notional: dec!(10.0),
            reservations: HashMap::new(),
            total_frozen: dec!(0),
        }
    }

    /// 预检订单
    ///
    /// 在下单前预检订单的风控条件。
    /// 如果通过，返回冻结金额。
    pub fn pre_check(
        &self,
        order_id: &str,
        symbol: &str,
        strategy_id: &str,
        order_value: Decimal,
        available_balance: Decimal,
        current_exposure: Decimal,
    ) -> OrderCheckResult {
        // 1. 检查名义价值
        if order_value < self.min_order_notional {
            return OrderCheckResult {
                passed: false,
                frozen_amount: dec!(0),
                reject_reason: Some(format!(
                    "订单名义价值 {} 小于最低要求 {}",
                    order_value, self.min_order_notional
                )),
                timestamp: chrono::Utc::now().timestamp(),
            };
        }

        // 2. 检查资金是否足够
        let total_needed = self.total_frozen + order_value;
        if total_needed > available_balance {
            return OrderCheckResult {
                passed: false,
                frozen_amount: dec!(0),
                reject_reason: Some(format!(
                    "可用资金 {} 不足，需要 {} (已冻结 {})",
                    available_balance, total_needed, self.total_frozen
                )),
                timestamp: chrono::Utc::now().timestamp(),
            };
        }

        // 3. 检查持仓比例
        let new_exposure = current_exposure + order_value;
        // 假设总权益 = 可用 + 当前敞口 (简化计算)
        let total_equity = available_balance + current_exposure;
        let new_ratio = new_exposure / total_equity;

        if new_ratio > self.max_position_ratio {
            return OrderCheckResult {
                passed: false,
                frozen_amount: dec!(0),
                reject_reason: Some(format!(
                    "持仓比例 {} 超过最大限制 {}",
                    new_ratio, self.max_position_ratio
                )),
                timestamp: chrono::Utc::now().timestamp(),
            };
        }

        // 通过检查
        OrderCheckResult {
            passed: true,
            frozen_amount: order_value,
            reject_reason: None,
            timestamp: chrono::Utc::now().timestamp(),
        }
    }

    /// 预占订单 (冻结保证金)
    ///
    /// 使用原子操作预占订单的保证金。
    /// 设计文档提到使用 Lua 脚本实现原子预占。
    pub fn reserve(
        &mut self,
        order_id: &str,
        symbol: &str,
        strategy_id: &str,
        frozen_amount: Decimal,
    ) -> Result<(), String> {
        // 检查是否已经预占
        if self.reservations.contains_key(order_id) {
            return Err(format!("订单 {} 已经有预占记录", order_id));
        }

        // 创建预占记录
        let reservation = OrderReservation {
            order_id: order_id.to_string(),
            symbol: symbol.to_string(),
            strategy_id: strategy_id.to_string(),
            frozen_amount,
            created_at: chrono::Utc::now().timestamp(),
            status: "pending".to_string(),
        };

        // 添加预占记录
        self.reservations.insert(order_id.to_string(), reservation);
        self.total_frozen += frozen_amount;

        Ok(())
    }

    /// 确认预占 (订单成交后调用)
    ///
    /// 将预占转为实际占用，从冻结金额中扣除。
    pub fn confirm_reservation(&mut self, order_id: &str) -> Result<Decimal, String> {
        let reservation = self.reservations.remove(order_id)
            .ok_or_else(|| format!("订单 {} 没有预占记录", order_id))?;

        if reservation.status != "pending" {
            return Err(format!("订单 {} 状态不是 pending", order_id));
        }

        self.total_frozen -= reservation.frozen_amount;
        Ok(reservation.frozen_amount)
    }

    /// 取消预占 (订单失败/撤销后调用)
    ///
    /// 释放冻结的保证金。
    pub fn cancel_reservation(&mut self, order_id: &str) -> Result<Decimal, String> {
        let reservation = self.reservations.remove(order_id)
            .ok_or_else(|| format!("订单 {} 没有预占记录", order_id))?;

        if reservation.status != "pending" {
            return Err(format!("订单 {} 状态不是 pending", order_id));
        }

        self.total_frozen -= reservation.frozen_amount;
        Ok(reservation.frozen_amount)
    }

    /// 释放所有预占 (用于系统重置)
    pub fn release_all(&mut self) {
        self.reservations.clear();
        self.total_frozen = dec!(0);
    }

    /// 获取总冻结金额
    pub fn total_frozen(&self) -> Decimal {
        self.total_frozen
    }

    /// 获取预占数量
    pub fn reservation_count(&self) -> usize {
        self.reservations.len()
    }

    /// 获取指定订单的预占记录
    pub fn get_reservation(&self, order_id: &str) -> Option<&OrderReservation> {
        self.reservations.get(order_id)
    }

    /// 检查是否有未处理的预占
    pub fn has_pending_reservations(&self) -> bool {
        !self.reservations.is_empty()
    }

    /// 设置最大持仓比例
    pub fn set_max_position_ratio(&mut self, ratio: Decimal) {
        self.max_position_ratio = ratio;
    }

    /// 设置最低订单名义价值
    pub fn set_min_order_notional(&mut self, notional: Decimal) {
        self.min_order_notional = notional;
    }

    /// 获取待确认的预占列表
    pub fn get_pending_reservations(&self) -> Vec<&OrderReservation> {
        self.reservations
            .values()
            .filter(|r| r.status == "pending")
            .collect()
    }
}

/// Lua 脚本执行器 (可选功能)
///
/// 设计文档提到使用 Lua 脚本实现原子预占。
/// 此处提供接口定义，实际集成需要 mlua crate。
pub struct LuaScriptExecutor;

impl LuaScriptExecutor {
    /// 原子预占 Lua 脚本
    ///
    /// Lua 脚本逻辑:
    /// ```lua
    /// local key = KEYS[1]
    /// local amount = tonumber(ARGV[1])
    /// local available = tonumber(redis.call('GET', key) or '0')
    /// if available >= amount then
    ///     redis.call('DECRBY', key, amount)
    ///     return amount
    /// else
    ///     return -1
    /// end
    /// ```
    pub fn atomic_reserve_script() -> &'static str {
        r#"
local key = KEYS[1]
local amount = tonumber(ARGV[1])
local available = tonumber(redis.call('GET', key) or '0')
if available >= amount then
    redis.call('DECRBY', key, amount)
    return amount
else
    return -1
end
"#
    }

    /// 原子释放 Lua 脚本
    pub fn atomic_release_script() -> &'static str {
        r#"
local key = KEYS[1]
local amount = tonumber(ARGV[1])
redis.call('INCRBY', key, amount)
return amount
"#
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order_pre_check_pass() {
        let checker = OrderCheck::new();
        let result = checker.pre_check(
            "order_1",
            "BTC",
            "trend",
            dec!(1000),   // order_value
            dec!(10000),  // available_balance
            dec!(0),      // current_exposure
        );
        assert!(result.passed);
        assert_eq!(result.frozen_amount, dec!(1000));
    }

    #[test]
    fn test_order_pre_check_insufficient_balance() {
        let checker = OrderCheck::new();
        let result = checker.pre_check(
            "order_1",
            "BTC",
            "trend",
            dec!(1000),
            dec!(500),    // available_balance < order_value
            dec!(0),
        );
        assert!(!result.passed);
        assert!(result.reject_reason.is_some());
    }

    #[test]
    fn test_order_pre_check_min_notional() {
        let checker = OrderCheck::new();
        let result = checker.pre_check(
            "order_1",
            "BTC",
            "trend",
            dec!(5),      // order_value < min_order_notional
            dec!(10000),
            dec!(0),
        );
        assert!(!result.passed);
        assert!(result.reject_reason.unwrap().contains("名义价值"));
    }

    #[test]
    fn test_reserve_and_confirm() {
        let mut checker = OrderCheck::new();
        checker.reserve("order_1", "BTC", "trend", dec!(1000)).unwrap();
        assert_eq!(checker.total_frozen(), dec!(1000));

        let released = checker.confirm_reservation("order_1").unwrap();
        assert_eq!(released, dec!(1000));
        assert_eq!(checker.total_frozen(), dec!(0));
    }

    #[test]
    fn test_reserve_and_cancel() {
        let mut checker = OrderCheck::new();
        checker.reserve("order_1", "BTC", "trend", dec!(1000)).unwrap();
        assert_eq!(checker.total_frozen(), dec!(1000));

        let released = checker.cancel_reservation("order_1").unwrap();
        assert_eq!(released, dec!(1000));
        assert_eq!(checker.total_frozen(), dec!(0));
    }

    #[test]
    fn test_duplicate_reserve() {
        let mut checker = OrderCheck::new();
        checker.reserve("order_1", "BTC", "trend", dec!(1000)).unwrap();
        let result = checker.reserve("order_1", "BTC", "trend", dec!(1000));
        assert!(result.is_err());
    }
}
