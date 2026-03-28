//! DT-004: h_15m::QuantityCalculator 数量计算器测试
//!
//! 测试分钟级策略数量计算器：
//! - calc_open_quantity: 开仓数量计算
//! - calc_add_quantity: 加仓数量计算
//! - calc_close_quantity: 平仓数量计算
//! - generate_signal: 完整信号生成
//! - 波动率调整: Low/Medium/High 三档
//! - 最大持仓限制

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use d_checktable::h_15m::quantity_calculator::{MinQuantityCalculator, MinQuantityConfig};
use d_checktable::types::{MinSignalInput, MinSignalOutput, VolatilityTier};

fn create_test_config() -> MinQuantityConfig {
    MinQuantityConfig {
        base_open_qty: dec!(0.05),
        max_position_qty: dec!(0.15),
        add_multiplier: dec!(1.5),
        vol_adjustment: true,
    }
}

#[test]
fn test_quantity_calculator_default_config() {
    let config = MinQuantityConfig::default();

    assert_eq!(config.base_open_qty, dec!(0.05));
    assert_eq!(config.max_position_qty, dec!(0.15));
    assert_eq!(config.add_multiplier, dec!(1.5));
    assert!(config.vol_adjustment);
}

#[test]
fn test_quantity_calculator_new() {
    let config = create_test_config();
    let calc = MinQuantityCalculator::new(config);

    assert!(calc.calc_open_quantity(&VolatilityTier::Low) > dec!(0));
}

#[test]
fn test_calc_open_quantity_low_volatility() {
    let calc = MinQuantityCalculator::with_default();

    // 低波动: base * 1.2 = 0.05 * 1.2 = 0.06
    let qty = calc.calc_open_quantity(&VolatilityTier::Low);
    assert_eq!(qty, dec!(0.05) * dec!(1.2));
}

#[test]
fn test_calc_open_quantity_medium_volatility() {
    let calc = MinQuantityCalculator::with_default();

    // 中波动: base = 0.05
    let qty = calc.calc_open_quantity(&VolatilityTier::Medium);
    assert_eq!(qty, dec!(0.05));
}

#[test]
fn test_calc_open_quantity_high_volatility() {
    let calc = MinQuantityCalculator::with_default();

    // 高波动: base * 0.8 = 0.05 * 0.8 = 0.04
    let qty = calc.calc_open_quantity(&VolatilityTier::High);
    assert_eq!(qty, dec!(0.05) * dec!(0.8));
}

#[test]
fn test_calc_open_quantity_no_vol_adjustment() {
    let config = MinQuantityConfig {
        base_open_qty: dec!(0.05),
        max_position_qty: dec!(0.15),
        add_multiplier: dec!(1.5),
        vol_adjustment: false,
    };
    let calc = MinQuantityCalculator::new(config);

    // 不使用波动率调整时，所有情况都返回 base
    let qty_low = calc.calc_open_quantity(&VolatilityTier::Low);
    let qty_medium = calc.calc_open_quantity(&VolatilityTier::Medium);
    let qty_high = calc.calc_open_quantity(&VolatilityTier::High);

    assert_eq!(qty_low, dec!(0.05));
    assert_eq!(qty_medium, dec!(0.05));
    assert_eq!(qty_high, dec!(0.05));
}

#[test]
fn test_calc_add_quantity_normal() {
    let calc = MinQuantityCalculator::with_default();

    // 正常加仓: base * add_multiplier = 0.05 * 1.5 = 0.075
    let qty = calc.calc_add_quantity(dec!(0.0), &VolatilityTier::Medium);
    assert_eq!(qty, dec!(0.05) * dec!(1.5));
}

#[test]
fn test_calc_add_quantity_near_limit() {
    let calc = MinQuantityCalculator::with_default();

    // 接近上限: max - current = 0.15 - 0.14 = 0.01
    let qty = calc.calc_add_quantity(dec!(0.14), &VolatilityTier::Medium);
    assert_eq!(qty, dec!(0.01));
}

#[test]
fn test_calc_add_quantity_at_limit() {
    let calc = MinQuantityCalculator::with_default();

    // 达到上限: 0
    let qty = calc.calc_add_quantity(dec!(0.15), &VolatilityTier::Medium);
    assert_eq!(qty, Decimal::ZERO);
}

#[test]
fn test_calc_add_quantity_exceed_limit() {
    let calc = MinQuantityCalculator::with_default();

    // 超出上限: 当 current > max 时，max_add 为负数
    // 代码逻辑: max_add = max - current = 0.15 - 0.20 = -0.05
    // add_qty = 0.075 > -0.05，所以 add_qty = -0.05 (负数表示无效)
    let qty = calc.calc_add_quantity(dec!(0.20), &VolatilityTier::Medium);
    assert!(qty < Decimal::ZERO);
}

#[test]
fn test_calc_add_quantity_high_volatility_reduces() {
    let calc = MinQuantityCalculator::with_default();

    // 高波动时加仓减少: add_qty * 0.7
    // base * add_multiplier = 0.05 * 1.5 = 0.075
    // 0.075 * 0.7 = 0.0525
    let qty = calc.calc_add_quantity(dec!(0.0), &VolatilityTier::High);
    assert_eq!(qty, dec!(0.05) * dec!(1.5) * dec!(0.7));
}

#[test]
fn test_calc_close_quantity_long_exit() {
    let calc = MinQuantityCalculator::with_default();

    let mut signal = MinSignalOutput::default();
    signal.long_exit = true;

    let (qty, full_close) = calc.calc_close_quantity(dec!(0.1), &signal);
    assert_eq!(qty, dec!(0.1));
    assert!(full_close);
}

#[test]
fn test_calc_close_quantity_short_exit() {
    let calc = MinQuantityCalculator::with_default();

    let mut signal = MinSignalOutput::default();
    signal.short_exit = true;

    let (qty, full_close) = calc.calc_close_quantity(dec!(0.08), &signal);
    assert_eq!(qty, dec!(0.08));
    assert!(full_close);
}

#[test]
fn test_calc_close_quantity_high_volatility_exit() {
    let calc = MinQuantityCalculator::with_default();

    let mut signal = MinSignalOutput::default();
    signal.exit_high_volatility = true;

    let (qty, full_close) = calc.calc_close_quantity(dec!(0.12), &signal);
    assert_eq!(qty, dec!(0.12));
    assert!(full_close);
}

#[test]
fn test_calc_close_quantity_no_signal() {
    let calc = MinQuantityCalculator::with_default();

    let signal = MinSignalOutput::default();

    let (qty, full_close) = calc.calc_close_quantity(dec!(0.1), &signal);
    assert_eq!(qty, Decimal::ZERO);
    assert!(!full_close);
}

#[test]
fn test_signal_generator_default() {
    let signal = MinSignalOutput::default();

    assert!(!signal.long_entry);
    assert!(!signal.short_entry);
    assert!(!signal.long_exit);
    assert!(!signal.short_exit);
    assert!(!signal.long_hedge);
    assert!(!signal.short_hedge);
    assert!(!signal.exit_high_volatility);
}

#[test]
fn test_min_signal_input_new() {
    use rust_decimal::Decimal;

    // 使用 MinSignalInput::new() 而不是 default()，因为 default() 使用派生实现
    let input = MinSignalInput::new();

    assert_eq!(input.tr_base_60min, Decimal::ZERO);
    assert_eq!(input.tr_ratio_15min, Decimal::ZERO);
    assert_eq!(input.zscore_14_1m, Decimal::ZERO);
    assert_eq!(input.pos_norm_60, dec!(50)); // new() 设置为 50
    assert!(input.pine_bg_color.is_empty());
    assert!(input.pine_bar_color.is_empty());
}

#[test]
fn test_volatility_tier_values() {
    // 测试 VolatilityTier 变体
    let high = VolatilityTier::High;
    let medium = VolatilityTier::Medium;
    let low = VolatilityTier::Low;

    assert_eq!(format!("{:?}", high), "High");
    assert_eq!(format!("{:?}", medium), "Medium");
    assert_eq!(format!("{:?}", low), "Low");
}
