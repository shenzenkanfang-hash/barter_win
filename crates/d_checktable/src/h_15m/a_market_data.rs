//! market_data.rs - 市场数据
//!
//! 从 MarketDataStore 读取市场数据：K线、波动率、价格

#![forbid(unsafe_code)]

use b_data_source::{default_store, MarketDataStore};
use rust_decimal::Decimal;

/// 市场数据快照
#[derive(Debug, Clone)]
pub struct MarketData {
    pub symbol: String,
    pub price: Decimal,
    pub volatility: f64,
    pub kline_1m: Option<KlineSnapshot>,
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

/// 读取市场数据
pub fn read_market_data(symbol: &str) -> Option<MarketData> {
    let kline = default_store().get_current_kline(symbol)?;
    let vol = default_store().get_volatility(symbol)?;

    let price = kline.close.parse().ok()?;
    let open = kline.open.parse().ok()?;
    let high = kline.high.parse().ok()?;
    let low = kline.low.parse().ok()?;
    let volume = kline.volume.parse().ok()?;

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
