//! 分钟级风控计算模块
//!
//! 对应 Python 的 risk_engine.py 中的 get_minute_open_notional 计算逻辑
//!
//! 配置来源: D:\\量化策略开发\\tradingW\\backup_old_code\\d_risk_monitor\\config.py

use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use crate::account_pool::AccountPool;
use crate::margin_config::{MarginPoolConfig, StrategyLevel, MIN_EFFECTIVE_MARGIN, MAX_SINGLE_NOTIONAL};

/// 分钟级开仓结果
#[derive(Debug, Clone)]
pub struct MinuteOpenResult {
    /// 账户有效保证金
    pub effective_margin: Decimal,
    /// 可用保证金
    pub total_available_margin: Decimal,
    /// 总新开仓限额 (保证金)
    pub total_new_open_margin: Decimal,
    /// 目标交易品种数量
    pub target_symbol_count: Decimal,
    /// 单个品种名义价值上限
    pub max_single_notional: Decimal,
    /// 实际每品种开仓名义价值
    pub actual_notional_per_symbol: Decimal,
    /// 是否达到最小开仓阈值
    pub meets_min_threshold: bool,
    /// 剩余可开仓保证金 (仅计算用)
    pub remaining_margin: Decimal,
}

/// 计算分钟级单品种开仓名义价值
///
/// 完全对标 Python risk_engine.py 的 calculate_minute_open_notional 逻辑:
///
/// # 参数
/// - `account_pool`: 账户保证金池
/// - `current_symbol_count`: 当前交易品种数量
/// - `leverage`: 杠杆倍数 (默认 10)
///
/// # 返回
/// - `MinuteOpenResult`: 包含各项计算结果的结构体
pub fn calculate_minute_open_notional(
    account_pool: &AccountPool,
    current_symbol_count: Decimal,
    leverage: Decimal,
) -> MinuteOpenResult {
    let margin_config = MarginPoolConfig::default();

    // 获取账户保证金信息 (分钟级策略)
    let account_margin = account_pool.get_account_margin(StrategyLevel::Minute);

    // 有效保证金
    let effective_margin = account_margin.effective_margin;

    // 可用保证金
    let total_available_margin = account_margin.total_available_margin;

    // 分钟级新开仓保证金上限 = 可用保证金 * 新开仓比例
    let total_new_open_margin = total_available_margin * margin_config.minute.new_open_ratio;

    // 配置参数
    let open_config = &margin_config.minute_open;
    let min_notional = open_config.min_notional_per_symbol;
    let target_count = Decimal::from(open_config.target_symbol_count);
    let threshold = open_config.threshold_amount;
    let max_single_notional = MAX_SINGLE_NOTIONAL;

    // 实际每品种名义价值
    let mut actual_notional_per_symbol: Decimal;
    let mut remaining_margin: Decimal = dec!(0);

    if total_new_open_margin <= dec!(0) {
        // 新开仓保证金为0，返回最低名义价值
        actual_notional_per_symbol = min_notional;
    } else {
        let total_available_notional = total_new_open_margin * leverage;

        if current_symbol_count <= dec!(0) {
            // 当前品种数为0，使用最低开仓名义价值
            actual_notional_per_symbol = min_notional;
        } else {
            // 计算最小总名义价值和所需保证金
            let min_total_notional = current_symbol_count * min_notional;
            let min_total_margin = min_total_notional / leverage;

            if total_new_open_margin <= threshold {
                // 保证金不足阈值，使用最低名义价值
                actual_notional_per_symbol = min_notional;
            } else {
                if current_symbol_count < target_count {
                    // 品种数不足目标，按剩余保证金分配
                    let excess_margin = total_new_open_margin - threshold;
                    let excess_notional = excess_margin * leverage;
                    let per_symbol_excess = excess_notional / current_symbol_count;
                    actual_notional_per_symbol = min_notional + per_symbol_excess;
                } else {
                    // 品种数已达目标，均分总名义价值
                    actual_notional_per_symbol = total_available_notional / target_count;
                }
            }
        }

        // 单品种上限限制: min(actual_notional, MAX_SINGLE_NOTIONAL) 并确保不低于 min_notional
        actual_notional_per_symbol = actual_notional_per_symbol
            .min(max_single_notional)
            .max(min_notional);

        // 计算已用保证金和剩余保证金
        if current_symbol_count > dec!(0) {
            let used_margin = (actual_notional_per_symbol * current_symbol_count) / leverage;
            remaining_margin = (total_new_open_margin - used_margin).max(dec!(0));
        }
    }

    // 是否满足最小名义价值阈值
    let meets_min_threshold = actual_notional_per_symbol >= threshold;

    MinuteOpenResult {
        effective_margin,
        total_available_margin,
        total_new_open_margin,
        target_symbol_count: target_count,
        max_single_notional,
        actual_notional_per_symbol,
        meets_min_threshold,
        remaining_margin,
    }
}

/// 计算小时级单品种开仓名义价值
///
/// 完全对标 Python risk_engine.py 的 calculate_hour_open_notional 逻辑:
///
/// 核心配置:
/// - 目标品种数: 10
/// - 每品种加仓次数: 10
/// - 总份数: 10 × 10 = 100
/// - 最小名义价值: 5 USDT
/// - 阈值: 500 USDT (对应保证金)
pub fn calculate_hour_open_notional(
    account_pool: &AccountPool,
    current_symbol_count: Decimal,
    leverage: Decimal,
) -> MinuteOpenResult {
    let margin_config = MarginPoolConfig::default();

    // 获取账户保证金信息 (小时级策略)
    let account_margin = account_pool.get_account_margin(StrategyLevel::Hour);

    // 有效保证金
    let effective_margin = account_margin.effective_margin;

    // 可用保证金
    let total_available_margin = account_margin.total_available_margin;

    // 小时级新开仓保证金上限 = 可用保证金 * 新开仓比例
    let total_new_open_margin = total_available_margin * margin_config.hour.new_open_ratio;

    // 配置参数
    let open_config = &margin_config.hour_open;
    let min_notional = open_config.min_notional_per_symbol; // 5.0
    let target_count = Decimal::from(open_config.target_symbol_count); // 10
    let add_times = Decimal::from(open_config.add_times); // 10
    let threshold_notional = open_config.threshold_notional; // 500.0
    let max_single_notional = MAX_SINGLE_NOTIONAL;

    // 总目标份数 = 目标品种数 × 加仓次数
    let total_target_shares = target_count * add_times; // 100

    // 阈值保证金 = 阈值名义价值 / 杠杆
    let threshold_margin = threshold_notional / leverage;

    // 实际每品种名义价值
    let mut actual_notional_per_symbol: Decimal;
    let mut remaining_margin: Decimal = dec!(0);

    if total_new_open_margin <= dec!(0) {
        // 新开仓保证金为0，返回最低名义价值
        actual_notional_per_symbol = min_notional;
    } else {
        let total_available_notional = total_new_open_margin * leverage;

        if current_symbol_count <= dec!(0) {
            // 当前品种数为0，使用最低开仓名义价值
            actual_notional_per_symbol = min_notional;
        } else {
            // 基础逻辑：先计算当前品种数的最小总名义价值
            let min_total_notional = current_symbol_count * min_notional;
            let _min_total_margin = min_total_notional / leverage;

            if total_new_open_margin <= threshold_margin {
                // 未达阈值：保持初始5刀/品种
                actual_notional_per_symbol = min_notional;
            } else {
                // 达到阈值：按100份分配，或按当前品种数均分
                if current_symbol_count < target_count {
                    // 当前品种数<10：超额部分按当前品种数均分
                    let excess_margin = total_new_open_margin - threshold_margin;
                    let excess_notional = excess_margin * leverage;
                    let per_symbol_excess = excess_notional / current_symbol_count;
                    actual_notional_per_symbol = min_notional + per_symbol_excess;
                } else {
                    // 当前品种数≥10：按100份均分总名义价值
                    actual_notional_per_symbol = total_available_notional / total_target_shares;
                }
            }
        }

        // 单品种上限限制: min(actual_notional, MAX_SINGLE_NOTIONAL) 并确保不低于 min_notional
        actual_notional_per_symbol = actual_notional_per_symbol
            .min(max_single_notional)
            .max(min_notional);

        // 计算已用保证金和剩余保证金
        if current_symbol_count > dec!(0) {
            let used_margin = (actual_notional_per_symbol * current_symbol_count) / leverage;
            remaining_margin = (total_new_open_margin - used_margin).max(dec!(0));
        }
    }

    // 是否满足最小名义价值阈值
    let meets_min_threshold = actual_notional_per_symbol >= threshold_notional;

    MinuteOpenResult {
        effective_margin,
        total_available_margin,
        total_new_open_margin,
        target_symbol_count: target_count,
        max_single_notional,
        actual_notional_per_symbol,
        meets_min_threshold,
        remaining_margin,
    }
}

/// 根据实际名义价值计算开仓数量
///
/// # 参数
/// - `open_notional`: 实际开仓名义价值 (USD
/// - `open_price`: 开仓价格
/// - `effective_min_qty`: 有效最小交易数量
/// - `step_size`: 步长大小
/// - `quantity_precision`: 数量精度
/// - `min_value_threshold`: 最小价值阈值
///
/// # 返回
/// - 开仓数量
pub fn calculate_open_qty_from_notional(
    open_notional: Decimal,
    open_price: Decimal,
    effective_min_qty: Decimal,
    step_size: Decimal,
    quantity_precision: u32,
    min_value_threshold: Decimal,
) -> Decimal {
    if open_notional <= dec!(0) || open_price <= dec!(0) {
        return effective_min_qty;
    }

    let base_qty = open_notional / open_price;
    let valid_qty = base_qty.max(effective_min_qty);
    let rounded_qty = (valid_qty / step_size).round() * step_size;
    let mut result = rounded_qty;

    // 确保不低于最小价值阈值
    while result * open_price < min_value_threshold {
        result = result + step_size;
    }

    result.round_dp(quantity_precision)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_minute_open_notional_basic() {
        let pool = AccountPool::with_config(
            dec!(100000.0),  // 初始资金 10万
            dec!(0.20),       // 20% 熔断阈值
            dec!(0.10),       // 10% 部分熔断
        );

        // 模拟 10 个交易品种，10x 杠杆
        let result = calculate_minute_open_notional(
            &pool,
            dec!(10),
            dec!(10),
        );

        // 有效保证金应该是 100000
        assert_eq!(result.effective_margin, dec!(100000.0));

        // 可用保证金 = 100000 * 0.8 = 80000
        assert_eq!(result.total_available_margin, dec!(80000.0));

        // 分钟级新开仓限额 = 80000 * 0.15 = 12000
        assert_eq!(result.total_new_open_margin, dec!(12000.0));

        // 目标品种数 = 50
        assert_eq!(result.target_symbol_count, dec!(50));

        // 当前品种数 10 < 目标 50 且保证金 12000 > 阈值 250
        // excess_margin = 12000 - 250 = 11750
        // excess_notional = 11750 * 10 = 117500
        // per_symbol_excess = 117500 / 10 = 11750
        // actual_notional = 5 + 11750 = 11755
        // 但限制在 MAX_SINGLE_NOTIONAL = 5000 以内
        assert_eq!(result.actual_notional_per_symbol, dec!(5000.0));

        // 满足最小阈值
        assert!(result.meets_min_threshold);
    }

    #[test]
    fn test_calculate_minute_open_notional_with_loss() {
        let pool = AccountPool::with_config(
            dec!(100000.0),
            dec!(0.20),
            dec!(0.10),
        );

        // 亏损 10%，剩余 90000
        pool.update_equity(dec!(-10000.0), 1000);

        let result = calculate_minute_open_notional(
            &pool,
            dec!(10),
            dec!(10),
        );

        // 有效保证金 = 90000
        assert_eq!(result.effective_margin, dec!(90000.0));

        // 可用保证金 = 90000 * 0.8 = 72000
        assert_eq!(result.total_available_margin, dec!(72000.0));

        // 分钟级新开仓限额 = 72000 * 0.15 = 10800
        assert_eq!(result.total_new_open_margin, dec!(10800.0));

        // 当前品种数 10 < 目标 50 且保证金 10800 > 阈值 250
        // actual_notional 被限制在 5000
        assert_eq!(result.actual_notional_per_symbol, dec!(5000.0));
    }

    #[test]
    fn test_calculate_open_qty_from_notional() {
        // 测试用例: 1200 USDT 名义价值，价格 50000
        let qty = calculate_open_qty_from_notional(
            dec!(1200.0),    // open_notional
            dec!(50000.0),   // open_price
            dec!(0.001),     // effective_min_qty
            dec!(0.001),     // step_size
            3,               // quantity_precision
            dec!(5.0),       // min_value_threshold
        );

        // 1200 / 50000 = 0.024 -> 0.024 向下取整到 0.001 步长 = 0.024
        assert_eq!(qty, dec!(0.024));
    }

    #[test]
    fn test_minute_zero_symbols() {
        let pool = AccountPool::with_config(
            dec!(100000.0),
            dec!(0.20),
            dec!(0.10),
        );

        // 品种数为 0，应该返回最低名义价值 5
        let result = calculate_minute_open_notional(
            &pool,
            dec!(0),
            dec!(10),
        );

        assert_eq!(result.actual_notional_per_symbol, dec!(5.0));
    }

    #[test]
    fn test_hour_open_notional_basic() {
        let pool = AccountPool::with_config(
            dec!(100000.0),
            dec!(0.20),
            dec!(0.10),
        );

        // 10 个品种，10x 杠杆
        let result = calculate_hour_open_notional(
            &pool,
            dec!(10),
            dec!(10),
        );

        // 有效保证金 = 100000
        assert_eq!(result.effective_margin, dec!(100000.0));

        // 可用保证金 = 100000 * 0.8 = 80000
        assert_eq!(result.total_available_margin, dec!(80000.0));

        // 小时级新开仓限额 = 80000 * 0.3 = 24000
        assert_eq!(result.total_new_open_margin, dec!(24000.0));

        // 目标份数 = 10 * 10 = 100
        // threshold_margin = 500 / 10 = 50
        // total_new_open_margin 24000 > threshold_margin 50
        // current_symbol_count 10 >= target_count 10
        // actual_notional = (24000 * 10) / 100 = 2400
        assert_eq!(result.actual_notional_per_symbol, dec!(2400.0));

        // 满足最小阈值
        assert!(result.meets_min_threshold);
    }
}
