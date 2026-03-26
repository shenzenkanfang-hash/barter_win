//! a_market_data.rs - 市场数据
//!
//! 从 MarketDataStore 读取市场数据：K线、波动率、价格、持仓信息

#![forbid(unsafe_code)]

use b_data_source::{default_store, MarketDataStore};
use rust_decimal::Decimal;

/// 持仓方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionSide {
    Long,
    Short,
}

/// 市场数据快照（包含持仓信息，对齐 pin_main.py）
#[derive(Debug, Clone)]
pub struct MarketData {
    pub symbol: String,
    pub price: Decimal,
    pub volatility: f64,
    pub kline_1m: Option<KlineSnapshot>,
    /// 多头持仓均价
    pub long_price_all: Decimal,
    /// 多头持仓数量
    pub long_num_all: Decimal,
    /// 空头持仓均价
    pub short_price_all: Decimal,
    /// 空头持仓数量
    pub short_num_all: Decimal,
    /// 未实现盈亏
    pub unrealized_pnl: Decimal,
    /// 已实现盈亏
    pub realized_pnl: Decimal,
}

/// K线快照
#[derive(Debug, Clone)]
pub struct KlineSnapshot {
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
    pub timestamp: i64,
}

/// 读取市场数据（包含持仓信息）
pub fn read_market_data(symbol: &str) -> Option<MarketData> {
    let kline = default_store().get_current_kline(symbol)?;
    let vol = default_store().get_volatility(symbol)?;

    let price = kline.close.parse().ok()?;
    let open = kline.open.parse().ok()?;
    let high = kline.high.parse().ok()?;
    let low = kline.low.parse().ok()?;
    let volume = kline.volume.parse().ok()?;

    // TODO: 从持仓管理器获取实际持仓
    // 暂时用零值占位
    let long_price_all = Decimal::ZERO;
    let long_num_all = Decimal::ZERO;
    let short_price_all = Decimal::ZERO;
    let short_num_all = Decimal::ZERO;

    // 计算未实现盈亏
    let long_pnl = if long_num_all > Decimal::ZERO {
        (price - long_price_all) * long_num_all
    } else {
        Decimal::ZERO
    };
    let short_pnl = if short_num_all > Decimal::ZERO {
        (short_price_all - price) * short_num_all
    } else {
        Decimal::ZERO
    };

    Some(MarketData {
        symbol: symbol.to_string(),
        price,
        volatility: vol.volatility,
        kline_1m: Some(KlineSnapshot {
            open,
            high,
            low,
            close: price,
            volume,
            timestamp: kline.kline_start_time,
        }),
        long_price_all,
        long_num_all,
        short_price_all,
        short_num_all,
        unrealized_pnl: long_pnl + short_pnl,
        realized_pnl: Decimal::ZERO,
    })
}

/// 获取当前价格
pub fn get_price(symbol: &str) -> Option<Decimal> {
    default_store()
        .get_current_kline(symbol)?
        .close
        .parse()
        .ok()
}

/// 获取波动率
pub fn get_volatility(symbol: &str) -> Option<f64> {
    default_store().get_volatility(symbol).map(|v| v.volatility)
}
