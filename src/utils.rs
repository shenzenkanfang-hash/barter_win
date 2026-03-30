//! 工具函数

use b_data_mock::MarketDataStore;
use rust_decimal::Decimal;
use crate::tick_context::RawKline;

/// 解析 K线原始数据
pub fn parse_raw_kline(data: &str) -> Result<RawKline, Box<dyn std::error::Error>> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "kebab-case")]
    struct RawFull {
        #[serde(rename = "open")]
        open_str: String,
        #[serde(rename = "close")]
        close_str: String,
        #[serde(rename = "high")]
        high_str: String,
        #[serde(rename = "low")]
        low_str: String,
        #[serde(rename = "volume")]
        volume_str: String,
        is_closed: bool,
    }

    #[derive(serde::Deserialize)]
    #[allow(dead_code)]
    struct RawBinance {
        #[serde(rename = "o")]
        o_str: String,
        #[serde(rename = "c")]
        c_str: String,
        #[serde(rename = "h")]
        h_str: String,
        #[serde(rename = "l")]
        l_str: String,
        #[serde(rename = "v")]
        v_str: String,
        #[serde(rename = "x")]
        x: bool,
    }

    #[derive(serde::Deserialize)]
    struct RawWrap {
        data: RawBinance,
    }

    if let Ok(raw) = serde_json::from_str::<RawFull>(data) {
        return Ok(RawKline {
            open: raw.open_str.parse()?,
            close: raw.close_str.parse()?,
            high: raw.high_str.parse()?,
            low: raw.low_str.parse()?,
            volume: raw.volume_str.parse()?,
            is_closed: raw.is_closed,
        });
    }

    if let Ok(raw) = serde_json::from_str::<RawBinance>(data) {
        return Ok(RawKline {
            open: raw.o_str.parse()?,
            close: raw.c_str.parse()?,
            high: raw.h_str.parse()?,
            low: raw.l_str.parse()?,
            volume: raw.v_str.parse()?,
            is_closed: raw.x,
        });
    }

    let wrapped: RawWrap = serde_json::from_str(data)?;
    let raw = wrapped.data;
    Ok(RawKline {
        open: raw.o_str.parse()?,
        close: raw.c_str.parse()?,
        high: raw.h_str.parse()?,
        low: raw.l_str.parse()?,
        volume: raw.v_str.parse()?,
        is_closed: raw.x,
    })
}

// NO_SIGNAL 修复：指标转换函数
pub type StoreRef = std::sync::Arc<b_data_mock::store::MarketDataStoreImpl>;
pub type MarketIndicators = d_checktable::h_15m::trader::MarketIndicators;
pub type TraderError = d_checktable::h_15m::trader::TraderError;

/// 从 Store 读取 Indicator1mOutput JSON 并转换为 MarketIndicators
pub fn convert_store_indicator_to_market_indicators(
    store: &StoreRef,
    symbol: &str,
) -> Result<MarketIndicators, TraderError> {
    let json = store
        .get_indicator(symbol)
        .ok_or_else(|| TraderError::Other(String::from("no indicator in store")))?;

    let ind: c_data_process::min::trend::Indicator1mOutput = serde_json::from_value(json)
        .map_err(|e| TraderError::Other(format!("indicator deserialize error: {}", e)))?;

    let (price_deviation, price_deviation_horizontal_position) = {
        let history = store.get_history_klines(symbol);
        let current = store.get_current_kline(symbol);
        tracing::trace!(
            symbol = %symbol,
            history_len = history.len(),
            current_price = ?current.as_ref().map(|k| &k.close),
            "convert: checking store state"
        );

        match (history.len(), current) {
            (len, Some(curr)) if len >= 14 => {
                let closes: Vec<f64> = history
                    .iter()
                    .filter_map(|k| k.close.parse::<f64>().ok())
                    .collect();
                let current_price = match curr.close.parse::<f64>() {
                    Ok(p) => p,
                    Err(_) => 0.0,
                };

                let n = closes.len();
                let mean = closes.iter().sum::<f64>() / n as f64;
                let variance = closes.iter().map(|p| (p - mean).powi(2)).sum::<f64>() / n as f64;
                let stddev = variance.sqrt();

                let pd = if mean > 0.0 && stddev > 0.0 {
                    Decimal::try_from((current_price - mean) / mean * 100.0).ok()
                } else {
                    None
                };

                let recent: Vec<f64> = history
                    .iter()
                    .rev()
                    .take(60)
                    .filter_map(|k| k.close.parse::<f64>().ok())
                    .collect();
                let hpos = if let Some((min_p, max_p)) =
                    recent.iter().cloned().fold(None, |acc: Option<(f64, f64)>, p| match acc {
                        None => Some((p, p)),
                        Some((min_v, max_v)) => Some((min_v.min(p), max_v.max(p))),
                    })
                {
                    let range = max_p - min_p;
                    if range > 0.0 {
                        Decimal::try_from(((current_price - min_p) / range * 100.0).clamp(0.0, 100.0)).ok()
                    } else {
                        Decimal::try_from(50.0).ok()
                    }
                } else {
                    Decimal::try_from(50.0).ok()
                };

                tracing::debug!(
                    symbol = %symbol,
                    closes_count = closes.len(),
                    mean = %mean,
                    stddev = %stddev,
                    current_price = %current_price,
                    price_deviation = ?pd,
                    hpos = ?hpos,
                    "convert: computed values"
                );

                (pd.unwrap_or(Decimal::ZERO), hpos.unwrap_or(rust_decimal_macros::dec!(50)))
            }
            _ => (Decimal::ZERO, rust_decimal_macros::dec!(50)),
        }
    };

    Ok(MarketIndicators {
        tr_base_60min: ind.tr_base_10min,
        tr_ratio_15min: ind.tr_ratio_10min_1h,
        zscore_14_1m: ind.zscore_14_1m,
        zscore_1h_1m: ind.zscore_1h_1m,
        tr_ratio_60min_5h: ind.tr_ratio_10min_1h,
        tr_ratio_10min_1h: ind.tr_ratio_10min_1h,
        pos_norm_60: ind.pos_norm_60,
        acc_percentile_1h: ind.acc_percentile,
        velocity_percentile_1h: ind.velocity_percentile,
        pine_bg_color: String::new(),
        pine_bar_color: String::new(),
        price_deviation,
        price_deviation_horizontal_position,
    })
}
