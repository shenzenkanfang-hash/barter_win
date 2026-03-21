//! Binance 测试网 WebSocket 连接器
//!
//! 连接地址: wss://stream.binancefuture.com/ws/
//! 本模块只处理 WebSocket 协议，返回原始消息，业务类型转换由 DataFeeder 完成。

use crate::claint::error::MarketError;
use chrono::{TimeZone, Utc};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::sleep;
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

/// Binance Trade WebSocket 消息格式 (原始消息)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinanceTradeMsg {
    #[serde(rename = "e")]
    pub event_type: String,
    #[serde(rename = "E")]
    pub event_time: i64,
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "t")]
    pub trade_id: i64,
    #[serde(rename = "p")]
    pub price: String,
    #[serde(rename = "q")]
    pub quantity: String,
    #[serde(rename = "T")]
    pub trade_time: i64,
    #[serde(rename = "m")]
    pub is_buyer_maker: bool,
}

/// Binance Kline WebSocket 消息格式 (原始消息)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinanceKlineMsg {
    #[serde(rename = "e")]
    pub event_type: String,
    #[serde(rename = "E")]
    pub event_time: i64,
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "k")]
    pub kline: KlineData,
}

/// Kline 数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KlineData {
    #[serde(rename = "t")]
    pub kline_start_time: i64,
    #[serde(rename = "T")]
    pub kline_close_time: i64,
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "i")]
    pub interval: String,
    #[serde(rename = "f")]
    pub first_trade_id: i64,
    #[serde(rename = "L")]
    pub last_trade_id: i64,
    #[serde(rename = "o")]
    pub open: String,
    #[serde(rename = "c")]
    pub close: String,
    #[serde(rename = "h")]
    pub high: String,
    #[serde(rename = "l")]
    pub low: String,
    #[serde(rename = "v")]
    pub volume: String,
    #[serde(rename = "n")]
    pub num_trades: i64,
    #[serde(rename = "x")]
    pub is_closed: bool,
}

/// Binance Depth WebSocket 消息格式 (原始消息)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinanceDepthMsg {
    #[serde(rename = "e")]
    pub event_type: String,
    #[serde(rename = "E")]
    pub event_time: i64,
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "U")]
    pub first_update_id: i64,
    #[serde(rename = "u")]
    pub final_update_id: i64,
    #[serde(rename = "b")]
    pub bids: Vec<PriceLevel>,
    #[serde(rename = "a")]
    pub asks: Vec<PriceLevel>,
}

/// 价格级别
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceLevel {
    #[serde(rename = "0")]
    pub price: String,
    #[serde(rename = "1")]
    pub qty: String,
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

    /// 连接到 Binance WebSocket 并返回 stream
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

    /// 重连 (指数退避)
    ///
    /// 重连策略: 5s → 10s → 20s → ... → 120s (最大)
    pub async fn reconnect_with_backoff(&mut self) -> Result<(), MarketError> {
        let mut backoff = Duration::from_secs(5);
        let max_backoff = Duration::from_secs(120);

        loop {
            tracing::info!("WebSocket 重连中, 等待 {:?}...", backoff);
            sleep(backoff).await;

            match self.connect().await {
                Ok(_) => {
                    tracing::info!("WebSocket 重连成功");
                    return Ok(());
                }
                Err(e) => {
                    tracing::warn!("WebSocket 重连失败: {}", e);
                    backoff = (backoff * 2).min(max_backoff);
                }
            }
        }
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
    /// 获取下一个原始 Trade 消息
    /// 调用方负责转换为业务类型
    pub async fn next_message(&mut self) -> Option<String> {
        while let Some(msg) = self.ws_stream.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    return Some(text.to_string());
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

    /// 解析为 Trade 消息
    pub fn parse_trade(&self, text: &str) -> Option<BinanceTradeMsg> {
        serde_json::from_str::<BinanceTradeMsg>(text).ok()
    }

    /// 解析为 Kline 消息
    pub fn parse_kline(&self, text: &str) -> Option<BinanceKlineMsg> {
        serde_json::from_str::<BinanceKlineMsg>(text).ok()
    }

    /// 解析为 Depth 消息
    pub fn parse_depth(&self, text: &str) -> Option<BinanceDepthMsg> {
        serde_json::from_str::<BinanceDepthMsg>(text).ok()
    }
}
