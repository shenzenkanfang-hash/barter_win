//! Binance Depth 订单簿 WebSocket 订阅
//!
//! 默认只订阅 BTC 维护连接，高波动时扩展

use a_common::Paths;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use tokio::time::{sleep, Duration};
use tokio_tungstenite::{connect_async, tungstenite::Message};

/// Depth 数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepthData {
    #[serde(rename = "lastUpdateId")]
    pub last_update_id: i64,
    #[serde(rename = "bids")]
    pub bids: Vec<(String, String)>,
    #[serde(rename = "asks")]
    pub asks: Vec<(String, String)>,
}

/// Depth WebSocket 流管理器
pub struct DepthStream {
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

impl DepthStream {
    /// 创建 Depth 流管理器
    ///
    /// 默认只订阅 BTC 维护连接
    /// 高波动时可扩展到其他交易对
    pub async fn new(symbols: Vec<String>) -> Result<Self, a_common::MarketError> {
        let paths = Paths::new();
        let base_dir = format!(
            "{}/depth_realtime",
            paths.memory_backup_dir
        );

        tracing::info!(
            "DepthStream subscribing to {} symbols",
            symbols.len()
        );

        // 构建 streams: @depth@100ms (20档，100ms更新)
        let streams: Vec<String> = symbols
            .iter()
            .map(|s| format!("{}@depth@100ms", s.to_lowercase()))
            .collect();

        // 建立单一 WebSocket 连接
        let url = "wss://fstream.binance.com/stream".to_string();
        let (ws, _) = connect_async(&url)
            .await
            .map_err(|e| a_common::MarketError::WebSocketConnectionFailed(e.to_string()))?;

        let (mut write, read) = ws.split();

        // 发送订阅
        let subscribe_msg = serde_json::json!({
            "method": "SUBSCRIBE",
            "params": streams,
            "id": 1
        });

        write
            .send(Message::Text(subscribe_msg.to_string().into()))
            .await
            .map_err(|e| a_common::MarketError::WebSocketError(e.to_string()))?;

        tracing::info!("DepthStream subscribed, using memory backup dir: {}", base_dir);

        Ok(Self {
            base_dir,
            symbols,
            ws_stream: Some(read),
            file_handles: HashMap::new(),
        })
    }

    /// 创建只订阅 BTC 的维护连接
    pub async fn new_btc_only() -> Result<Self, a_common::MarketError> {
        Self::new(vec!["btcusdt".to_string()]).await
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

    /// 覆盖写入文件（截断后写入最新数据，确保监视器能看到）
    fn write_overwrite(&mut self, symbol: &str, json_str: &str) -> std::io::Result<()> {
        let symbol_lower = symbol.to_lowercase();
        let path = format!("{}/{}.json", self.base_dir, symbol_lower);
        let mut file = File::create(&path)?;
        file.write_all(json_str.as_bytes())?;
        file.write_all(b"\n")?;
        file.flush()?;
        file.sync_all()
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

            // 解析 Depth 数据
            if let Some(data) = obj.get("data") {
                if let Some(depth_obj) = data.get("depth") {
                    if let Some(symbol) = depth_obj.get("s").and_then(|v| v.as_str()) {
                        if let Some(json_str) = serde_json::to_string(&depth_obj).ok() {
                            // 覆盖写入：每次只保留最新一条数据
                            let _ = self.write_overwrite(symbol, &json_str);
                        }
                    }
                }
            }
        }

        Some(text)
    }
}
