//! indicator.rs - 指标计算 + 信号生成
//!
//! 核心：
//! - 读取市场数据、指标数据
//! - 结合持仓状态生成交易信号

#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use rust_decimal_macros::dec;

/// ==================== 常量 ====================
pub mod config {
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;

    pub const PROFIT_THRESHOLD: Decimal = dec!(0.01);      // 1%盈利平仓
    pub const PRICE_DOWN_THRESHOLD: Decimal = dec!(0.98);   // 2%下跌对冲
    pub const PRICE_UP_THRESHOLD: Decimal = dec!(1.02);     // 2%上涨对冲
    pub const PRICE_DOWN_HARD: Decimal = dec!(0.90);       // 10%下跌
    pub const PRICE_UP_HARD: Decimal = dec!(1.10);         // 10%上涨
    pub const LONG_ADD_THRESHOLD: Decimal = dec!(1.02);     // 多头加仓
    pub const LONG_ADD_HARD: Decimal = dec!(1.08);          // 多头加仓硬阈值
    pub const SHORT_ADD_THRESHOLD: Decimal = dec!(0.98);    // 空头加仓
    pub const SHORT_ADD_HARD: Decimal = dec!(0.92);        // 空头加仓硬阈值
}

/// ==================== 类型 ====================
/// 市场数据
#[derive(Debug, Clone)]
pub struct MarketData {
    pub price: Decimal,
    pub volatility: f64,
    pub tr_ratio: f64,
}

/// 持仓数据
#[derive(Debug, Clone)]
pub struct PositionData {
    pub long_price: Decimal,
    pub long_qty: Decimal,
    pub short_price: Decimal,
    pub short_qty: Decimal,
}

impl Default for PositionData {
    fn default() -> Self {
        Self {
            long_price: Decimal::ZERO,
            long_qty: Decimal::ZERO,
            short_price: Decimal::ZERO,
            short_qty: Decimal::ZERO,
        }
    }
}

/// 市场状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketStatus {
    Pin,
    Range,
    Trend,
}

/// 交易信号
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    None,
    LongOpen,
    ShortOpen,
    LongAdd,
    ShortAdd,
    LongClose,
    ShortClose,
    LongHedge,
    ShortHedge,
}

/// ==================== 信号生成器 ====================
pub struct Indicator {
    market_status: MarketStatus,
}

impl Indicator {
    pub fn new() -> Self {
        Self {
            market_status: MarketStatus::Trend,
        }
    }

    /// 检测市场状态
    pub fn detect_market(&mut self, volatility: f64, tr_ratio: f64) {
        self.market_status = if volatility > 0.15 || tr_ratio > 0.1 {
            MarketStatus::Pin
        } else if volatility < 0.02 && tr_ratio < 0.01 {
            MarketStatus::Range
        } else {
            MarketStatus::Trend
        };
    }

    /// 生成信号
    pub fn generate(&mut self, market: &MarketData, position: &PositionData) -> (Signal, Decimal) {
        let price = market.price;
        let vol = market.volatility;

        self.detect_market(vol, market.tr_ratio);

        match self.market_status {
            MarketStatus::Pin => self.pin_signals(price, position),
            MarketStatus::Trend => (Signal::None, Decimal::ZERO),
            MarketStatus::Range => (Signal::None, Decimal::ZERO),
        }
    }

    /// PIN行情信号
    fn pin_signals(&mut self, price: Decimal, pos: &PositionData) -> (Signal, Decimal) {
        // 盈利平仓
        if pos.long_qty > Decimal::ZERO && price > pos.long_price * (dec!(1) + config::PROFIT_THRESHOLD) {
            return (Signal::LongClose, pos.long_qty);
        }
        if pos.short_qty > Decimal::ZERO && price < pos.short_price * (dec!(1) - config::PROFIT_THRESHOLD) {
            return (Signal::ShortClose, pos.short_qty);
        }

        // 多头加仓
        if pos.long_qty > Decimal::ZERO {
            if price > pos.long_price * config::LONG_ADD_THRESHOLD || price > pos.long_price * config::LONG_ADD_HARD {
                return (Signal::LongAdd, pos.long_qty);
            }
            if price < pos.long_price * config::PRICE_DOWN_THRESHOLD || price < pos.long_price * config::PRICE_DOWN_HARD {
                return (Signal::ShortHedge, pos.long_qty);
            }
        }

        // 空头加仓
        if pos.short_qty > Decimal::ZERO {
            if price < pos.short_price * config::SHORT_ADD_THRESHOLD || price < pos.short_price * config::SHORT_ADD_HARD {
                return (Signal::ShortAdd, pos.short_qty);
            }
            if price > pos.short_price * config::PRICE_UP_THRESHOLD || price > pos.short_price * config::PRICE_UP_HARD {
                return (Signal::LongHedge, pos.short_qty);
            }
        }

        // 无持仓时开仓（TODO: 接入7条件Pin模式）
        if pos.long_qty == Decimal::ZERO && pos.short_qty == Decimal::ZERO {
            // return (Signal::LongOpen, dec!(0.05));
        }

        (Signal::None, Decimal::ZERO)
    }
}

impl Default for Indicator {
    fn default() -> Self {
        Self::new()
    }
}
