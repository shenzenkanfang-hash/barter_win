//! Binance 测试网 WebSocket 连接器
//!
//! 连接地址: wss://stream.binancefuture.com/ws/

use crate::error::MarketError;
use crate::types::Tick;
use chrono::{TimeZone, Utc};
use futures_util::StreamExt;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use tokio_tungstenite::{connect_async, tungstenite::Message};

/// Binance WebSocket 连接器 (测试网)
pub struct BinanceWsConnector {
    url: String,
    symbol: String,
}

/// Binance Trade WebSocket 消息格式
#[derive(Debug, Clone, Serialize, Deserialize)]
struct BinanceTradeMsg {
    #[serde(rename = "e")]
    event_type: String,
    #[serde(rename = "E")]
    event_time: i64,
    #[serde(rename = "s")]
    symbol: String,
    #[serde(rename = "t")]
    trade_id: i64,
    #[serde(rename = "p")]
    price: String,
    #[serde(rename = "q")]
    quantity: String,
    #[serde(rename = "T")]
    trade_time: i64,
    #[serde(rename = "m")]
    is_buyer_maker: bool,
}

impl BinanceWsConnector {
    pub fn new(symbol: &str) -> Self {
        // Binance 测试网 WebSocket URL
        let url = format!(
            "wss://stream.binancefuture.com/ws/{}@trade",
            symbol.to_lowercase()
        );
        Self {
            url,
            symbol: symbol.to_string(),
        }
    }

    /// 连接到 Binance WebSocket 并返回 ticks
    pub async fn connect(
        &self,
    ) -> Result<BinanceTradeStream, MarketError> {
        let (ws_stream, _) = connect_async(&self.url)
            .await
            .map_err(|e| MarketError::WebSocketConnectionFailed(e.to_string()))?;

        tracing::info!("Binance WebSocket 连接成功: {}", self.url);

        let (write, read) = ws_stream.split();
        Ok(BinanceTradeStream {
            ws_stream: read,
            _write: write,
            symbol: self.symbol.clone(),
        })
    }
}

/// Binance Trade 流
pub struct BinanceTradeStream {
    ws_stream: futures_util::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    >,
    _write: futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
        Message,
    >,
    symbol: String,
}

impl BinanceTradeStream {
    /// 获取下一个 tick
    pub async fn next_tick(&mut self) -> Option<Tick> {
        while let Some(msg) = self.ws_stream.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Ok(trade) = serde_json::from_str::<BinanceTradeMsg>(&text) {
                        return Some(Tick {
                            symbol: self.symbol.clone(),
                            price: Decimal::from_str(&trade.price).ok()?,
                            qty: Decimal::from_str(&trade.quantity).ok()?,
                            timestamp: Utc.timestamp_millis_opt(trade.trade_time).unwrap(),
                        });
                    }
                }
                Ok(Message::Ping(_)) => {
                    // 处理 ping
                }
                Ok(Message::Close(_)) => {
                    tracing::warn!("Binance WebSocket 连接关闭");
                    break;
                }
                Err(e) => {
                    tracing::error!("WebSocket 错误: {}", e);
                    break;
                }
                _ => {}
            }
        }
        None
    }
}
