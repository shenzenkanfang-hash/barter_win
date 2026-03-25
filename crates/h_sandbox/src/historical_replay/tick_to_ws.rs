//! SimulatedTick → BinanceKlineMsg 转换器
//!
//! 将内部 SimulatedTick 转换为 Binance WebSocket K线消息格式

use a_common::ws::binance_ws::{BinanceKlineMsg, KlineData};
use super::SimulatedTick;

/// SimulatedTick → BinanceKlineMsg 转换器
pub struct TickToWsConverter {
    symbol: String,
    interval: String,
}

impl TickToWsConverter {
    /// 创建转换器
    pub fn new(symbol: String, interval: String) -> Self {
        Self { symbol, interval }
    }

    /// 将 SimulatedTick 转换为 BinanceKlineMsg
    ///
    /// # 参数
    /// * `tick` - SimulatedTick 数据
    /// * `tick_index` - 当前 tick 在 K 线内的索引 (0-59)
    /// * `is_last_tick` - 是否是当前 K 线的最后一个 tick
    pub fn convert(&self, tick: &SimulatedTick, tick_index: u8, is_last_tick: bool) -> BinanceKlineMsg {
        BinanceKlineMsg {
            event_type: "kline".to_string(),
            event_time: tick.timestamp.timestamp_millis(),
            symbol: tick.symbol.clone(),
            kline: KlineData {
                kline_start_time: tick.kline_timestamp.timestamp_millis(),
                kline_close_time: tick.kline_timestamp.timestamp_millis() + 60_000,
                symbol: tick.symbol.clone(),
                interval: self.interval.clone(),
                first_trade_id: 0,
                last_trade_id: 0,
                open: tick.open.to_string(),
                close: tick.price.to_string(),
                high: tick.high.to_string(),
                low: tick.low.to_string(),
                volume: tick.volume.to_string(),
                num_trades: 0,
                is_closed: is_last_tick,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use rust_decimal_macros::dec;

    #[test]
    fn test_tick_to_ws_converter() {
        let converter = TickToWsConverter::new("BTCUSDT".to_string(), "1m".to_string());

        let tick = SimulatedTick {
            symbol: "BTCUSDT".to_string(),
            price: dec!(50025),
            qty: dec!(0.01),
            timestamp: Utc::now(),
            open: dec!(50000),
            high: dec!(50050),
            low: dec!(49975),
            volume: dec!(1.5),
            kline_timestamp: Utc::now(),
        };

        let ws_msg = converter.convert(&tick, 30, false);

        assert_eq!(ws_msg.event_type, "kline");
        assert_eq!(ws_msg.symbol, "BTCUSDT");
        assert_eq!(ws_msg.kline.interval, "1m");
        assert!(!ws_msg.kline.is_closed);
    }

    #[test]
    fn test_last_tick_is_closed() {
        let converter = TickToWsConverter::new("BTCUSDT".to_string(), "1m".to_string());

        let tick = SimulatedTick {
            symbol: "BTCUSDT".to_string(),
            price: dec!(50050),
            qty: dec!(0.01),
            timestamp: Utc::now(),
            open: dec!(50000),
            high: dec!(50050),
            low: dec!(49975),
            volume: dec!(1.5),
            kline_timestamp: Utc::now(),
        };

        let ws_msg = converter.convert(&tick, 59, true);

        assert!(ws_msg.kline.is_closed);
    }
}
