//! Binance 1m K线 WebSocket 订阅
//!
//! 分片订阅: 每批50个symbol，间隔500ms发送

use a_common::Paths;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use tokio::time::{sleep, Duration};
use tokio_tungstenite::{connect_async, tungstenite::Message};

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
    #[serde(rename = "x")]
    pub is_closed: bool,
}

/// 1m K线 WebSocket 流管理器
pub struct Kline1mStream {
    base_dir: String,
    symbols: Vec<String>,
    ws_stream: Option<
        futures_util::stream::SplitStream<
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
        >,
    >,
    file_handles: HashMap<String, File>,
}

impl Kline1mStream {
    /// 创建 1m K线流管理器 (分片订阅: 每批50个，间隔500ms)
    ///
    /// Binance WebSocket 约束:
    /// - Base URL: wss://fstream.binance.com
    /// - 单连接最多 1024 个 streams
    /// - 每秒最多 10 个订阅消息
    pub async fn new(symbols: Vec<String>) -> Result<Self, a_common::MarketError> {
        let paths = Paths::new();
        let base_dir = format!(
            "{}/kline_1m_realtime",
            paths.memory_backup_dir
        );

        // 分片订阅参数
        const BATCH_SIZE: usize = 50;
        const BATCH_INTERVAL_MS: u64 = 500;

        // 构建所有 streams
        let streams: Vec<String> = symbols
            .iter()
            .map(|s| format!("{}@kline_1m", s.to_lowercase()))
            .collect();

        tracing::info!(
            "Kline1mStream subscribing to {} symbols in {} batches ({}ms interval)",
            symbols.len(),
            (streams.len() + BATCH_SIZE - 1) / BATCH_SIZE,
            BATCH_INTERVAL_MS
        );

        // 建立单一 WebSocket 连接
        let url = "wss://fstream.binance.com/stream".to_string();
        let (ws, _) = connect_async(&url)
            .await
            .map_err(|e| a_common::MarketError::WebSocketConnectionFailed(e.to_string()))?;

        let (mut write, read) = ws.split();

        // 分批发送订阅消息
        let total_batches = (streams.len() + BATCH_SIZE - 1) / BATCH_SIZE;
        for (i, batch) in streams.chunks(BATCH_SIZE).enumerate() {
            let subscribe_msg = serde_json::json!({
                "method": "SUBSCRIBE",
                "params": batch.to_vec(),
                "id": i as i64 + 1
            });

            write
                .send(Message::Text(subscribe_msg.to_string().into()))
                .await
                .map_err(|e| a_common::MarketError::WebSocketError(e.to_string()))?;

            tracing::debug!(
                "Sent subscription batch {}/{} ({} streams)",
                i + 1,
                total_batches,
                batch.len()
            );

            // 批次间等待（最后一个批次后不需要等待）
            if i < total_batches - 1 {
                sleep(Duration::from_millis(BATCH_INTERVAL_MS)).await;
            }
        }

        tracing::info!("Kline1mStream all subscriptions sent, using memory backup dir: {}", base_dir);

        Ok(Self {
            base_dir,
            symbols,
            ws_stream: Some(read),
            file_handles: HashMap::new(),
        })
    }

    fn ensure_dir(path: &std::path::Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Ok(())
    }

    fn get_file(&mut self, symbol: &str) -> std::io::Result<&mut File> {
        let symbol_lower = symbol.to_lowercase();
        if !self.file_handles.contains_key(&symbol_lower) {
            let path = format!("{}/{}.json", self.base_dir, symbol_lower);
            Self::ensure_dir(std::path::Path::new(&path))?;
            let file = File::create(&path)?;
            self.file_handles.insert(symbol_lower.clone(), file);
        }
        Ok(self.file_handles.get_mut(&symbol_lower).unwrap())
    }

    /// 获取下一条消息并写入缓存
    pub async fn next_message(&mut self) -> Option<String> {
        use futures_util::StreamExt;
        let stream = self.ws_stream.as_mut()?;
        let msg = match stream.next().await {
            Some(Ok(msg)) => msg,
            _ => return None,
        }.into_text().ok()?;
        let text = msg.to_string();

        // 解析消息
        if let Ok(obj) = serde_json::from_str::<serde_json::Value>(&text) {
            // 忽略订阅确认消息
            if obj.get("result").is_some() && obj.get("id").is_some() {
                return Some(text);
            }

            // 解析 K线数据
            if let Some(data) = obj.get("data") {
                if let Some(kline_obj) = data.get("k") {
                    if let (Some(symbol), Some(json_str)) = (
                        kline_obj.get("s").and_then(|v| v.as_str()),
                        serde_json::to_string(&kline_obj).ok(),
                    ) {
                        if let Ok(ref mut f) = self.get_file(symbol) {
                            let _ = f.write_all(json_str.as_bytes());
                            let _ = f.write_all(b"\n");
                            let _ = f.flush();
                        }
                    }
                }
            }
        }

        Some(text)
    }
}
