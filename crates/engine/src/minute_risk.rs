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
}

/// 计算分钟级单品种开仓名义价值
///
/// 计算逻辑 (对应 Python risk_engine.py):
/// ```python
/// minute_margin = self.calculate_strategy_margin("MINUTE")
/// total_new_open_margin = minute_margin.new_open_max
/// total_available_notional = total_new_open_margin * leverage
/// actual_notional = min(total_available_notional / symbol_count, MAX_SINGLE_NOTIONAL)
/// ```
///
/// # 参数
/// - `account_pool`: 账户保证金池
/// - `margin_config`: 保证金池配置
/// - `symbol_count`: 当前交易品种数量
/// - `leverage`: 杠杆倍数 (默认 1)
///
/// # 返回
/// - `MinuteOpenResult`: 包含各项计算结果的结构体
pub fn calculate_minute_open_notional(
    account_pool: &AccountPool,
    symbol_count: Decimal,
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

    // 目标品种数量 (转换为 Decimal)
    let target_symbol_count = Decimal::from(margin_config.minute_open.target_symbol_count);

    // 单品种名义价值上限
    let max_single_notional = MAX_SINGLE_NOTIONAL;

    // 考虑杠杆后的可用名义价值
    let total_available_notional = total_new_open_margin * leverage;

    // 实际每品种名义价值 = min(总可用名义价值 / 品种数量, 最大单品种限制)
    let actual_notional_per_symbol = if symbol_count > dec!(0) {
        (total_available_notional / symbol_count).min(max_single_notional)
    } else {
        dec!(0)
    };

    // 是否满足最小名义价值阈值
    let min_threshold = margin_config.minute_open.threshold_amount;
    let meets_min_threshold = actual_notional_per_symbol >= min_threshold;

    MinuteOpenResult {
        effective_margin,
        total_available_margin,
        total_new_open_margin,
        target_symbol_count,
        max_single_notional,
        actual_notional_per_symbol,
        meets_min_threshold,
    }
}

/// 计算小时级单品种开仓名义价值
///
/// 计算逻辑与分钟级类似，但使用小时级配置
pub fn calculate_hour_open_notional(
    account_pool: &AccountPool,
    symbol_count: Decimal,
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

    // 目标品种数量
    let target_symbol_count = Decimal::from(margin_config.hour_open.target_symbol_count);

    // 单品种名义价值上限
    let max_single_notional = MAX_SINGLE_NOTIONAL;

    // 考虑杠杆后的可用名义价值
    let total_available_notional = total_new_open_margin * leverage;

    // 实际每品种名义价值
    let actual_notional_per_symbol = if symbol_count > dec!(0) {
        (total_available_notional / symbol_count).min(max_single_notional)
    } else {
        dec!(0)
    };

    // 是否满足最小名义价值阈值
    let min_threshold = margin_config.hour_open.threshold_notional;
    let meets_min_threshold = actual_notional_per_symbol >= min_threshold;

    MinuteOpenResult {
        effective_margin,
        total_available_margin,
        total_new_open_margin,
        target_symbol_count,
        max_single_notional,
        actual_notional_per_symbol,
        meets_min_threshold,
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

        // 模拟 10 个交易品种，1x 杠杆
        let result = calculate_minute_open_notional(
            &pool,
            dec!(10),
            dec!(1),
        );

        // 有效保证金应该是 100000
        assert_eq!(result.effective_margin, dec!(100000.0));

        // 可用保证金 = 100000 * 0.8 = 80000
        assert_eq!(result.total_available_margin, dec!(80000.0));

        // 分钟级新开仓限额 = 80000 * 0.15 = 12000
        assert_eq!(result.total_new_open_margin, dec!(12000.0));

        // 实际每品种名义价值 = min(12000 / 10, 5000) = min(1200, 5000) = 1200
        assert_eq!(result.actual_notional_per_symbol, dec!(1200.0));

        // 满足最小阈值 (1200 >= 250)
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
            dec!(1),
        );

        // 有效保证金 = 90000 (未实现盈亏为0时 total_equity = 90000)
        // 实际上 cumulative_profit = -10000, total_equity = 90000
        assert_eq!(result.effective_margin, dec!(90000.0));

        // 可用保证金 = 90000 * 0.8 = 72000
        assert_eq!(result.total_available_margin, dec!(72000.0));

        // 分钟级新开仓限额 = 72000 * 0.15 = 10800
        assert_eq!(result.total_new_open_margin, dec!(10800.0));

        // 实际每品种名义价值 = min(10800 / 10, 5000) = 1080
        assert_eq!(result.actual_notional_per_symbol, dec!(1080.0));
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
    fn test_max_single_notional_limit() {
        let pool = AccountPool::with_config(
            dec!(100000.0),
            dec!(0.20),
            dec!(0.10),
        );

        // 只有 2 个品种，应该用 total_new_open_margin / 2 但不超过 5000
        let result = calculate_minute_open_notional(
            &pool,
            dec!(2),
            dec!(1),
        );

        // total_new_open_margin = 80000 * 0.15 = 12000
        // 12000 / 2 = 6000，但 max_single_notional = 5000
        // 所以实际应该是 5000
        assert_eq!(result.actual_notional_per_symbol, dec!(5000.0));
    }
}
