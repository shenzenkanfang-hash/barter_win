//! Binance 1m K线 WebSocket 订阅
//!
//! 分片订阅: 每批50个symbol，间隔500ms发送

use crate::volatility::VolatilityManager;
use a_common::Paths;
use a_common::volatility::KLineInput;
use chrono::{DateTime, TimeZone, Utc};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::time::Instant;
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
    /// 历史K线目录（收盘时追加写入）
    history_dir: String,
    symbols: Vec<String>,
    ws_stream: Option<
        futures_util::stream::SplitStream<
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
        >,
    >,
    file_handles: HashMap<String, File>,
    /// 记录每个 symbol 上次写入时间（用于超时强制写入）
    last_write_times: HashMap<String, Instant>,
    /// 超时写入间隔（秒）
    write_timeout_secs: u64,
    /// 波动率管理器
    volatility_manager: VolatilityManager,
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
        let history_dir = format!(
            "{}/kline_1m_history",
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
            history_dir,
            symbols,
            ws_stream: Some(read),
            file_handles: HashMap::new(),
            last_write_times: HashMap::new(),
            write_timeout_secs: 5, // 5秒超时写入
            volatility_manager: VolatilityManager::new(),
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

    /// 覆盖写入文件（截断后写入最新数据）
    fn write_overwrite(&mut self, symbol: &str, json_str: &str) -> std::io::Result<()> {
        let symbol_lower = symbol.to_lowercase();
        let path = format!("{}/{}.json", self.base_dir, symbol_lower);
        // 先创建目录，失败时记录错误
        if let Err(e) = Self::ensure_dir(std::path::Path::new(&path)) {
            tracing::error!("Failed to create directory for {}: {}", symbol_lower, e);
            return Err(e);
        }
        let mut file = File::create(&path)?;
        file.write_all(json_str.as_bytes())?;
        file.write_all(b"\n")?;
        file.flush()?;  // 内存盘不需要 sync_all
        tracing::debug!("Write kline to {}: {} bytes", path, json_str.len());
        Ok(())
    }

    /// 写入历史K线文件（收盘时调用）
    /// 格式: [[o,h,l,c,v,t], [o,h,l,c,v,t], ...]
    fn write_to_history(&mut self, symbol: &str, kline_obj: &serde_json::Value) -> std::io::Result<()> {
        let symbol_lower = symbol.to_lowercase();
        let path = format!("{}/{}.json", self.history_dir, symbol_lower);

        // 确保目录存在
        if let Err(e) = Self::ensure_dir(std::path::Path::new(&path)) {
            tracing::error!("Failed to create history directory for {}: {}", symbol_lower, e);
            return Err(e);
        }

        // 提取 OHLCVT 数据: [open, high, low, close, volume, time]
        let o = kline_obj.get("o").and_then(|v| v.as_str()).unwrap_or("0");
        let h = kline_obj.get("h").and_then(|v| v.as_str()).unwrap_or("0");
        let l = kline_obj.get("l").and_then(|v| v.as_str()).unwrap_or("0");
        let c = kline_obj.get("c").and_then(|v| v.as_str()).unwrap_or("0");
        let v = kline_obj.get("v").and_then(|v| v.as_str()).unwrap_or("0");
        let t = kline_obj.get("T").and_then(|v| v.as_i64()).unwrap_or(0);

        let ohlcvt = serde_json::json!([o, h, l, c, v, t]);

        // 读取现有数据或创建新数组
        let mut data: Vec<serde_json::Value> = Vec::new();

        if std::path::Path::new(&path).exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(existing) = serde_json::from_str::<Vec<serde_json::Value>>(&content) {
                    data = existing;
                }
            }
        }

        // 时间戳校验：必须大于最后一条（但第一条K线允许 t=0）
        let last_time = data.last()
            .and_then(|k| k.as_array().and_then(|a| a.get(5)))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        // 跳过条件：非首条K线 且 时间戳 <= 最后一条
        if !data.is_empty() && t <= last_time {
            tracing::debug!("Skip duplicate/disordered kline: t={} <= last={}", t, last_time);
            return Ok(());
        }

        // 追加新K线
        data.push(ohlcvt);

        // 写入文件
        let json_str = serde_json::to_string(&data)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let mut file = File::create(&path)?;
        file.write_all(json_str.as_bytes())?;
        file.write_all(b"\n")?;
        file.flush()?;
        tracing::debug!("Write history kline to {}: {} klines", path, data.len());
        Ok(())
    }

    /// 判断是否应该写入：收盘 或 超时
    fn should_write(&self, symbol: &str, is_closed: bool) -> bool {
        let symbol_lower = symbol.to_lowercase();
        let now = Instant::now();
        let timeout = Duration::from_secs(self.write_timeout_secs);

        // 收盘时立即写入
        if is_closed {
            return true;
        }

        // 非收盘时，检查是否超时
        if let Some(last_time) = self.last_write_times.get(&symbol_lower) {
            if now.duration_since(*last_time) >= timeout {
                return true;
            }
        } else {
            // 首次写入，需要写入
            return true;
        }

        false
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

            // 尝试解析 K线数据 - 支持两种格式:
            // 格式1: {"data":{"k":{...}}} (标准格式)
            // 格式2: {"i":"1m","s":"BTCUSDT",...} (直接字段格式)
            let kline_obj = if let Some(data) = obj.get("data") {
                data.get("k")
            } else if obj.get("i").is_some() && obj.get("s").is_some() {
                // 直接在顶层，可能是简化格式
                Some(&obj)
            } else {
                None
            };

            if let Some(kline) = kline_obj {
                if let (Some(symbol), Some(json_str), Some(is_closed)) = (
                    kline.get("s").and_then(|v| v.as_str()),
                    serde_json::to_string(&kline).ok(),
                    kline.get("x").and_then(|v| v.as_bool()),
                ) {
                    // 解析 KLineInput 用于波动率计算
                    if let (Some(o), Some(h), Some(l), Some(c), Some(t)) = (
                        kline.get("o").and_then(|v| v.as_str()),
                        kline.get("h").and_then(|v| v.as_str()),
                        kline.get("l").and_then(|v| v.as_str()),
                        kline.get("c").and_then(|v| v.as_str()),
                        kline.get("T").and_then(|v| v.as_i64()),
                    ) {
                        // 将字符串转换为 Decimal，Binance 价格精度为 8 位小数
                        let parse_price = |s: &str| -> rust_decimal::Decimal {
                            s.parse::<rust_decimal::Decimal>().unwrap_or(rust_decimal::Decimal::ZERO)
                        };
                        let timestamp_ms = t;
                        let timestamp = match Utc.timestamp_millis_opt(timestamp_ms) {
                            chrono::LocalResult::Single(t) => t,
                            _ => chrono::Utc::now(),
                        };

                        let kline_input = KLineInput {
                            open: parse_price(o),
                            high: parse_price(h),
                            low: parse_price(l),
                            close: parse_price(c),
                            timestamp,
                        };

                        // 每 tick 更新波动率（这是项目的基石）
                        let _vol_stats = self.volatility_manager.update(symbol, kline_input);

                        // 检查是否需要输出每分钟汇总
                        self.volatility_manager.check_and_log_summary();
                    }

                    // K线闭合时，写入历史目录（结构化格式）
                    if is_closed {
                        let _ = self.write_to_history(symbol, kline);
                    }
                    // 写入条件：收盘 或 超时(5秒)
                    if self.should_write(symbol, is_closed) {
                        if self.write_overwrite(symbol, &json_str).is_ok() {
                            // 写入成功，重置计时器
                            let symbol_lower = symbol.to_lowercase();
                            self.last_write_times.insert(symbol_lower, Instant::now());
                        }
                    }
                }
            }
        }

        Some(text)
    }
}
