//! DataFeeder - 核心数据分发器，统一管理 4 个 WebSocket 连接
//!
//! # 连接架构
//! - 连接1: KLine 1m 第一组 (约750品种)
//! - 连接2: KLine 1m 第二组 (约750品种)
//! - 连接3: KLine 1d (1500+ 品种)
//! - 连接4: Depth (默认订阅 BTCUSDT)
//!
//! # 分片订阅
//! - 每批 50 streams, 间隔 500ms

use crate::binance_ws::BinanceWsConnector;
use crate::orderbook::OrderBook;
use crate::volatility::VolatilityDetector;
use crate::symbol_registry::SymbolRegistry;
use crate::types::KLine;
use fnv::FnvHashMap;
use fnv::FnvHashSet;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tokio::time::{sleep, Duration};

const PAGINATE_SIZE: usize = 50;
const PAGINATE_INTERVAL_MS: u64 = 500;
const BINANCE_WS_BASE: &str = "wss://stream.binancefuture.com/ws";

/// DataFeeder 核心数据分发器
pub struct DataFeeder {
    /// 4 个 WebSocket 连接
    kline_1m_conn_1: RwLock<Option<BinanceWsConnector>>,
    kline_1m_conn_2: RwLock<Option<BinanceWsConnector>>,
    kline_1d_conn: RwLock<Option<BinanceWsConnector>>,
    depth_conn: RwLock<Option<BinanceWsConnector>>,

    /// 广播通道
    broadcaster: broadcast::Sender<DataMessage>,

    /// 波动率检测器 (按品种)
    volatility_detectors: Arc<RwLock<FnvHashMap<String, VolatilityDetector>>>,

    /// 订单簿 (按品种)
    orderbooks: Arc<RwLock<FnvHashMap<String, OrderBook>>>,

    /// Depth 订阅管理器
    depth_subscribed: Arc<RwLock<FnvHashSet<String>>>,
    default_depth_symbol: String,

    /// 品种注册
    symbol_registry: Arc<RwLock<SymbolRegistry>>,

    /// 状态 - 使用 Arc 包装以支持 clone
    is_initialized: Arc<RwLock<bool>>,
    last_data_time: Arc<RwLock<std::time::Instant>>,
}

/// DataFeeder 输出的数据消息
#[derive(Debug, Clone)]
pub enum DataMessage {
    /// 1分钟 K线
    KLine1m { symbol: String, kline: KLine },
    /// 日 K线
    KLine1d { symbol: String, kline: KLine },
    /// 订单簿深度
    Depth { symbol: String, orderbook: OrderBook },
}

impl DataFeeder {
    /// 创建 DataFeeder
    pub async fn new(redis_url: &str) -> Result<Self, crate::error::MarketError> {
        let symbol_registry = Arc::new(RwLock::new(
            SymbolRegistry::new(redis_url).await?,
        ));

        let (broadcaster, _) = broadcast::channel(10000);

        Ok(Self {
            kline_1m_conn_1: RwLock::new(None),
            kline_1m_conn_2: RwLock::new(None),
            kline_1d_conn: RwLock::new(None),
            depth_conn: RwLock::new(None),
            broadcaster,
            volatility_detectors: Arc::new(RwLock::new(FnvHashMap::default())),
            orderbooks: Arc::new(RwLock::new(FnvHashMap::default())),
            depth_subscribed: Arc::new(RwLock::new(FnvHashSet::default())),
            default_depth_symbol: "BTCUSDT".to_string(),
            symbol_registry,
            is_initialized: Arc::new(RwLock::new(false)),
            last_data_time: Arc::new(RwLock::new(std::time::Instant::now())),
        })
    }

    /// 启动 DataFeeder
    pub async fn start(&mut self) -> Result<(), crate::error::MarketError> {
        tracing::info!("[DataFeeder] 启动中...");

        // 1. 更新品种列表
        {
            let mut registry = self.symbol_registry.write().await;
            registry.update_symbols().await?;
        }

        let symbols = {
            let registry = self.symbol_registry.read().await;
            registry.get_trading_symbols().await
        };

        let symbol_vec: Vec<String> = symbols.into_iter().collect();
        tracing::info!("[DataFeeder] 获取到 {} 个交易品种", symbol_vec.len());

        // 2. 初始化 4 个连接
        self.init_kline_1m_connections(&symbol_vec).await?;
        self.init_kline_1d_connection(&symbol_vec).await?;
        self.init_depth_connection().await?;

        // 3. 标记初始化完成
        {
            let mut initialized = self.is_initialized.write().await;
            *initialized = true;
        }

        tracing::info!("[DataFeeder] 初始化完成");

        // 4. 启动后台任务
        let feeder = DataFeederHandle {
            is_initialized: self.is_initialized.clone(),
            last_data_time: self.last_data_time.clone(),
        };

        // 启动数据监控循环
        let handle1 = feeder.clone();
        tokio::spawn(async move {
            handle1.data_monitor_loop().await;
        });

        // 启动品种更新循环
        let handle2 = feeder.clone();
        let registry2 = self.symbol_registry.clone();
        tokio::spawn(async move {
            handle2.symbol_update_loop(registry2).await;
        });

        Ok(())
    }

    /// 建立两组 1m K线连接
    async fn init_kline_1m_connections(
        &self,
        symbols: &[String],
    ) -> Result<(), crate::error::MarketError> {
        let mid = symbols.len() / 2;
        let symbols_1 = &symbols[..mid];
        let symbols_2 = &symbols[mid..];

        tracing::info!(
            "[DataFeeder] 初始化 KLine 1m 连接, 第一组: {} 品种, 第二组: {} 品种",
            symbols_1.len(),
            symbols_2.len()
        );

        // 连接1
        {
            let streams: Vec<String> = symbols_1
                .iter()
                .map(|s| format!("{}@kline_1m", s.to_lowercase()))
                .collect();
            let conn = BinanceWsConnector::new_multi(BINANCE_WS_BASE, streams);
            let mut conn_guard = self.kline_1m_conn_1.write().await;
            *conn_guard = Some(conn);
        }

        // 连接2
        {
            let streams: Vec<String> = symbols_2
                .iter()
                .map(|s| format!("{}@kline_1m", s.to_lowercase()))
                .collect();
            let conn = BinanceWsConnector::new_multi(BINANCE_WS_BASE, streams);
            let mut conn_guard = self.kline_1m_conn_2.write().await;
            *conn_guard = Some(conn);
        }

        // 分片订阅 - 连接1
        {
            let mut conn_guard = self.kline_1m_conn_1.write().await;
            if let Some(ref mut conn) = *conn_guard {
                Self::do_subscribe_kline_batch(conn, symbols_1, "1m").await?;
            }
        }

        // 分片订阅 - 连接2
        {
            let mut conn_guard = self.kline_1m_conn_2.write().await;
            if let Some(ref mut conn) = *conn_guard {
                Self::do_subscribe_kline_batch(conn, symbols_2, "1m").await?;
            }
        }

        Ok(())
    }

    /// 建立 1d K线连接
    async fn init_kline_1d_connection(
        &self,
        symbols: &[String],
    ) -> Result<(), crate::error::MarketError> {
        tracing::info!(
            "[DataFeeder] 初始化 KLine 1d 连接, {} 品种",
            symbols.len()
        );

        {
            let streams: Vec<String> = symbols
                .iter()
                .map(|s| format!("{}@kline_1d", s.to_lowercase()))
                .collect();
            let conn = BinanceWsConnector::new_multi(BINANCE_WS_BASE, streams);
            let mut conn_guard = self.kline_1d_conn.write().await;
            *conn_guard = Some(conn);
        }

        // 分片订阅
        {
            let mut conn_guard = self.kline_1d_conn.write().await;
            if let Some(ref mut conn) = *conn_guard {
                Self::do_subscribe_kline_batch(conn, symbols, "1d").await?;
            }
        }

        Ok(())
    }

    /// 建立 Depth 连接（默认订阅 BTCUSDT）
    async fn init_depth_connection(&self) -> Result<(), crate::error::MarketError> {
        tracing::info!(
            "[DataFeeder] 初始化 Depth 连接, 默认订阅: {}",
            self.default_depth_symbol
        );

        let symbol = self.default_depth_symbol.to_lowercase();
        let streams = vec![format!("{}@depth20@100ms", symbol)];

        let conn = BinanceWsConnector::new_multi(BINANCE_WS_BASE, streams);

        {
            let mut conn_guard = self.depth_conn.write().await;
            *conn_guard = Some(conn);
        }

        // 添加到已订阅列表
        {
            let mut subscribed = self.depth_subscribed.write().await;
            subscribed.insert(self.default_depth_symbol.clone());
        }

        // 初始化该品种的 OrderBook
        {
            let mut orderbooks = self.orderbooks.write().await;
            orderbooks.insert(
                self.default_depth_symbol.clone(),
                OrderBook::new(self.default_depth_symbol.clone()),
            );
        }

        Ok(())
    }

    /// 执行分片订阅 K线
    async fn do_subscribe_kline_batch(
        conn: &mut BinanceWsConnector,
        symbols: &[String],
        interval: &str,
    ) -> Result<(), crate::error::MarketError> {
        let streams: Vec<String> = symbols
            .iter()
            .map(|s| format!("{}@kline_{}", s.to_lowercase(), interval))
            .collect();

        // 分片订阅
        for chunk in streams.chunks(PAGINATE_SIZE) {
            conn.subscribe(chunk).await?;
            sleep(Duration::from_millis(PAGINATE_INTERVAL_MS)).await;
        }

        tracing::info!(
            "[DataFeeder] {} K线订阅完成, 共 {} 批",
            interval,
            (streams.len() + PAGINATE_SIZE - 1) / PAGINATE_SIZE
        );

        Ok(())
    }

    /// 添加 Depth 订阅
    pub async fn add_depth_subscription(&self, symbol: &str) -> Result<(), crate::error::MarketError> {
        let should_subscribe = {
            let subscribed = self.depth_subscribed.read().await;
            !subscribed.contains(symbol)
        };

        if !should_subscribe {
            tracing::debug!("[DataFeeder] {} 已在 Depth 订阅中", symbol);
            return Ok(());
        }

        // 订阅
        {
            let mut conn_guard = self.depth_conn.write().await;
            if let Some(ref mut conn) = *conn_guard {
                let stream = format!("{}@depth20@100ms", symbol.to_lowercase());
                conn.subscribe(&[stream]).await?;
            }
        }

        // 更新已订阅列表
        {
            let mut subscribed = self.depth_subscribed.write().await;
            subscribed.insert(symbol.to_string());
        }

        // 初始化该品种的 OrderBook
        {
            let mut orderbooks = self.orderbooks.write().await;
            orderbooks.insert(symbol.to_string(), OrderBook::new(symbol.to_string()));
        }

        tracing::info!("[DataFeeder] 添加 Depth 订阅: {}", symbol);

        Ok(())
    }

    /// 移除 Depth 订阅
    pub async fn remove_depth_subscription(
        &self,
        symbol: &str,
    ) -> Result<(), crate::error::MarketError> {
        let should_unsubscribe = {
            let subscribed = self.depth_subscribed.read().await;
            subscribed.contains(symbol)
        };

        if !should_unsubscribe {
            tracing::debug!("[DataFeeder] {} 不在 Depth 订阅中", symbol);
            return Ok(());
        }

        // 退订
        {
            let mut conn_guard = self.depth_conn.write().await;
            if let Some(ref mut conn) = *conn_guard {
                let stream = format!("{}@depth20@100ms", symbol.to_lowercase());
                conn.unsubscribe(&[stream]).await?;
            }
        }

        // 更新已订阅列表
        {
            let mut subscribed = self.depth_subscribed.write().await;
            subscribed.remove(symbol);
        }

        tracing::info!("[DataFeeder] 移除 Depth 订阅: {}", symbol);

        Ok(())
    }

    /// 返回广播接收器
    pub fn subscribe(&self) -> broadcast::Receiver<DataMessage> {
        self.broadcaster.clone().subscribe()
    }

    /// 更新波动率检测器
    pub async fn update_volatility(&self, symbol: &str, price: rust_decimal::Decimal, timestamp: chrono::DateTime<chrono::Utc>) -> crate::types::VolatilityStats {
        let mut detectors = self.volatility_detectors.write().await;

        let detector = detectors
            .entry(symbol.to_string())
            .or_insert_with(|| VolatilityDetector::new(symbol.to_string()));

        detector.update(price, timestamp)
    }

    /// 检查是否应该添加/移除 Depth 订阅（基于波动率）
    pub async fn check_depth_subscription(&self, symbol: &str, vol_stats: crate::types::VolatilityStats) {
        if vol_stats.is_high_volatility {
            if let Err(e) = self.add_depth_subscription(symbol).await {
                tracing::error!("[DataFeeder] 添加 Depth 订阅失败: {}", e);
            }
        } else {
            // 非高波动时，保留默认订阅 BTCUSDT，移除其他
            if symbol != self.default_depth_symbol {
                if let Err(e) = self.remove_depth_subscription(symbol).await {
                    tracing::error!("[DataFeeder] 移除 Depth 订阅失败: {}", e);
                }
            }
        }
    }
}

/// DataFeeder 句柄，用于后台任务
#[derive(Clone)]
struct DataFeederHandle {
    is_initialized: Arc<RwLock<bool>>,
    last_data_time: Arc<RwLock<std::time::Instant>>,
}

impl DataFeederHandle {
    /// 数据监控循环（60秒告警，仅初始化后）
    async fn data_monitor_loop(&self) {
        tracing::info!("[DataFeeder] 数据监控循环已启动");

        loop {
            sleep(Duration::from_secs(60)).await;

            let is_initialized = {
                let guard = self.is_initialized.read().await;
                *guard
            };

            if !is_initialized {
                continue;
            }

            let elapsed = {
                let guard = self.last_data_time.read().await;
                guard.elapsed().as_secs()
            };

            if elapsed >= 60 {
                tracing::warn!("[DataFeeder] 警告: 超过 60 秒没有收到数据! 上次数据: {} 秒前", elapsed);
            }
        }
    }

    /// 品种更新循环（2分钟）
    async fn symbol_update_loop(&self, registry: Arc<RwLock<SymbolRegistry>>) {
        tracing::info!("[DataFeeder] 品种更新循环已启动");

        loop {
            sleep(Duration::from_secs(120)).await;

            let needs_update = {
                let guard = registry.read().await;
                guard.needs_update()
            };

            if needs_update {
                tracing::info!("[DataFeeder] 检测到品种需要更新");
                let mut guard = registry.write().await;
                if let Err(e) = guard.update_symbols().await {
                    tracing::error!("[DataFeeder] 品种更新失败: {}", e);
                }
            }
        }
    }
}

impl Clone for DataFeeder {
    fn clone(&self) -> Self {
        Self {
            // 连接不支持 clone，需要重新创建
            kline_1m_conn_1: RwLock::new(None),
            kline_1m_conn_2: RwLock::new(None),
            kline_1d_conn: RwLock::new(None),
            depth_conn: RwLock::new(None),
            // 广播通道 sender 可以 clone
            broadcaster: self.broadcaster.clone(),
            // per-connection 状态需要重新创建
            volatility_detectors: Arc::new(RwLock::new(FnvHashMap::default())),
            orderbooks: Arc::new(RwLock::new(FnvHashMap::default())),
            depth_subscribed: Arc::new(RwLock::new(FnvHashSet::default())),
            default_depth_symbol: self.default_depth_symbol.clone(),
            symbol_registry: self.symbol_registry.clone(),
            is_initialized: self.is_initialized.clone(),
            last_data_time: self.last_data_time.clone(),
        }
    }
}
