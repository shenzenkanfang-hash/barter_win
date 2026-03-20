use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

/// 交易对规则 - 承载交易对所有规则
///
/// 包含：价格/数量精度、手续费、杠杆、下单限制等核心规则
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolRules {
    /// 交易对名称
    pub symbol: String,
    /// 价格精度
    pub price_precision: u8,
    /// 数量精度
    pub quantity_precision: u8,
    /// 最小价格变动
    pub tick_size: Decimal,
    /// 交易所原始最小数量（底层数据，不直接使用）
    pub min_qty: Decimal,
    /// 最小名义价值
    pub min_notional: Decimal,
    /// 最大名义价值
    pub max_notional: Decimal,
    /// 杠杆倍数
    pub leverage: u32,
    /// 挂单手续费率
    pub maker_fee: Decimal,
    /// 吃单手续费率
    pub taker_fee: Decimal,
    /// 平仓最小盈亏比阈值
    pub close_min_ratio: Decimal,
    /// 下单最小名义价值阈值（略高于交易所限制）
    pub min_value_threshold: Decimal,
    /// 规则最后更新时间戳
    pub update_ts: i64,
}

impl SymbolRules {
    /// 创建 SymbolRules
    pub fn new(symbol: String) -> Self {
        Self {
            symbol,
            price_precision: 2,
            quantity_precision: 6,
            tick_size: dec!(0.01),
            min_qty: dec!(0.000001),
            min_notional: dec!(5.0),
            max_notional: dec!(1000000.0),
            leverage: 10,
            maker_fee: dec!(0.0002),
            taker_fee: dec!(0.0005),
            close_min_ratio: dec!(0.01),
            min_value_threshold: dec!(10.0),
            update_ts: 0,
        }
    }

    /// 实际有效最小开仓数量（自动计算）
    ///
    /// 计算逻辑：
    /// 1. 理论最小数量 = max(min_notional / tick_size, min_qty)
    /// 2. 按数量精度取整
    /// 3. 校验取整后仍满足最小名义价值
    pub fn effective_min_qty(&self) -> Decimal {
        // 第一步：基于最小名义价值计算理论最小数量
        let theoretical_min_qty = if self.tick_size > dec!(0) {
            self.min_notional / self.tick_size
        } else {
            self.min_notional
        };

        // 第二步：确保不低于交易所原始最小数量
        let base_min_qty = theoretical_min_qty.max(self.min_qty);

        // 第三步：按数量精度取整（四舍五入）
        let step_size = self.step_size();
        let rounded_min_qty = (base_min_qty / step_size)
            .round()
            * step_size;

        // 第四步：最终校验，确保取整后仍满足最小名义价值
        let mut result = rounded_min_qty;
        while result * self.tick_size < self.min_notional {
            result = result + step_size;
        }

        result
    }

    /// 数量最小步进
    pub fn step_size(&self) -> Decimal {
        dec!(1) / dec!(10).powd(dec!(self.quantity_precision))
    }

    /// 价格取整
    ///
    /// 按 tick_size 步进取整，按 price_precision 精度取舍
    pub fn round_price(&self, price: Decimal) -> Decimal {
        if price < dec!(0) {
            return dec!(0);
        }
        if self.tick_size > dec!(0) {
            let rounded = (price / self.tick_size).round() * self.tick_size;
            rounded.round_dp(self.price_precision as u32)
        } else {
            price.round_dp(self.price_precision as u32)
        }
    }

    /// 数量取整
    ///
    /// 使用 effective_min_qty 确保不低于最低数量
    pub fn round_qty(&self, qty: Decimal) -> Decimal {
        if qty < dec!(0) {
            return dec!(0);
        }
        let valid_qty = qty.max(self.effective_min_qty());
        valid_qty.round_dp(self.quantity_precision as u32)
    }

    /// 订单校验
    ///
    /// 检查数量 >= effective_min_qty 且名义价值 >= min_value_threshold
    pub fn validate_order(&self, price: Decimal, qty: Decimal) -> bool {
        if price < dec!(0) || qty < dec!(0) {
            return false;
        }
        let order_notional = price * qty;
        qty >= self.effective_min_qty() && order_notional >= self.min_value_threshold
    }

    /// 基于名义价值计算合规开仓数量
    ///
    /// 核心逻辑：
    /// 1. 使用 Decimal 保证高精度计算，避免浮点数误差
    /// 2. 确保数量 >= effective_min_qty
    /// 3. 按数量精度/步进取整，最终验证名义价值达标
    pub fn calculate_open_qty(&self, open_notional: Decimal, open_price: Decimal) -> Decimal {
        if open_notional <= dec!(0) || open_price <= dec!(0) {
            return self.effective_min_qty();
        }

        // 基础数量计算：数量 = 名义价值 / 价格
        let base_qty = open_notional / open_price;

        // 确保数量不低于有效最小数量
        let valid_qty = base_qty.max(self.effective_min_qty());

        // 按数量精度取整（四舍五入）
        let step_size = self.step_size();
        let rounded_qty = (valid_qty / step_size).round() * step_size;

        // 最终校验：确保取整后的数量仍满足最小名义价值要求
        let mut result = rounded_qty;
        while result * open_price < self.min_value_threshold {
            result = result + step_size;
        }

        result.round_dp(self.quantity_precision as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_rules_basic() {
        let rules = SymbolRules::new("BTCUSDT".to_string());
        assert_eq!(rules.symbol, "BTCUSDT");
        assert_eq!(rules.price_precision, 2);
        assert_eq!(rules.quantity_precision, 6);
    }

    #[test]
    fn test_effective_min_qty() {
        let rules = SymbolRules::new("BTCUSDT".to_string());
        let min_qty = rules.effective_min_qty();
        assert!(min_qty > dec!(0));
    }

    #[test]
    fn test_round_price() {
        let rules = SymbolRules::new("BTCUSDT".to_string());
        let price = rules.round_price(dec!(68000.123456));
        assert_eq!(price, dec!(68000.12));
    }

    #[test]
    fn test_round_qty() {
        let rules = SymbolRules::new("BTCUSDT".to_string());
        let qty = rules.round_qty(dec!(0.000123456));
        assert!(qty >= rules.effective_min_qty());
    }

    #[test]
    fn test_validate_order() {
        let rules = SymbolRules::new("BTCUSDT".to_string());
        assert!(rules.validate_order(dec!(50000), dec!(0.001)));
        assert!(!rules.validate_order(dec!(50000), dec!(0.0000001)));
    }

    #[test]
    fn test_calculate_open_qty() {
        let rules = SymbolRules::new("BTCUSDT".to_string());
        let qty = rules.calculate_open_qty(dec!(10.0), dec!(50000.0));
        assert!(qty > dec!(0));
        assert!(rules.validate_order(dec!(50000.0), qty));
    }
}
