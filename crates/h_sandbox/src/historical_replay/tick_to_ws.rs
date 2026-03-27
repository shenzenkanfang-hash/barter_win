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
    /// * `tick` - SimulatedTick 数据（含 is_last_in_kline 标记）
    /// * `tick_index` - 当前 tick 在 K 线内的索引 (0-59)
    /// * `is_last_tick` - 备选：外部指定的 last tick 标记（当 tick.is_last_in_kline 为 true 时优先使用）
    pub fn convert(&self, tick: &SimulatedTick, _tick_index: u8, is_last_tick: bool) -> BinanceKlineMsg {
        // 优先使用 SimulatedTick 自身的 is_last_in_kline 标记（由生成器内部判断）
        // 若未标记（外部构造的 tick），则使用传入的 is_last_tick 参数
        let is_closed = if tick.is_last_in_kline {
            true
        } else {
            is_last_tick
        };

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
                is_closed,
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
            sequence_id: 1,
            open: dec!(50000),
            high: dec!(50050),
            low: dec!(49975),
            volume: dec!(1.5),
            kline_timestamp: Utc::now(),
            is_last_in_kline: false,
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
            sequence_id: 60,
            open: dec!(50000),
            high: dec!(50050),
            low: dec!(49975),
            volume: dec!(1.5),
            kline_timestamp: Utc::now(),
            is_last_in_kline: true, // 由生成器内部判断为最后一根
        };

        let ws_msg = converter.convert(&tick, 59, false); // 外部参数传 false，但 tick.is_last_in_kline=true

        assert!(ws_msg.kline.is_closed);
    }
}
