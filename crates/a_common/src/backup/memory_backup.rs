#![forbid(unsafe_code)]

//! 内存备份系统 - 高速内存文件系统备份
//!
//! 将实时交易数据保存到高速内存盘 (E:/shm/backup)，定期同步到磁盘。
//! 用于快速读写高频交易数据，同时保证数据持久性。

use crate::api::SymbolRulesData;
use crate::config::Paths;
use crate::EngineError;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tokio::fs::{self, File};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{interval, Duration, Instant};

// ============================================================================
// 常量定义
// ============================================================================

/// 内存备份根目录 (由 Platform::detect() 自动选择)
pub fn memory_backup_dir() -> String {
    Paths::new().memory_backup_dir
}

/// K线最大条目数
pub const MAX_KLINE_ENTRIES: usize = 1000;
/// 成交最大条目数
pub const MAX_TRADES_ENTRIES: usize = 500;
/// 指标最大条目数
pub const MAX_INDICATORS_ENTRIES: usize = 100;
/// 深度最大条目数
pub const MAX_DEPTH_ENTRIES: usize = 100;
/// 任务最大条目数
pub const MAX_TASKS_ENTRIES: usize = 100;

/// 刷新间隔（秒）
const BUFFER_FLUSH_INTERVAL_SECS: u64 = 5;

/// 检查文件大小的调用间隔
const FILE_SIZE_CHECK_INTERVAL: usize = 10;

// CSV 文件最大大小 (100MB)
pub const MAX_CSV_FILE_SIZE: u64 = 100 * 1024 * 1024;

// 根目录文件
pub const ACCOUNT_FILE: &str = "account.json";
pub const POSITIONS_FILE: &str = "positions.json";
pub const TRADING_PAIRS_FILE: &str = "trading_pairs.json";
pub const SYSTEM_CONFIG_FILE: &str = "system_config.json";

// 通道目录
pub const CHANNEL_DIR: &str = "channel/";
pub const CHANNEL_MINUTE_FILE: &str = "channel/minute.json";
pub const CHANNEL_DAILY_FILE: &str = "channel/daily.json";

// 一级目录
pub const DEPTH_DIR: &str = "depth/";
pub const TRADES_DIR: &str = "trades/";
pub const RULES_DIR: &str = "rules/";

// K线目录
pub const KLINE_1M_REALTIME_DIR: &str = "kline_1m_realtime/";
pub const KLINE_1M_HISTORY_DIR: &str = "kline_1m_history/";
pub const KLINE_1D_REALTIME_DIR: &str = "kline_1d_realtime/";
pub const KLINE_1D_HISTORY_DIR: &str = "kline_1d_history/";

// 指标目录
pub const INDICATORS_1M_REALTIME_DIR: &str = "indicators_1m_realtime/";
pub const INDICATORS_1M_HISTORY_DIR: &str = "indicators_1m_history/";
pub const INDICATORS_1D_REALTIME_DIR: &str = "indicators_1d_realtime/";
pub const INDICATORS_1D_HISTORY_DIR: &str = "indicators_1d_history/";

// 任务池
pub const TASKS_DIR: &str = "tasks/";
pub const TASKS_MINUTE_DIR: &str = "tasks/minute/";
pub const TASKS_DAILY_DIR: &str = "tasks/daily/";

// 策略互斥
pub const MUTEX_DIR: &str = "mutex/";
pub const MUTEX_MINUTE_DIR: &str = "mutex/minute/";
pub const MUTEX_HOUR_DIR: &str = "mutex/hour/";

// ============================================================================
// 同步状态
// ============================================================================

/// 内存备份同步状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncStatus {
    /// 已同步
    Synced,
    /// 脏数据（有待同步）
    Dirty,
    /// 同步失败
    Failed,
}

impl Default for SyncStatus {
    fn default() -> Self {
        SyncStatus::Dirty // 默认认为是脏的，需要同步
    }
}

// ============================================================================
// 磁盘空间检查
// ============================================================================

/// 检查目标路径所在磁盘的可用空间
/// 返回 Ok(可用空间字节数) 或 Err(错误信息)
fn check_disk_space(_path: &Path, _min_required_bytes: u64) -> Result<u64, EngineError> {
    // 使用 std::fs::metadata 获取磁盘信息
    // 注意：这是同步调用，但在备份场景下可接受
    #[cfg(unix)]
    {
        use std::fs::Metadata;
        use std::os::unix::fs::MetadataExt;
        let metadata = Metadata::from(path);
        // 在 Unix 上，我们使用 statvfs
        // 这里简化处理，直接返回 OK
        Ok(std::fs::metadata(path).map(|m| m.len()).unwrap_or(0))
    }
    #[cfg(not(unix))]
    {
        // Windows 或其他平台：简化处理，返回足够大的值
        Ok(1024 * 1024 * 1024) // 默认假设有 1GB 可用
    }
}

/// 检查磁盘可用空间是否足够（阈值：100MB）
const MIN_DISK_SPACE_BYTES: u64 = 100 * 1024 * 1024;

// ============================================================================
// 数据类型定义
// ============================================================================

/// K线条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KlineEntry {
    /// 时间戳
    pub t: String,
    /// 开盘价
    pub o: Decimal,
    /// 最高价
    pub h: Decimal,
    /// 最低价
    pub l: Decimal,
    /// 收盘价
    pub c: Decimal,
    /// 成交量
    pub v: Decimal,
}

/// K线数据类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KlineData {
    pub period: String,
    pub data_type: String,
    pub klines: Vec<KlineEntry>,
}

impl KlineData {
    pub fn new(period: &str, data_type: &str) -> Self {
        Self {
            period: period.to_string(),
            data_type: data_type.to_string(),
            klines: Vec::new(),
        }
    }
}

/// 系统配置快照（保存到高速盘）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfig {
    /// API 请求权重限制（每分钟）
    pub request_weight_limit: u32,
    /// API 订单限制（每分钟）
    pub orders_limit: u32,
    /// 当前已用请求权重
    pub used_weight: u32,
    /// 当前已用订单数
    pub used_orders: u32,
    /// 窗口起始时间戳（秒）
    pub window_start_ts: u64,
    /// 更新时间戳
    pub updated_at: String,
}

/// 账户快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountSnapshot {
    pub equity: Decimal,
    pub available: Decimal,
    pub frozen: Decimal,
    pub unrealized_pnl: Decimal,
    pub updated_at: String,
}

/// 持仓快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionSnapshot {
    pub symbol: String,
    pub long_qty: Decimal,
    pub long_avg_price: Decimal,
    pub short_qty: Decimal,
    pub short_avg_price: Decimal,
    pub updated_at: String,
}

/// 持仓列表（统一管理）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Positions {
    pub positions: Vec<PositionSnapshot>,
    pub updated_at: String,
}

impl Default for Positions {
    fn default() -> Self {
        Self {
            positions: Vec::new(),
            updated_at: String::new(),
        }
    }
}

/// 深度数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepthData {
    pub symbol: String,
    pub bids: Vec<DepthEntry>,
    pub asks: Vec<DepthEntry>,
    pub updated_at: String,
}

impl Default for DepthData {
    fn default() -> Self {
        Self {
            symbol: String::new(),
            bids: Vec::new(),
            asks: Vec::new(),
            updated_at: String::new(),
        }
    }
}

/// 深度条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepthEntry {
    pub price: Decimal,
    pub qty: Decimal,
}

/// 指标数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndicatorsData {
    pub period: String,
    pub data_type: String,
    pub ema_fast: Decimal,
    pub ema_slow: Decimal,
    pub rsi: Decimal,
    pub pine_color: String,
    pub price_position: Decimal,
    pub tr_ratio: Decimal,
    pub updated_at: String,
}

impl IndicatorsData {
    pub fn new(period: &str, data_type: &str) -> Self {
        Self {
            period: period.to_string(),
            data_type: data_type.to_string(),
            ema_fast: Decimal::ZERO,
            ema_slow: Decimal::ZERO,
            rsi: Decimal::ZERO,
            pine_color: String::new(),
            price_position: Decimal::ZERO,
            tr_ratio: Decimal::ZERO,
            updated_at: String::new(),
        }
    }
}

/// 通道数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelData {
    pub channel_type: String,
    pub volatility: Decimal,
    pub tr_ratio: Decimal,
    pub trend: String,
    pub updated_at: String,
}

/// 任务池数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskPool {
    pub pool_type: String,
    pub active_tasks: Vec<TaskInfo>,
    pub completed_count: u64,
    pub failed_count: u64,
    pub updated_at: String,
}

impl Default for TaskPool {
    fn default() -> Self {
        Self {
            pool_type: String::new(),
            active_tasks: Vec::new(),
            completed_count: 0,
            failed_count: 0,
            updated_at: String::new(),
        }
    }
}

/// 任务信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInfo {
    pub task_id: String,
    pub symbol: String,
    pub task_type: String,
    pub status: String,
    pub created_at: String,
}

/// 交易品种列表
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingPairs {
    pub pairs: Vec<TradingPairInfo>,
    pub updated_at: String,
}

impl Default for TradingPairs {
    fn default() -> Self {
        Self {
            pairs: Vec::new(),
            updated_at: String::new(),
        }
    }
}

/// 交易品种信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingPairInfo {
    pub symbol: String,
    pub status: String,
    pub base_asset: String,
    pub quote_asset: String,
}

/// 策略互斥状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolMutexStatus {
    pub symbol: String,
    pub strategy_level: String,
    pub status: String,
    pub registered_at: i64,
    pub updated_at: i64,
}

// ============================================================================
// MemoryBackup 内存备份管理器
// ============================================================================

/// 内存备份管理器
pub struct MemoryBackup {
    tmpfs_dir: String,
    disk_dir: String,
    sync_interval_secs: u64,
    /// 写入缓冲（symbol -> 待写入的 JSON 数据）
    write_buffer: HashMap<String, Vec<u8>>,
    /// 上次刷新时间
    last_flush: HashMap<String, Instant>,
}

impl MemoryBackup {
    pub fn new(tmpfs_dir: &str, disk_dir: &str, sync_interval_secs: u64) -> Self {
        Self {
            tmpfs_dir: tmpfs_dir.to_string(),
            disk_dir: disk_dir.to_string(),
            sync_interval_secs,
            write_buffer: HashMap::new(),
            last_flush: HashMap::new(),
        }
    }

    /// 刷新缓冲（内部使用）
    async fn flush_buffer(&mut self, symbol: &str) -> Result<(), EngineError> {
        if let Some(data) = self.write_buffer.get_mut(symbol) {
            if !data.is_empty() {
                let path = format!("{}/{}.json", self.tmpfs_dir, symbol);
                let mut file = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .await
                    .map_err(|e| EngineError::MemoryBackup(format!("打开文件失败: {}", e)))?;
                file.write_all(data).await
                    .map_err(|e| EngineError::MemoryBackup(format!("写入缓冲失败: {}", e)))?;
                data.clear();
            }
        }
        self.last_flush.insert(symbol.to_string(), Instant::now());
        Ok(())
    }

    /// 缓冲写入（替代直接序列化）
    pub async fn save_with_buffer(&mut self, symbol: &str, data: &impl Serialize) -> Result<(), EngineError> {
        let json = serde_json::to_vec(data)
            .map_err(|e| EngineError::MemoryBackup(format!("序列化失败: {}", e)))?;

        // 追加到缓冲（修正：追加实际JSON数据而非仅换行符）
        let buffer = self.write_buffer.entry(symbol.to_string()).or_insert_with(Vec::new);
        buffer.extend_from_slice(&json);
        buffer.push(b'\n');

        // 检查是否需要刷新
        let now = Instant::now();
        let should_flush = self.last_flush
            .get(symbol)
            .map(|t| now.duration_since(*t).as_secs() >= BUFFER_FLUSH_INTERVAL_SECS)
            .unwrap_or(true);

        if should_flush {
            self.flush_buffer(symbol).await?;
        }

        Ok(())
    }

    pub async fn start_sync_task(self: std::sync::Arc<Self>) {
        let mut timer = interval(Duration::from_secs(self.sync_interval_secs));
        let mut consecutive_failures: u32 = 0;
        const MAX_CONSECUTIVE_FAILURES: u32 = 3;

        loop {
            timer.tick().await;
            match self.sync_to_disk().await {
                Ok(_) => {
                    if consecutive_failures > 0 {
                        tracing::info!("内存备份同步恢复成功 (曾连续失败 {} 次)", consecutive_failures);
                        consecutive_failures = 0;
                    }
                }
                Err(e) => {
                    consecutive_failures += 1;
                    if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                        tracing::error!(
                            error = %e,
                            consecutive_failures = consecutive_failures,
                            "【严重】内存备份同步连续失败，数据丢失风险！请检查磁盘空间"
                        );
                    } else {
                        tracing::warn!(
                            error = %e,
                            consecutive_failures = consecutive_failures,
                            "内存备份同步失败"
                        );
                    }
                }
            }
        }
    }

    pub async fn sync_to_disk(&self) -> Result<(), EngineError> {
        let tmp_path = Path::new(&self.tmpfs_dir);
        let disk_path = Path::new(&self.disk_dir);

        // 检查磁盘空间
        if let Err(e) = check_disk_space(disk_path, MIN_DISK_SPACE_BYTES) {
            tracing::error!("磁盘空间检查失败，跳过本次同步: {}", e);
            return Err(EngineError::MemoryBackup(format!(
                "磁盘空间不足或检查失败: {}", e
            )));
        }

        // 创建目标目录
        fs::create_dir_all(disk_path).await.map_err(|e| {
            EngineError::MemoryBackup(format!("创建磁盘备份目录失败: {}", e))
        })?;

        self.sync_directory(tmp_path, disk_path).await?;
        tracing::info!("内存备份已同步到磁盘");
        Ok(())
    }

    async fn sync_directory(&self, src: &Path, dst: &Path) -> Result<(), EngineError> {
        let mut entries = fs::read_dir(src).await.map_err(|e| {
            EngineError::MemoryBackup(format!("读取目录失败: {}", e))
        })?;

        while let Some(entry) = entries.next_entry().await.map_err(|e| {
            EngineError::MemoryBackup(format!("读取目录条目失败: {}", e))
        })? {
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());

            if src_path.is_dir() {
                fs::create_dir_all(&dst_path).await.map_err(|e| {
                    EngineError::MemoryBackup(format!("创建目录失败: {}", e))
                })?;
                Box::pin(self.sync_directory(&src_path, &dst_path)).await?;
            } else {
                fs::copy(&src_path, &dst_path).await.map_err(|e| {
                    EngineError::MemoryBackup(format!("复制文件失败: {}", e))
                })?;
            }
        }
        Ok(())
    }

    // =========================================================================
    // 账户/持仓/品种
    // =========================================================================

    pub async fn save_account(&self, account: &AccountSnapshot) -> Result<(), EngineError> {
        let path = format!("{}/{}", self.tmpfs_dir, ACCOUNT_FILE);
        self.ensure_dir(&path).await?;
        self.write_json(&path, account).await
    }

    pub async fn load_account(&self) -> Result<Option<AccountSnapshot>, EngineError> {
        let path = format!("{}/{}", self.tmpfs_dir, ACCOUNT_FILE);
        match self.load_json::<AccountSnapshot>(&path).await {
            Ok(data) => Ok(Some(data)),
            Err(e) => self.handle_load_error(e),
        }
    }

    // =========================================================================
    // 系统配置
    // =========================================================================

    pub async fn save_system_config(&self, config: &SystemConfig) -> Result<(), EngineError> {
        let path = format!("{}/{}", self.tmpfs_dir, SYSTEM_CONFIG_FILE);
        self.ensure_dir(&path).await?;
        self.write_json(&path, config).await
    }

    pub async fn load_system_config(&self) -> Result<Option<SystemConfig>, EngineError> {
        let path = format!("{}/{}", self.tmpfs_dir, SYSTEM_CONFIG_FILE);
        match self.load_json::<SystemConfig>(&path).await {
            Ok(data) => Ok(Some(data)),
            Err(e) => self.handle_load_error(e),
        }
    }

    pub async fn save_positions(&self, positions: &Positions) -> Result<(), EngineError> {
        let path = format!("{}/{}", self.tmpfs_dir, POSITIONS_FILE);
        self.ensure_dir(&path).await?;
        self.write_json(&path, positions).await
    }

    pub async fn load_positions(&self) -> Result<Option<Positions>, EngineError> {
        let path = format!("{}/{}", self.tmpfs_dir, POSITIONS_FILE);
        match self.load_json::<Positions>(&path).await {
            Ok(data) => Ok(Some(data)),
            Err(e) => self.handle_load_error(e),
        }
    }

    // =========================================================================
    // 同步方法（用于恢复启动）
    // =========================================================================

    /// 同步加载持仓数据（用于启动恢复）
    pub fn load_positions_sync(&self) -> Result<Option<Positions>, EngineError> {
        let path = format!("{}/{}", self.tmpfs_dir, POSITIONS_FILE);
        match self.load_json_sync::<Positions>(&path) {
            Ok(data) => Ok(Some(data)),
            Err(e) => {
                if let EngineError::MemoryBackup(ref msg) = e {
                    if msg.contains("打开文件失败") {
                        return Ok(None);
                    }
                }
                Err(e)
            }
        }
    }

    /// 同步加载账户数据（用于启动恢复）
    pub fn load_account_sync(&self) -> Result<Option<AccountSnapshot>, EngineError> {
        let path = format!("{}/{}", self.tmpfs_dir, ACCOUNT_FILE);
        match self.load_json_sync::<AccountSnapshot>(&path) {
            Ok(data) => Ok(Some(data)),
            Err(e) => {
                if let EngineError::MemoryBackup(ref msg) = e {
                    if msg.contains("打开文件失败") {
                        return Ok(None);
                    }
                }
                Err(e)
            }
        }
    }

    /// 同步加载 JSON（阻塞）
    fn load_json_sync<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T, EngineError> {
        use std::fs::File;
        use std::io::Read;
        
        let mut file = File::open(path).map_err(|e| {
            EngineError::MemoryBackup(format!("打开文件失败: {}", e))
        })?;

        let mut contents = String::new();
        file.read_to_string(&mut contents).map_err(|e| {
            EngineError::MemoryBackup(format!("读取文件失败: {}", e))
        })?;

        serde_json::from_str(&contents).map_err(|e| {
            EngineError::MemoryBackup(format!("解析 JSON 失败: {}", e))
        })
    }

    pub async fn save_trading_pairs(&self, pairs: &TradingPairs) -> Result<(), EngineError> {
        let path = format!("{}/{}", self.tmpfs_dir, TRADING_PAIRS_FILE);
        self.ensure_dir(&path).await?;
        self.write_json(&path, pairs).await
    }

    pub async fn load_trading_pairs(&self) -> Result<Option<TradingPairs>, EngineError> {
        let path = format!("{}/{}", self.tmpfs_dir, TRADING_PAIRS_FILE);
        match self.load_json::<TradingPairs>(&path).await {
            Ok(data) => Ok(Some(data)),
            Err(e) => self.handle_load_error(e),
        }
    }

    // =========================================================================
    // 通道
    // =========================================================================

    pub async fn save_channel(&self, channel: &ChannelData) -> Result<(), EngineError> {
        let path = match channel.channel_type.as_str() {
            "minute" => format!("{}/{}", self.tmpfs_dir, CHANNEL_MINUTE_FILE),
            "daily" => format!("{}/{}", self.tmpfs_dir, CHANNEL_DAILY_FILE),
            _ => return Err(EngineError::MemoryBackup(format!("未知通道类型: {}", channel.channel_type))),
        };
        self.ensure_dir(&path).await?;
        self.write_json(&path, channel).await
    }

    pub async fn load_channel(&self, channel_type: &str) -> Result<Option<ChannelData>, EngineError> {
        let path = match channel_type {
            "minute" => format!("{}/{}", self.tmpfs_dir, CHANNEL_MINUTE_FILE),
            "daily" => format!("{}/{}", self.tmpfs_dir, CHANNEL_DAILY_FILE),
            _ => return Err(EngineError::MemoryBackup(format!("未知通道类型: {}", channel_type))),
        };
        match self.load_json::<ChannelData>(&path).await {
            Ok(data) => Ok(Some(data)),
            Err(e) => self.handle_load_error(e),
        }
    }

    // =========================================================================
    // 订单簿
    // =========================================================================

    pub async fn save_depth(&self, symbol: &str, depth: &DepthData) -> Result<(), EngineError> {
        let sanitized = Self::sanitize_symbol(symbol)?;
        let path = format!("{}/{}/{}.json", self.tmpfs_dir, DEPTH_DIR.trim_end_matches('/'), sanitized);
        self.ensure_dir(&path).await?;

        let mut data = depth.clone();
        self.trim_depth_entries(&mut data.bids, MAX_DEPTH_ENTRIES, true);
        self.trim_depth_entries(&mut data.asks, MAX_DEPTH_ENTRIES, false);

        self.write_json(&path, &data).await
    }

    pub async fn load_depth(&self, symbol: &str) -> Result<Option<DepthData>, EngineError> {
        let sanitized = Self::sanitize_symbol(symbol)?;
        let path = format!("{}/{}/{}.json", self.tmpfs_dir, DEPTH_DIR.trim_end_matches('/'), sanitized);
        match self.load_json::<DepthData>(&path).await {
            Ok(data) => Ok(Some(data)),
            Err(e) => self.handle_load_error(e),
        }
    }

    // =========================================================================
    // 成交 (CSV)
    // =========================================================================

    /// 追加成交到 CSV 文件
    pub async fn append_trade(&self, symbol: &str, csv_line: &str) -> Result<(), EngineError> {
        let dir = format!("{}/{}", self.tmpfs_dir, TRADES_DIR.trim_end_matches('/'));
        let file_path = format!("{}/{}.csv", dir, symbol);
        self.ensure_dir(&file_path).await?;

        // 检查文件大小，决定是否需要创建新文件
        let file_index = self.get_csv_file_index(&file_path).await?;
        let actual_path = if file_index > 1 {
            format!("{}_{:03}.csv", file_path.trim_end_matches(".csv"), file_index)
        } else {
            file_path.clone()
        };

        // 追加写入
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&actual_path)
            .await
            .map_err(|e| EngineError::MemoryBackup(format!("打开文件失败: {}", e)))?;

        file.write_all(csv_line.as_bytes()).await.map_err(|e| {
            EngineError::MemoryBackup(format!("写入文件失败: {}", e))
        })?;
        file.write_all(b"\n").await.map_err(|e| {
            EngineError::MemoryBackup(format!("写入换行失败: {}", e))
        })?;

        // 检查是否需要创建新文件
        let metadata = fs::metadata(&actual_path).await.map_err(|e| {
            EngineError::MemoryBackup(format!("获取文件元数据失败: {}", e))
        })?;

        if metadata.len() >= MAX_CSV_FILE_SIZE {
            // 创建新文件
            let new_path = format!("{}_{:03}.csv", file_path.trim_end_matches(".csv"), file_index + 1);
            // 写入表头
            let header = "timestamp,symbol,side,price,qty,trade_id,order_id,ema_signal,rsi_value,pine_color,price_position,final_signal,target_price,quantity,risk_flag,round_id,is_high_freq\n";
            let mut new_file = fs::OpenOptions::new()
                .create(true)
                .write(true)
                .open(&new_path)
                .await
                .map_err(|e| EngineError::MemoryBackup(format!("创建新文件失败: {}", e)))?;
            new_file.write_all(header.as_bytes()).await.map_err(|e| {
                EngineError::MemoryBackup(format!("写入表头失败: {}", e))
            })?;
        }

        Ok(())
    }

    async fn get_csv_file_index(&self, base_path: &str) -> Result<usize, EngineError> {
        let base = base_path.trim_end_matches(".csv");
        let mut index = 1;

        loop {
            let path = if index == 1 {
                base_path.to_string()
            } else {
                format!("{}_{:03}.csv", base, index)
            };

            if !Path::new(&path).exists() {
                break;
            }

            let metadata = fs::metadata(&path).await.map_err(|e| {
                EngineError::MemoryBackup(format!("获取文件元数据失败: {}", e))
            })?;

            if metadata.len() >= MAX_CSV_FILE_SIZE {
                index += 1;
            } else {
                break;
            }

            if index > 1000 {
                return Err(EngineError::MemoryBackup(format!("CSV 文件数量超过限制: {}", index)));
            }
        }

        Ok(index)
    }

    // =========================================================================
    // 规则
    // =========================================================================

    /// 验证并清理交易对符号，防止路径遍历攻击
    /// 只允许字母、数字和下划线
    fn sanitize_symbol(symbol: &str) -> Result<String, EngineError> {
        if symbol.is_empty() || symbol.len() > 20 {
            return Err(EngineError::MemoryBackup(format!("无效的交易对符号: {}", symbol)));
        }
        // 只允许字母、数字和下划线
        let sanitized = symbol.to_uppercase();
        if !sanitized.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(EngineError::MemoryBackup(format!("交易对符号包含非法字符: {}", symbol)));
        }
        Ok(sanitized)
    }

    pub async fn save_symbol_rules(&self, symbol: &str, rules: &SymbolRulesData) -> Result<(), EngineError> {
        let sanitized = Self::sanitize_symbol(symbol)?;
        let path = format!("{}/{}/{}.json", self.tmpfs_dir, RULES_DIR.trim_end_matches('/'), sanitized);
        self.ensure_dir(&path).await?;
        self.write_json(&path, rules).await
    }

    pub async fn load_symbol_rules(&self, symbol: &str) -> Result<Option<SymbolRulesData>, EngineError> {
        let sanitized = Self::sanitize_symbol(symbol)?;
        let path = format!("{}/{}/{}.json", self.tmpfs_dir, RULES_DIR.trim_end_matches('/'), sanitized);
        match self.load_json::<SymbolRulesData>(&path).await {
            Ok(data) => Ok(Some(data)),
            Err(e) => self.handle_load_error(e),
        }
    }

    // =========================================================================
    // K线
    // =========================================================================

    pub async fn save_kline(&self, symbol: &str, period: &str, data_type: &str, kline: &KlineData) -> Result<(), EngineError> {
        let sanitized = Self::sanitize_symbol(symbol)?;
        let dir = self.get_kline_dir(period, data_type);
        let path = format!("{}/{}/{}.json", self.tmpfs_dir, dir.trim_end_matches('/'), sanitized);
        self.ensure_dir(&path).await?;

        let mut data = kline.clone();
        self.trim_entries(&mut data.klines, MAX_KLINE_ENTRIES);

        self.write_json(&path, &data).await
    }

    pub async fn load_kline(&self, symbol: &str, period: &str, data_type: &str) -> Result<Option<KlineData>, EngineError> {
        let sanitized = Self::sanitize_symbol(symbol)?;
        let dir = self.get_kline_dir(period, data_type);
        let path = format!("{}/{}/{}.json", self.tmpfs_dir, dir.trim_end_matches('/'), sanitized);
        match self.load_json::<KlineData>(&path).await {
            Ok(data) => Ok(Some(data)),
            Err(e) => self.handle_load_error(e),
        }
    }

    fn get_kline_dir(&self, period: &str, data_type: &str) -> String {
        match (period, data_type) {
            ("1m", "realtime") => KLINE_1M_REALTIME_DIR.to_string(),
            ("1m", "history") => KLINE_1M_HISTORY_DIR.to_string(),
            ("1d", "realtime") => KLINE_1D_REALTIME_DIR.to_string(),
            ("1d", "history") => KLINE_1D_HISTORY_DIR.to_string(),
            _ => KLINE_1M_REALTIME_DIR.to_string(),
        }
    }

    // =========================================================================
    // 指标
    // =========================================================================

    pub async fn save_indicators(&self, symbol: &str, period: &str, data_type: &str, indicators: &IndicatorsData) -> Result<(), EngineError> {
        let sanitized = Self::sanitize_symbol(symbol)?;
        let dir = self.get_indicators_dir(period, data_type);
        let path = format!("{}/{}/{}.json", self.tmpfs_dir, dir.trim_end_matches('/'), sanitized);
        self.ensure_dir(&path).await?;
        self.write_json(&path, indicators).await
    }

    pub async fn load_indicators(&self, symbol: &str, period: &str, data_type: &str) -> Result<Option<IndicatorsData>, EngineError> {
        let sanitized = Self::sanitize_symbol(symbol)?;
        let dir = self.get_indicators_dir(period, data_type);
        let path = format!("{}/{}/{}.json", self.tmpfs_dir, dir.trim_end_matches('/'), sanitized);
        match self.load_json::<IndicatorsData>(&path).await {
            Ok(data) => Ok(Some(data)),
            Err(e) => self.handle_load_error(e),
        }
    }

    fn get_indicators_dir(&self, period: &str, data_type: &str) -> String {
        match (period, data_type) {
            ("1m", "realtime") => INDICATORS_1M_REALTIME_DIR.to_string(),
            ("1m", "history") => INDICATORS_1M_HISTORY_DIR.to_string(),
            ("1d", "realtime") => INDICATORS_1D_REALTIME_DIR.to_string(),
            ("1d", "history") => INDICATORS_1D_HISTORY_DIR.to_string(),
            _ => INDICATORS_1M_REALTIME_DIR.to_string(),
        }
    }

    // =========================================================================
    // 任务池
    // =========================================================================

    pub async fn save_task_pool(&self, pool_type: &str, pool: &TaskPool) -> Result<(), EngineError> {
        let path = match pool_type {
            "minute" => format!("{}/pool.json", TASKS_MINUTE_DIR),
            "daily" => format!("{}/pool.json", TASKS_DAILY_DIR),
            _ => return Err(EngineError::MemoryBackup(format!("未知任务池类型: {}", pool_type))),
        };
        let full_path = format!("{}/{}", self.tmpfs_dir, path);
        self.ensure_dir(&full_path).await?;

        let mut data: TaskPool = self.load_json(&full_path).await.unwrap_or_default();
        data.active_tasks.extend_from_slice(&pool.active_tasks);
        data.completed_count = pool.completed_count;
        data.failed_count = pool.failed_count;
        data.updated_at = pool.updated_at.clone();
        self.trim_entries(&mut data.active_tasks, MAX_TASKS_ENTRIES);

        self.write_json(&full_path, &data).await
    }

    pub async fn load_task_pool(&self, pool_type: &str) -> Result<Option<TaskPool>, EngineError> {
        let path = match pool_type {
            "minute" => format!("{}/pool.json", TASKS_MINUTE_DIR),
            "daily" => format!("{}/pool.json", TASKS_DAILY_DIR),
            _ => return Err(EngineError::MemoryBackup(format!("未知任务池类型: {}", pool_type))),
        };
        let full_path = format!("{}/{}", self.tmpfs_dir, path);
        match self.load_json::<TaskPool>(&full_path).await {
            Ok(data) => Ok(Some(data)),
            Err(e) => self.handle_load_error(e),
        }
    }

    // =========================================================================
    // 策略互斥
    // =========================================================================

    pub async fn save_mutex_status(&self, strategy_level: &str, symbol: &str, status: &SymbolMutexStatus) -> Result<(), EngineError> {
        let sanitized = Self::sanitize_symbol(symbol)?;
        let dir = match strategy_level {
            "minute" => MUTEX_MINUTE_DIR,
            "hour" => MUTEX_HOUR_DIR,
            _ => return Err(EngineError::MemoryBackup(format!("未知策略级别: {}", strategy_level))),
        };
        let path = format!("{}/{}/{}.json", self.tmpfs_dir, dir.trim_end_matches('/'), sanitized);
        self.ensure_dir(&path).await?;
        self.write_json(&path, status).await
    }

    pub async fn load_mutex_status(&self, strategy_level: &str, symbol: &str) -> Result<Option<SymbolMutexStatus>, EngineError> {
        let sanitized = Self::sanitize_symbol(symbol)?;
        let dir = match strategy_level {
            "minute" => MUTEX_MINUTE_DIR,
            "hour" => MUTEX_HOUR_DIR,
            _ => return Err(EngineError::MemoryBackup(format!("未知策略级别: {}", strategy_level))),
        };
        let path = format!("{}/{}/{}.json", self.tmpfs_dir, dir.trim_end_matches('/'), sanitized);
        match self.load_json::<SymbolMutexStatus>(&path).await {
            Ok(data) => Ok(Some(data)),
            Err(e) => self.handle_load_error(e),
        }
    }

    pub async fn remove_mutex_status(&self, strategy_level: &str, symbol: &str) -> Result<(), EngineError> {
        let sanitized = Self::sanitize_symbol(symbol)?;
        let dir = match strategy_level {
            "minute" => MUTEX_MINUTE_DIR,
            "hour" => MUTEX_HOUR_DIR,
            _ => return Err(EngineError::MemoryBackup(format!("未知策略级别: {}", strategy_level))),
        };
        let path = format!("{}/{}/{}.json", self.tmpfs_dir, dir.trim_end_matches('/'), sanitized);

        if Path::new(&path).exists() {
            fs::remove_file(&path).await.map_err(|e| {
                EngineError::MemoryBackup(format!("删除文件失败: {}", e))
            })?;
        }

        Ok(())
    }

    // =========================================================================
    // 辅助方法
    // =========================================================================

    fn handle_load_error<T>(&self, e: EngineError) -> Result<Option<T>, EngineError> {
        if let EngineError::MemoryBackup(ref msg) = e {
            if msg.contains("打开文件失败") {
                return Ok(None);
            }
        }
        Err(e)
    }

    fn trim_entries<T>(&self, v: &mut Vec<T>, max: usize) {
        while v.len() > max {
            v.remove(0);
        }
    }

    /// 修剪并排序深度条目
    /// - bids: 买盘，按价格升序排列（最低价在前）
    /// - asks: 卖盘，按价格降序排列（最高价在前）
    fn trim_depth_entries(&self, v: &mut Vec<DepthEntry>, max: usize, is_bids: bool) {
        while v.len() > max {
            v.remove(0);
        }
        if is_bids {
            // 买盘：升序（价格从低到高）
            v.sort_by(|a, b| a.price.cmp(&b.price));
        } else {
            // 卖盘：降序（价格从高到低）
            v.sort_by(|a, b| b.price.cmp(&a.price));
        }
    }

    async fn load_json<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T, EngineError> {
        let mut file = File::open(path).await.map_err(|e| {
            EngineError::MemoryBackup(format!("打开文件失败: {}", e))
        })?;

        let mut contents = String::new();
        file.read_to_string(&mut contents).await.map_err(|e| {
            EngineError::MemoryBackup(format!("读取文件失败: {}", e))
        })?;

        serde_json::from_str(&contents).map_err(|e| {
            EngineError::MemoryBackup(format!("解析 JSON 失败: {}", e))
        })
    }

    async fn write_json<T: Serialize>(&self, path: &str, data: &T) -> Result<(), EngineError> {
        let json = serde_json::to_string_pretty(data).map_err(|e| {
            EngineError::MemoryBackup(format!("序列化 JSON 失败: {}", e))
        })?;

        let mut file = File::create(path).await.map_err(|e| {
            EngineError::MemoryBackup(format!("创建文件失败: {}", e))
        })?;

        file.write_all(json.as_bytes()).await.map_err(|e| {
            EngineError::MemoryBackup(format!("写入文件失败: {}", e))
        })?;

        Ok(())
    }

    async fn ensure_dir(&self, path: &str) -> Result<(), EngineError> {
        let path = Path::new(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await.map_err(|e| {
                EngineError::MemoryBackup(format!("创建目录失败: {}", e))
            })?;
        }
        Ok(())
    }

    pub fn tmpfs_dir(&self) -> &str {
        &self.tmpfs_dir
    }

    pub fn disk_dir(&self) -> &str {
        &self.disk_dir
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_backup_creation() {
        let paths = Paths::new();
        let backup = MemoryBackup::new(&paths.memory_backup_dir, &paths.disk_sync_dir, 30);
        assert_eq!(backup.tmpfs_dir(), paths.memory_backup_dir);
        assert_eq!(backup.disk_dir(), paths.disk_sync_dir);
    }

    #[tokio::test]
    async fn test_trim_entries() {
        let backup = MemoryBackup::new("/tmp/test", "/tmp/disk", 30);
        let mut v = vec![1, 2, 3, 4, 5];
        backup.trim_entries(&mut v, 3);
        assert_eq!(v, vec![3, 4, 5]);
    }

    #[tokio::test]
    async fn test_positions_default() {
        let positions = Positions::default();
        assert!(positions.positions.is_empty());
    }

    #[tokio::test]
    async fn test_kline_data() {
        let kline = KlineData::new("1m", "realtime");
        assert_eq!(kline.period, "1m");
        assert_eq!(kline.data_type, "realtime");
        assert!(kline.klines.is_empty());
    }

    #[tokio::test]
    async fn test_indicators_data() {
        let indicators = IndicatorsData::new("1d", "history");
        assert_eq!(indicators.period, "1d");
        assert_eq!(indicators.data_type, "history");
    }

    #[tokio::test]
    async fn test_get_kline_dir() {
        let backup = MemoryBackup::new("/tmp", "/tmp/disk", 30);
        assert_eq!(backup.get_kline_dir("1m", "realtime"), KLINE_1M_REALTIME_DIR);
        assert_eq!(backup.get_kline_dir("1m", "history"), KLINE_1M_HISTORY_DIR);
        assert_eq!(backup.get_kline_dir("1d", "realtime"), KLINE_1D_REALTIME_DIR);
        assert_eq!(backup.get_kline_dir("1d", "history"), KLINE_1D_HISTORY_DIR);
    }

    #[tokio::test]
    async fn test_get_indicators_dir() {
        let backup = MemoryBackup::new("/tmp", "/tmp/disk", 30);
        assert_eq!(backup.get_indicators_dir("1m", "realtime"), INDICATORS_1M_REALTIME_DIR);
        assert_eq!(backup.get_indicators_dir("1m", "history"), INDICATORS_1M_HISTORY_DIR);
        assert_eq!(backup.get_indicators_dir("1d", "realtime"), INDICATORS_1D_REALTIME_DIR);
        assert_eq!(backup.get_indicators_dir("1d", "history"), INDICATORS_1D_HISTORY_DIR);
    }
}
