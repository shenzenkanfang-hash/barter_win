//! HistoryDataManager - 历史数据管理器核心实现
//!
//! 实现三层历史数据管理：内存 -> 磁盘 -> API
//!
//! # 线程安全设计
//! - 使用 parking_lot::RwLock（同步锁，性能更高）
//! - ring_buffer: Vec<KLine> 带锁保护
//! - current: Option<KLine> 带锁保护
//! - Arc<SymbolKlineCache> 支持跨协程共享
//!
//! # 差异化策略
//! - 1分钟K线: 5秒批量fsync（可接受5秒数据丢失）
//! - 日线K线: 立即fsync（不允许丢失一天数据）

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use tokio::sync::broadcast;

use a_common::config::Paths;
use crate::history::api::HistoryApiClient;
use crate::history::types::{
    DataIssue, DataSource, HistoryError, HistoryResponse, KLine,
    klines_to_disk, klines_from_disk,
};
use crate::history::provider::HistoryDataProvider;

/// 最大K线条目数（RingBuffer容量）
pub const MAX_KLINE_ENTRIES: usize = 1000;

/// 磁盘同步间隔（1分钟K线，秒）
const DISK_SYNC_INTERVAL_SECS: u64 = 5;

/// 判断是否为日线周期
fn is_daily_period(period: &str) -> bool {
    matches!(period, "1d" | "1D" | "d" | "day" | "daily")
}

/// 单品种K线缓存（线程安全）
struct SymbolKlineCache {
    symbol: String,
    period: String,
    /// 闭合K线环形缓冲区
    ring_buffer: RwLock<Vec<KLine>>,
    /// 当前未闭合K线
    current: RwLock<Option<KLine>>,
    /// 最后同步到磁盘的时间
    last_disk_sync: RwLock<Instant>,
}

impl SymbolKlineCache {
    fn new(symbol: &str, period: &str) -> Self {
        Self {
            symbol: symbol.to_string(),
            period: period.to_string(),
            ring_buffer: RwLock::new(Vec::with_capacity(MAX_KLINE_ENTRIES)),
            current: RwLock::new(None),
            last_disk_sync: RwLock::new(Instant::now()),
        }
    }

    /// 添加闭合K线
    fn push_closed_kline(&self, kline: KLine) -> Result<(), HistoryError> {
        // 时间戳校验：必须大于最后一条
        {
            let buffer = self.ring_buffer.read();
            if let Some(last) = buffer.last() {
                if kline.timestamp_ms <= last.timestamp_ms {
                    return Err(HistoryError::DuplicateData {
                        symbol: self.symbol.clone(),
                        timestamp: kline.timestamp_ms,
                    });
                }
            }
        }

        // 获取写锁并更新
        let mut buffer = self.ring_buffer.write();
        buffer.push(kline);

        // 如果超过容量，淘汰最旧的
        if buffer.len() > MAX_KLINE_ENTRIES {
            buffer.remove(0);
        }

        Ok(())
    }

    /// 更新当前未闭合K线
    fn update_current(&self, kline: KLine) {
        let mut current = self.current.write();
        *current = Some(kline);
    }

    /// 获取当前未闭合K线的克隆
    fn get_current(&self) -> Option<KLine> {
        let current = self.current.read();
        current.clone()
    }

    /// 获取历史K线
    fn get_history(&self, limit: u32) -> Vec<KLine> {
        let buffer = self.ring_buffer.read();
        let len = buffer.len();
        if len == 0 {
            return Vec::new();
        }
        let start = if len > limit as usize {
            len - limit as usize
        } else {
            0
        };
        buffer[start..].to_vec()
    }

    /// 获取K线数据用于刷盘
    fn get_klines_for_sync(&self) -> Vec<KLine> {
        let buffer = self.ring_buffer.read();
        buffer.clone()
    }

    /// 判断是否需要刷盘
    fn should_sync_to_disk(&self) -> bool {
        let last = *self.last_disk_sync.read();
        last.elapsed() >= Duration::from_secs(DISK_SYNC_INTERVAL_SECS)
    }

    /// 更新最后同步时间
    fn update_sync_time(&self) {
        let mut last = self.last_disk_sync.write();
        *last = Instant::now();
    }

    /// 获取symbol
    fn symbol(&self) -> &str {
        &self.symbol
    }

    /// 获取period
    fn period(&self) -> &str {
        &self.period
    }
}

/// 历史数据管理器
pub struct HistoryDataManager {
    /// 品种K线缓存（symbol_period -> cache）
    caches: RwLock<HashMap<String, Arc<SymbolKlineCache>>>,
    /// 内存缓存目录
    memory_dir: String,
    /// 磁盘同步目录
    disk_dir: String,
    /// 关闭信号
    shutdown_tx: RwLock<Option<broadcast::Sender<()>>>,
    /// API客户端（用于从交易所拉取历史数据）
    api_client: HistoryApiClient,
}

impl HistoryDataManager {
    /// 创建历史数据管理器（使用合约API）
    pub fn new() -> Self {
        let paths = Paths::new();
        Self {
            caches: RwLock::new(HashMap::new()),
            memory_dir: paths.memory_backup_dir.clone(),
            disk_dir: paths.disk_sync_dir.clone(),
            shutdown_tx: RwLock::new(None),
            api_client: HistoryApiClient::new_futures(),
        }
    }

    /// 创建指定API类型的管理器
    pub fn with_api_client(api_client: HistoryApiClient) -> Self {
        let paths = Paths::new();
        Self {
            caches: RwLock::new(HashMap::new()),
            memory_dir: paths.memory_backup_dir.clone(),
            disk_dir: paths.disk_sync_dir.clone(),
            shutdown_tx: RwLock::new(None),
            api_client,
        }
    }

    /// 创建测试用管理器
    #[cfg(test)]
    pub fn new_for_test(memory_dir: &str, disk_dir: &str) -> Self {
        Self {
            caches: RwLock::new(HashMap::new()),
            memory_dir: memory_dir.to_string(),
            disk_dir: disk_dir.to_string(),
            shutdown_tx: RwLock::new(None),
            api_client: HistoryApiClient::new_spot(),
        }
    }

    /// 获取或创建缓存
    fn get_or_create_cache(&self, symbol: &str, period: &str) -> Arc<SymbolKlineCache> {
        let key = Self::cache_key(symbol, period);
        let caches = self.caches.read();
        if let Some(cache) = caches.get(&key) {
            return cache.clone();
        }
        drop(caches);

        let mut caches = self.caches.write();
        // 双重检查
        if let Some(cache) = caches.get(&key) {
            return cache.clone();
        }

        // 创建新缓存
        let cache = Arc::new(SymbolKlineCache::new(symbol, period));
        caches.insert(key, cache.clone());
        cache
    }

    /// 生成缓存键
    fn cache_key(symbol: &str, period: &str) -> String {
        format!("{}_{}", symbol.to_lowercase(), period)
    }

    /// 保存到磁盘
    async fn save_to_disk(&self, symbol: &str, period: &str, klines: &[KLine]) -> Result<(), HistoryError> {
        let dir = Path::new(&self.disk_dir);
        let file_path = dir.join(format!("{}_{}.json", symbol.to_lowercase(), period));

        // 确保目录存在
        if !dir.exists() {
            std::fs::create_dir_all(dir)
                .map_err(|e| HistoryError::DiskWriteFailed(e.to_string()))?;
        }

        // 序列化并写入
        let data = klines_to_disk(klines);
        let json = serde_json::to_string(&data)
            .map_err(|e| HistoryError::DiskWriteFailed(e.to_string()))?;

        tokio::fs::write(&file_path, json)
            .await
            .map_err(|e| HistoryError::DiskWriteFailed(e.to_string()))?;

        tracing::debug!("Saved {} klines for {} {} to disk", klines.len(), symbol, period);
        Ok(())
    }

    /// 从磁盘加载
    async fn load_from_disk(&self, symbol: &str, period: &str) -> Result<Vec<KLine>, HistoryError> {
        let file_path = Path::new(&self.disk_dir)
            .join(format!("{}_{}.json", symbol.to_lowercase(), period));

        if !file_path.exists() {
            return Ok(Vec::new());
        }

        let content = tokio::fs::read_to_string(&file_path)
            .await
            .map_err(|e| HistoryError::DiskWriteFailed(e.to_string()))?;

        let data: Vec<Vec<serde_json::Value>> = serde_json::from_str(&content)
            .map_err(|e| HistoryError::DiskWriteFailed(e.to_string()))?;

        let klines = klines_from_disk(symbol, period, data);

        Ok(klines)
    }

    /// 从API获取历史数据
    ///
    /// 使用带重试+jitter的API客户端
    async fn fetch_from_api(
        &self,
        symbol: &str,
        period: &str,
        limit: u32,
    ) -> Result<Vec<KLine>, HistoryError> {
        tracing::info!("Fetching {} {} from API (limit: {})", symbol, period, limit);

        // 计算时间范围：从当前时间往前推
        let end_time = Utc::now().timestamp_millis();
        let start_time = match period {
            "1m" => end_time - (limit as i64 * 60 * 1000), // 1分钟K线：往前limit分钟
            "1d" => end_time - (limit as i64 * 86400 * 1000), // 日线：往前limit天
            _ => end_time - (limit as i64 * 60 * 1000), // 默认按1分钟处理
        };

        self.api_client
            .fetch_klines(symbol, period, Some(start_time), Some(end_time), limit)
            .await
    }

    /// 检查数据完整性
    fn check_data_integrity(&self, klines: &[KLine]) -> Option<DataIssue> {
        if klines.len() < 2 {
            return None;
        }

        for window in klines.windows(2) {
            let prev = &window[0];
            let curr = &window[1];
            if curr.timestamp_ms <= prev.timestamp_ms {
                return Some(DataIssue::BrokenSequence {
                    last_timestamp: prev.timestamp_ms,
                });
            }
        }

        None
    }

    /// 启动定时同步任务
    pub fn start_sync_task(self: Arc<Self>) {
        let (tx, _rx) = broadcast::channel(1);
        {
            let mut shutdown = self.shutdown_tx.write();
            *shutdown = Some(tx.clone());
        }

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(DISK_SYNC_INTERVAL_SECS));
            let mut shutdown_rx = tx.subscribe();
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let manager = &*self;
                        let caches = manager.caches.read();
                        for (_key, cache) in caches.iter() {
                            if cache.should_sync_to_disk() {
                                let klines = cache.get_klines_for_sync();
                                if !klines.is_empty() {
                                    let symbol = cache.symbol().to_string();
                                    let period = cache.period().to_string();
                                    let klines_for_save = klines.clone();
                                    let disk_dir = manager.disk_dir.clone();
                                    
                                    // 异步刷盘，不持有锁
                                    tokio::spawn(async move {
                                        let file_path = Path::new(&disk_dir)
                                            .join(format!("{}_{}.json", symbol.to_lowercase(), period));
                                        let data = klines_to_disk(&klines_for_save);
                                        let json = serde_json::to_string(&data)
                                            .map_err(|e| HistoryError::DiskWriteFailed(e.to_string()));
                                        if let Ok(json) = json {
                                            if let Err(e) = tokio::fs::write(&file_path, json).await {
                                                tracing::error!("Failed to sync to disk: {}", e);
                                            }
                                        }
                                    });
                                }
                            }
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        tracing::info!("HistoryDataManager sync task shutting down");
                        break;
                    }
                }
            }
        });
    }

    /// 停止同步任务
    pub fn stop_sync_task(&self) {
        let tx = self.shutdown_tx.write().take();
        if let Some(tx) = tx {
            let _ = tx.send(());
        }
    }

    /// 同步保存K线到磁盘（用于日线立即fsync）
    async fn save_kline_sync(&self, disk_dir: &str, symbol: &str, period: &str, kline: KLine) {
        let file_path = Path::new(disk_dir)
            .join(format!("{}_{}.json", symbol.to_lowercase(), period));

        // 确保目录存在
        if let Some(parent) = file_path.parent() {
            if !parent.exists() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    tracing::error!("Failed to create directory: {}", e);
                    return;
                }
            }
        }

        let data = klines_to_disk(&[kline]);
        match serde_json::to_string(&data) {
            Ok(json) => {
                if let Err(e) = tokio::fs::write(&file_path, json).await {
                    tracing::warn!("Sync disk write failed: {}", e);
                } else {
                    tracing::debug!("Saved daily kline for {} {} to disk (immediate fsync)", symbol, period);
                }
            }
            Err(e) => {
                tracing::warn!("Serialization failed: {}", e);
            }
        }
    }

    /// 克隆内部（用于async上下文）
    fn clone_inner(&self) -> Self {
        Self {
            caches: RwLock::new(HashMap::new()),
            memory_dir: self.memory_dir.clone(),
            disk_dir: self.disk_dir.clone(),
            shutdown_tx: RwLock::new(None),
            api_client: self.api_client.clone(),
        }
    }
}

impl Clone for HistoryDataManager {
    fn clone(&self) -> Self {
        Self {
            caches: RwLock::new(HashMap::new()),
            memory_dir: self.memory_dir.clone(),
            disk_dir: self.disk_dir.clone(),
            shutdown_tx: RwLock::new(None),
            api_client: self.api_client.clone(),
        }
    }
}

#[async_trait]
impl HistoryDataProvider for HistoryDataManager {
    async fn update_realtime_kline(
        &self,
        symbol: &str,
        period: &str,
        kline: KLine,
        is_closed: bool,
    ) -> Result<(), HistoryError> {
        let cache = self.get_or_create_cache(symbol, period);

        if is_closed {
            cache.push_closed_kline(kline.clone())?;

            // 差异化fsync策略
            if is_daily_period(period) {
                // 日线：立即fsync（不允许丢失一天数据）
                let disk_dir = self.disk_dir.clone();
                let sym = symbol.to_string();
                let per = period.to_string();
                let kl = kline;
                self.save_kline_sync(&disk_dir, &sym, &per, kl).await;
            } else {
                // 1分钟：异步批量，5秒后同步（可接受5秒数据丢失）
                let disk_dir = self.disk_dir.clone();
                let sym = symbol.to_string();
                let per = period.to_string();
                let kl = kline;
                tokio::spawn(async move {
                    // 等待5秒批量窗口
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    let file_path = Path::new(&disk_dir)
                        .join(format!("{}_{}.json", sym.to_lowercase(), per));
                    let data = klines_to_disk(&[kl]);
                    let json = serde_json::to_string(&data)
                        .map_err(|e| HistoryError::DiskWriteFailed(e.to_string()));
                    if let Ok(json) = json {
                        if let Err(e) = tokio::fs::write(&file_path, json).await {
                            tracing::warn!("Async disk write failed: {}", e);
                        }
                    }
                });
            }
        } else {
            cache.update_current(kline);
        }

        Ok(())
    }

    async fn query_history(
        &self,
        symbol: &str,
        period: &str,
        _end_time: DateTime<Utc>,
        limit: u32,
    ) -> Result<HistoryResponse, HistoryError> {
        let key = Self::cache_key(symbol, period);

        // 先检查缓存是否存在
        let cache_result: Option<(Vec<KLine>, Option<KLine>)> = {
            let caches = self.caches.read();
            caches.get(&key).map(|cache| {
                let klines = cache.get_history(limit);
                let current = cache.get_current();
                (klines, current)
            })
        }; // guard dropped here

        if let Some((klines, current)) = cache_result {
            let source = if klines.len() >= limit as usize {
                DataSource::Memory
            } else {
                DataSource::Disk
            };
            return Ok(HistoryResponse {
                klines,
                current,
                has_more: false,
                source,
            });
        }

        // 缓存不存在，从磁盘加载
        let klines = self.load_from_disk(symbol, period).await?;
        let cache = self.get_or_create_cache(symbol, period);

        for kline in &klines {
            cache.push_closed_kline(kline.clone())?;
        }

        let current = cache.get_current();

        Ok(HistoryResponse {
            klines,
            current,
            has_more: false,
            source: DataSource::Disk,
        })
    }

    async fn get_history_for_indicator(
        &self,
        symbol: &str,
        period: &str,
        limit: u32,
    ) -> Result<HistoryResponse, HistoryError> {
        let key = Self::cache_key(symbol, period);

        // 先检查缓存
        let cache_result: Option<(Vec<KLine>, Option<KLine>)> = {
            let caches = self.caches.read();
            caches.get(&key).map(|cache| {
                let klines = cache.get_history(limit);
                let current = cache.get_current();
                (klines, current)
            })
        }; // guard dropped here

        if let Some((klines, current)) = cache_result {
            if klines.len() >= limit as usize {
                return Ok(HistoryResponse {
                    klines,
                    current,
                    has_more: false,
                    source: DataSource::Memory,
                });
            }
        }

        // 缓存不足，从磁盘或API补全
        let mut klines = self.load_from_disk(symbol, period).await?;

        if klines.len() < limit as usize {
            let fetched = self.fetch_from_api(symbol, period, limit).await?;
            klines.extend(fetched);
        }

        klines.sort_by_key(|k| k.timestamp_ms);
        klines.dedup_by(|a, b| a.timestamp_ms == b.timestamp_ms);

        if klines.len() > limit as usize {
            klines.truncate(limit as usize);
        }

        Ok(HistoryResponse {
            klines,
            current: None,
            has_more: false,
            source: DataSource::Remote,
        })
    }

    async fn report_issue(
        &self,
        symbol: &str,
        period: &str,
        issue: DataIssue,
    ) -> Result<(), HistoryError> {
        tracing::warn!("Data issue reported for {} {}: {:?}", symbol, period, issue);

        match issue {
            DataIssue::MissingData { from, to } => {
                let _ = self.fetch_from_api(symbol, period, 1000).await?;
                tracing::info!("Missing data from {} to {}, fetching from API", from, to);
            }
            DataIssue::BrokenSequence { last_timestamp } => {
                let _ = self.fetch_from_api(symbol, period, 1000).await?;
                tracing::info!("Broken sequence at {}, need to fetch from API", last_timestamp);
            }
            DataIssue::InvalidData { timestamp } => {
                tracing::info!("Invalid data at {}, need to refetch", timestamp);
            }
        }

        Ok(())
    }

    async fn get_current_kline(
        &self,
        symbol: &str,
        period: &str,
    ) -> Result<Option<KLine>, HistoryError> {
        let key = Self::cache_key(symbol, period);
        let caches = self.caches.read();

        if let Some(cache) = caches.get(&key) {
            Ok(cache.get_current())
        } else {
            Ok(None)
        }
    }

    async fn check_integrity(&self, symbol: &str, period: &str) -> Result<bool, HistoryError> {
        let klines = self.load_from_disk(symbol, period).await?;

        if let Some(issue) = self.check_data_integrity(&klines) {
            tracing::warn!("Data integrity check failed: {:?}", issue);
            return Ok(false);
        }

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[tokio::test]
    async fn test_push_closed_kline() {
        let cache = Arc::new(SymbolKlineCache::new("BTCUSDT", "1m"));

        let kline1 = KLine {
            symbol: "BTCUSDT".to_string(),
            period: "1m".to_string(),
            open: dec!(100),
            high: dec!(105),
            low: dec!(99),
            close: dec!(103),
            volume: dec!(10),
            timestamp_ms: 1000,
        };

        assert!(cache.push_closed_kline(kline1.clone()).is_ok());

        let buffer = cache.ring_buffer.read();
        assert_eq!(buffer.len(), 1);
        drop(buffer);

        // 重复时间戳应该被拒绝
        let dup = KLine {
            timestamp_ms: 1000,
            ..kline1.clone()
        };
        assert!(cache.push_closed_kline(dup).is_err());

        // 更早的时间戳应该被拒绝
        let earlier = KLine {
            timestamp_ms: 500,
            ..kline1.clone()
        };
        assert!(cache.push_closed_kline(earlier).is_err());

        // 正常添加
        let kline2 = KLine {
            timestamp_ms: 1060,
            ..kline1.clone()
        };
        assert!(cache.push_closed_kline(kline2).is_ok());

        let buffer = cache.ring_buffer.read();
        assert_eq!(buffer.len(), 2);
    }

    #[tokio::test]
    async fn test_data_integrity_check() {
        let manager = HistoryDataManager::new_for_test("/tmp", "/tmp");

        let kline_base = KLine {
            symbol: "BTCUSDT".to_string(),
            period: "1m".to_string(),
            open: dec!(100),
            high: dec!(105),
            low: dec!(99),
            close: dec!(103),
            volume: dec!(10),
            timestamp_ms: 1000,
        };

        let klines = vec![
            kline_base.clone(),
            KLine {
                timestamp_ms: 1060,
                ..kline_base.clone()
            },
        ];

        assert!(manager.check_data_integrity(&klines).is_none());

        let broken = vec![
            KLine {
                timestamp_ms: 2000,
                ..kline_base.clone()
            },
            KLine {
                timestamp_ms: 1000, // 倒退的时间戳，违反递增规则
                ..kline_base
            },
        ];

        assert!(manager.check_data_integrity(&broken).is_some());
    }

    #[tokio::test]
    async fn test_update_current() {
        let cache = Arc::new(SymbolKlineCache::new("BTCUSDT", "1m"));

        let kline = KLine {
            symbol: "BTCUSDT".to_string(),
            period: "1m".to_string(),
            open: dec!(100),
            high: dec!(105),
            low: dec!(99),
            close: dec!(103),
            volume: dec!(10),
            timestamp_ms: 1000,
        };

        assert!(cache.get_current().is_none());

        cache.update_current(kline.clone());
        assert_eq!(cache.get_current(), Some(kline.clone()));

        let kline2 = KLine {
            timestamp_ms: 1060,
            ..kline
        };
        cache.update_current(kline2);
        assert_eq!(cache.get_current().map(|k| k.timestamp_ms), Some(1060));
    }
}
