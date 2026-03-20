//! Binance 测试网 WebSocket 连接器
//!
//! 连接地址: wss://stream.binancefuture.com/ws/

use crate::error::MarketError;
use crate::types::Tick;
use chrono::{TimeZone, Utc};
use futures_util::{SinkExt, StreamExt};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use tokio_tungstenite::{connect_async, tungstenite::Message};

/// Binance WebSocket 连接器 (测试网)
pub struct BinanceWsConnector {
    url: String,
    symbol: String,
    ws_stream: Option<
        futures_util::stream::SplitSink<
            tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
            Message,
        >,
    >,
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
    /// 创建 Trade 流连接器 (单 symbol)
    pub fn new(symbol: &str) -> Self {
        let url = format!(
            "wss://stream.binancefuture.com/ws/{}@trade",
            symbol.to_lowercase()
        );
        Self {
            url,
            symbol: symbol.to_string(),
            ws_stream: None,
        }
    }

    /// 创建多 stream 连接器 (用于 KLine/Depth 批量订阅)
    /// url: wss://stream.binancefuture.com/ws
    /// streams: 要订阅的 streams 列表，如 ["btcusdt@kline_1m", "ethusdt@kline_1m"]
    pub fn new_multi(url: &str, streams: Vec<String>) -> Self {
        Self {
            url: url.to_string(),
            symbol: streams.join(","), // 用于标识
            ws_stream: None,
        }
    }

    /// 连接到 Binance WebSocket 并返回 ticks
    pub async fn connect(&mut self) -> Result<BinanceTradeStream, MarketError> {
        let (ws_stream, _) = connect_async(&self.url)
            .await
            .map_err(|e| MarketError::WebSocketConnectionFailed(e.to_string()))?;

        tracing::info!("Binance WebSocket 连接成功: {}", self.url);

        let (write, read) = ws_stream.split();
        self.ws_stream = Some(write);
        Ok(BinanceTradeStream {
            ws_stream: read,
            symbol: self.symbol.clone(),
        })
    }

    /// 发送订阅消息
    pub async fn subscribe(&mut self, streams: &[String]) -> Result<(), MarketError> {
        let msg = serde_json::json!({
            "method": "SUBSCRIBE",
            "params": streams,
            "id": chrono::Utc::now().timestamp_millis()
        });

        let text = serde_json::to_string(&msg)
            .map_err(|e| MarketError::SerializeError(e.to_string()))?;

        let write = self.ws_stream.as_mut()
            .ok_or_else(|| MarketError::WebSocketError("Not connected".to_string()))?;

        write.send(Message::Text(text.into()))
            .await
            .map_err(|e| MarketError::WebSocketError(e.to_string()))?;

        Ok(())
    }

    /// 发送退订消息
    pub async fn unsubscribe(&mut self, streams: &[String]) -> Result<(), MarketError> {
        let msg = serde_json::json!({
            "method": "UNSUBSCRIBE",
            "params": streams,
            "id": chrono::Utc::now().timestamp_millis()
        });

        let text = serde_json::to_string(&msg)
            .map_err(|e| MarketError::SerializeError(e.to_string()))?;

        let write = self.ws_stream.as_mut()
            .ok_or_else(|| MarketError::WebSocketError("Not connected".to_string()))?;

        write.send(Message::Text(text.into()))
            .await
            .map_err(|e| MarketError::WebSocketError(e.to_string()))?;

        Ok(())
    }
}

/// Binance Trade 流
pub struct BinanceTradeStream {
    ws_stream: futures_util::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
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
