//! MarketStream trait 和 Mock 实现
//!
//! 定义市场数据流接口，返回业务类型 Tick。

use crate::models::types::Tick;
use async_trait::async_trait;
use chrono::TimeZone;
use rust_decimal::Decimal;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::io::Write;
use std::fs::File;

/// 市场数据流 trait
#[async_trait]
pub trait MarketStream: Send + Sync {
    async fn next_tick(&self) -> Option<Tick>;
}

/// 模拟市场数据流 - 用于测试
pub struct MockMarketStream {
    symbol: String,
    base_price: Decimal,
    current_price: Decimal,
}

impl MockMarketStream {
    pub fn new(symbol: String, base_price: Decimal) -> Self {
        Self {
            symbol,
            base_price,
            current_price: base_price,
        }
    }
}

#[async_trait]
impl MarketStream for MockMarketStream {
    async fn next_tick(&self) -> Option<Tick> {
        use rand::Rng;

        // 简单的随机游走价格生成
        let change_percent = rand::thread_rng().gen_range(-0.001..0.001);
        let price_change = self.current_price * Decimal::try_from(change_percent).ok()?;

        let new_price = self.current_price + price_change;

        // 确保价格不会变得太小
        let final_price = if new_price < self.base_price * Decimal::try_from(0.5).ok()? {
            self.base_price
        } else {
            new_price
        };

        // 注意: 这不是线程安全的，但在单线程测试场景下可以工作
        // 如果需要真正的线程安全，应该用 Arc<Mutex<Decimal>>
        let price_for_tick = final_price;

        Some(Tick {
            symbol: self.symbol.clone(),
            price: price_for_tick,
            qty: Decimal::try_from(1.0).ok()?,
            timestamp: Utc::now(),
            kline_1m: None,
            kline_15m: None,
            kline_1d: None,
        })
    }
}

/// 真实 Binance WebSocket 市场数据流
pub struct BinanceMarketStream {
    /// WebSocket 连接器 (保留用于未来重连逻辑)
    _connector: Arc<Mutex<a_common::ws::BinanceWsConnector>>,
    /// 交易消息流
    stream: Arc<Mutex<Option<a_common::ws::BinanceTradeStream>>>,
}

impl BinanceMarketStream {
    /// 创建 Binance 市场数据流 (连接 Binance Futures 测试网)
    pub async fn new(symbol: &str) -> Result<Self, a_common::MarketError> {
        let mut connector = a_common::ws::BinanceWsConnector::new(symbol);
        let stream = connector.connect().await?;

        tracing::info!("BinanceMarketStream 连接成功: {}", symbol);

        Ok(Self {
            _connector: Arc::new(Mutex::new(connector)),
            stream: Arc::new(Mutex::new(Some(stream))),
        })
    }

    /// 获取下一个 Tick (从 Binance WebSocket)
    pub async fn next_tick_from_binance(&self) -> Option<Tick> {
        let mut stream_guard = self.stream.lock().await;
        let stream = stream_guard.as_mut()?;

        let msg = stream.next_message().await?;
        let trade: a_common::ws::BinanceTradeMsg = stream.parse_trade(&msg)?;

        // 使用 str::parse 解析价格和数量 (Binance 返回的是普通小数格式如 "70434.00")
        let price: Decimal = trade.price.parse().ok()?;
        let qty: Decimal = trade.quantity.parse().ok()?;
        let timestamp = Utc.timestamp_millis_opt(trade.trade_time).single()?;

        Some(Tick {
            symbol: trade.symbol.clone(),
            price,
            qty,
            timestamp,
            kline_1m: None,
            kline_15m: None,
            kline_1d: None,
        })
    }
}

#[async_trait]
impl MarketStream for BinanceMarketStream {
    async fn next_tick(&self) -> Option<Tick> {
        self.next_tick_from_binance().await
    }
}

/// 多数据流写入器 - 使用约定的高速内存盘路径
///
/// 路径结构 (来自 a_common::Paths):
/// - Windows: E:/shm/backup/
/// - Linux: /dev/shm/backup/
///
/// 目录结构:
/// - trades/{symbol}.csv     # 成交数据
/// - depth/{symbol}.json      # 深度数据
/// - kline-1m-实时/{symbol}.json  # K线实时数据
pub struct MultiStreamWriter {
    /// 基础目录 (memory_backup_dir)
    base_dir: String,
    /// Trade 文件按品种分开
    trade_files: std::collections::HashMap<String, File>,
    /// Kline 文件按品种分开
    kline_files: std::collections::HashMap<String, File>,
    /// Depth 文件按品种分开
    depth_files: std::collections::HashMap<String, File>,
}

impl MultiStreamWriter {
    /// 使用约定的高速内存盘路径创建写入器
    pub fn new(base_dir: &str) -> std::io::Result<Self> {
        Ok(Self {
            base_dir: base_dir.to_string(),
            trade_files: std::collections::HashMap::new(),
            kline_files: std::collections::HashMap::new(),
            depth_files: std::collections::HashMap::new(),
        })
    }

    /// 确保目录存在
    fn ensure_dir(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Ok(())
    }

    /// 获取或创建 trade 文件
    fn get_trade_file(&mut self, symbol: &str) -> std::io::Result<&mut File> {
        let symbol_lower = symbol.to_lowercase();
        if !self.trade_files.contains_key(&symbol_lower) {
            let path = format!("{}/trades/{}.csv", self.base_dir, symbol_lower);
            self.ensure_dir(std::path::Path::new(&path))?;
            let file = File::create(&path)?;
            self.trade_files.insert(symbol_lower, file);
        }
        Ok(self.trade_files.get_mut(&symbol.to_lowercase()).unwrap())
    }

    /// 获取或创建 kline 文件
    fn get_kline_file(&mut self, symbol: &str) -> std::io::Result<&mut File> {
        let symbol_lower = symbol.to_lowercase();
        if !self.kline_files.contains_key(&symbol_lower) {
            let path = format!("{}/kline-1m-实时/{}.json", self.base_dir, symbol_lower);
            self.ensure_dir(std::path::Path::new(&path))?;
            let file = File::create(&path)?;
            self.kline_files.insert(symbol_lower, file);
        }
        Ok(self.kline_files.get_mut(&symbol.to_lowercase()).unwrap())
    }

    /// 获取或创建 depth 文件
    fn get_depth_file(&mut self, symbol: &str) -> std::io::Result<&mut File> {
        let symbol_lower = symbol.to_lowercase();
        if !self.depth_files.contains_key(&symbol_lower) {
            let path = format!("{}/depth/{}.json", self.base_dir, symbol_lower);
            self.ensure_dir(std::path::Path::new(&path))?;
            let file = File::create(&path)?;
            self.depth_files.insert(symbol_lower, file);
        }
        Ok(self.depth_files.get_mut(&symbol.to_lowercase()).unwrap())
    }

    /// 写入 trade 数据
    pub fn write_trade(&mut self, symbol: &str, line: &str) {
        if let Ok(ref mut f) = self.get_trade_file(symbol) {
            let _ = f.write_all(line.as_bytes());
            let _ = f.write_all(b"\n");
            let _ = f.flush();
        }
    }

    /// 写入 kline 数据
    pub fn write_kline(&mut self, symbol: &str, line: &str) {
        if let Ok(ref mut f) = self.get_kline_file(symbol) {
            let _ = f.write_all(line.as_bytes());
            let _ = f.write_all(b"\n");
            let _ = f.flush();
        }
    }

    /// 写入 depth 数据
    pub fn write_depth(&mut self, symbol: &str, line: &str) {
        if let Ok(ref mut f) = self.get_depth_file(symbol) {
            let _ = f.write_all(line.as_bytes());
            let _ = f.write_all(b"\n");
            let _ = f.flush();
        }
    }
}

/// 全市场 WebSocket 多数据流订阅
pub struct BinanceMultiStream {
    /// Combined stream (读写两端)
    stream: a_common::ws::BinanceCombinedStream,
    /// 写入器
    writer: MultiStreamWriter,
    /// 订阅的交易对
    symbols: Vec<String>,
}

impl BinanceMultiStream {
    /// 创建多数据流 - 使用约定的高速内存盘路径
    pub async fn new(symbols: Vec<String>) -> Result<Self, a_common::MarketError> {
        // 使用平台自适应路径 (E:/shm/backup 或 /dev/shm/backup)
        let base_dir = a_common::Paths::new().memory_backup_dir;

        // 构建订阅列表: <symbol>@trade, <symbol>@kline_1m, <symbol>@depth10@100ms
        let mut streams: Vec<String> = Vec::new();
        for symbol in &symbols {
            let sym = symbol.to_lowercase();
            streams.push(format!("{}@trade", sym));           // Trade 成交
            streams.push(format!("{}@kline_1m", sym));       // K线 1分钟
            streams.push(format!("{}@depth10@100ms", sym));   // 深度 10档 100ms
        }

        let url = "wss://stream.binancefuture.com/stream".to_string();

        // 使用 CombinedStream 连接
        let mut stream = a_common::ws::BinanceCombinedStream::connect(&url).await?;
        stream.subscribe(&streams).await?;

        let writer = MultiStreamWriter::new(&base_dir)
            .map_err(|e| a_common::MarketError::SerializeError(e.to_string()))?;

        tracing::info!("BinanceMultiStream connected to: {}", url);
        tracing::info!("Using memory backup dir: {}", base_dir);

        Ok(Self {
            stream,
            writer,
            symbols,
        })
    }

    /// 获取下一条消息并写入约定的高速内存盘目录
    pub async fn next_message(&mut self) -> Option<String> {
        let msg = self.stream.next_message().await?;

        // Binance combined stream 格式: {"stream":"btcusdt@kline_1m","data":{...}}
        // 根据 stream 名称判断类型并提取 symbol
        if let Ok(obj) = serde_json::from_str::<serde_json::Value>(&msg) {
            if let Some(stream_name) = obj.get("stream").and_then(|v| v.as_str()) {
                // 提取 symbol (格式: "btcusdt@kline_1m" -> "btcusdt")
                let symbol = stream_name.split('@').next().unwrap_or("unknown");

                if stream_name.contains("@trade") {
                    // 写入 trades/{symbol}.csv
                    self.writer.write_trade(symbol, &msg);
                } else if stream_name.contains("@kline_") {
                    // 写入 kline-1m-实时/{symbol}.json
                    self.writer.write_kline(symbol, &msg);
                } else if stream_name.contains("@depth") {
                    // 写入 depth/{symbol}.json
                    self.writer.write_depth(symbol, &msg);
                }
                // 忽略其他类型
            }
        }

        Some(msg)
    }
}
