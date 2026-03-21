#![forbid(unsafe_code)]

//! 内存备份系统 - 高速内存文件系统备份
//!
//! 将实时交易数据保存到高速内存盘 (E:/shm/backup)，定期同步到磁盘。
//! 用于快速读写高频交易数据，同时保证数据持久性。
//!
//! # 目录结构
//!
//! ```ignore
//! E:/shm/backup/
//! ├── account.json              # 账户信息
//! ├── trading_pairs.json        # 交易品种列表
//! ├── symbol_rules.json         # 规则汇总
//! │
//! ├── channel/                  # 类别为主
//! │   ├── minute.json           # 分钟通道
//! │   └── daily.json            # 日线通道
//! │
//! ├── symbols/                  # 品种为主（通用数据）
//! │   ├── btcusdt/
//! │   │   ├── kxian.json
//! │   │   ├── depth.json
//! │   │   ├── trades.json
//! │   │   ├── indicators.json
//! │   │   └── position.json
//! │   └── ethusdt/
//! │
//! └── tasks/                    # 任务池单独目录
//!     ├── minute/
//!     │   ├── pool.json         # 分钟池总览
//!     │   └── {symbol}.json
//!     └── daily/
//!         ├── pool.json
//!         └── {symbol}.json
//! ```

use crate::error::EngineError;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs::{self, File};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{interval, Duration};

// ============================================================================
// 常量定义
// ============================================================================

/// 内存备份根目录 (由 Platform::detect() 自动选择)
pub fn memory_backup_dir() -> String {
    crate::platform::Paths::new().memory_backup_dir
}

/// K线最大条目数
const MAX_KXIAN_ENTRIES: usize = 1000;
/// 成交最大条目数
const MAX_TRADES_ENTRIES: usize = 500;
/// 指标最大条目数
const MAX_INDICATORS_ENTRIES: usize = 100;
/// 订单最大条目数
const MAX_ORDERS_ENTRIES: usize = 200;
/// 深度最大条目数
const MAX_DEPTH_ENTRIES: usize = 100;
/// 任务最大条目数
const MAX_TASKS_ENTRIES: usize = 100;

// 单文件（根目录）
const ACCOUNT_FILE: &str = "account.json";
const TRADING_PAIRS_FILE: &str = "trading_pairs.json";
const SYMBOL_RULES_FILE: &str = "symbol_rules.json";

// 类别为主
const CHANNEL_DIR: &str = "channel/";
const CHANNEL_MINUTE_FILE: &str = "channel/minute.json";
const CHANNEL_DAILY_FILE: &str = "channel/daily.json";

// 品种为主
const SYMBOLS_DIR: &str = "symbols/";

// 任务池
const TASKS_DIR: &str = "tasks/";
const TASKS_MINUTE_DIR: &str = "tasks/minute/";
const TASKS_DAILY_DIR: &str = "tasks/daily/";

// ============================================================================
// 数据类型定义
// ============================================================================

/// K线数据（多周期）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KxianData {
    pub m1: Vec<KlineEntry>,
    pub m5: Vec<KlineEntry>,
    pub m15: Vec<KlineEntry>,
    pub d1: Vec<KlineEntry>,
}

impl Default for KxianData {
    fn default() -> Self {
        Self {
            m1: Vec::new(),
            m5: Vec::new(),
            m15: Vec::new(),
            d1: Vec::new(),
        }
    }
}

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

/// 账户快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountSnapshot {
    /// 账户权益
    pub equity: Decimal,
    /// 可用资金
    pub available: Decimal,
    /// 冻结资金
    pub frozen: Decimal,
    /// 未实现盈亏
    pub unrealized_pnl: Decimal,
    /// 更新时间
    pub updated_at: String,
}

/// 持仓快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionSnapshot {
    /// 交易对
    pub symbol: String,
    /// 多头数量
    pub long_qty: Decimal,
    /// 多头均价
    pub long_avg_price: Decimal,
    /// 空头数量
    pub short_qty: Decimal,
    /// 空头均价
    pub short_avg_price: Decimal,
    /// 更新时间
    pub updated_at: String,
}

/// 订单快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderSnapshot {
    /// 订单ID
    pub order_id: String,
    /// 交易对
    pub symbol: String,
    /// 方向
    pub side: String,
    /// 数量
    pub qty: Decimal,
    /// 价格
    pub price: Decimal,
    /// 状态
    pub status: String,
    /// 创建时间
    pub created_at: String,
}

/// 交易快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeSnapshot {
    /// 交易ID
    pub trade_id: String,
    /// 订单ID
    pub order_id: String,
    /// 交易对
    pub symbol: String,
    /// 价格
    pub price: Decimal,
    /// 数量
    pub qty: Decimal,
    /// 成交时间
    pub executed_at: String,
}

/// 交易规则数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolRulesData {
    /// 交易对
    pub symbol: String,
    /// 价格精度
    pub price_precision: i32,
    /// 数量精度
    pub quantity_precision: i32,
    /// 步长
    pub tick_size: Decimal,
    /// 最小数量
    pub min_qty: Decimal,
    /// 步长数量
    pub step_size: Decimal,
    /// 最小名义价值
    pub min_notional: Decimal,
    /// 最大名义价值
    pub max_notional: Decimal,
    /// 杠杆
    pub leverage: i32,
    /// 做市商费率
    pub maker_fee: Decimal,
    /// 吃单费率
    pub taker_fee: Decimal,
}

/// 深度数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepthData {
    pub bids: Vec<DepthEntry>,
    pub asks: Vec<DepthEntry>,
}

impl Default for DepthData {
    fn default() -> Self {
        Self {
            bids: Vec::new(),
            asks: Vec::new(),
        }
    }
}

/// 深度条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepthEntry {
    pub price: Decimal,
    pub qty: Decimal,
}

/// 实时成交数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealtimeTradesData {
    pub trades: Vec<RealtimeTradeEntry>,
}

impl Default for RealtimeTradesData {
    fn default() -> Self {
        Self {
            trades: Vec::new(),
        }
    }
}

/// 实时成交条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealtimeTradeEntry {
    /// 成交ID
    pub t: String,
    /// 价格
    pub p: Decimal,
    /// 数量
    pub q: Decimal,
    /// 时间
    pub time: String,
    /// 是否做多
    pub is_buyerMaker: bool,
}

/// 指标数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndicatorsData {
    pub ema_fast: Decimal,
    pub ema_slow: Decimal,
    pub rsi: Decimal,
    pub pine_color: String,
    pub price_position: Decimal,
    pub tr_ratio: Decimal,
    pub updated_at: String,
}

// ============================================================================
// 通道数据类型
// ============================================================================

/// 通道数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelData {
    pub channel_type: String,
    pub volatility: Decimal,
    pub tr_ratio: Decimal,
    pub trend: String,
    pub updated_at: String,
}

// ============================================================================
// 任务池数据类型
// ============================================================================

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

// ============================================================================
// 交易品种列表
// ============================================================================

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

// ============================================================================
// 规则汇总
// ============================================================================

/// 规则汇总
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolRulesSummary {
    pub rules: Vec<SymbolRulesData>,
    pub updated_at: String,
}

impl Default for SymbolRulesSummary {
    fn default() -> Self {
        Self {
            rules: Vec::new(),
            updated_at: String::new(),
        }
    }
}

// ============================================================================
// K线缓存（用于内存操作）
// ============================================================================

/// K线缓存（内存中的 K线数据）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KxianCache {
    pub m1: Vec<KlineEntry>,
    pub m5: Vec<KlineEntry>,
    pub m15: Vec<KlineEntry>,
    pub d1: Vec<KlineEntry>,
}

impl Default for KxianCache {
    fn default() -> Self {
        Self {
            m1: Vec::new(),
            m5: Vec::new(),
            m15: Vec::new(),
            d1: Vec::new(),
        }
    }
}

// ============================================================================
// MemoryBackup 内存备份管理器
// ============================================================================

/// 内存备份管理器
///
/// 将实时交易数据保存到高速内存盘 (E:/shm/backup)，定期同步到磁盘。
///
/// # 设计原则
/// - 高频数据写入高速内存盘，避免磁盘 IO 瓶颈
/// - 定期同步到磁盘，保证数据持久性
/// - 限制内存使用，防止无限增长
pub struct MemoryBackup {
    /// 高速内存盘目录 (如 E:/shm/backup/)
    tmpfs_dir: String,
    /// 磁盘备份目录 (如 E:/backup/sync/)
    disk_dir: String,
    /// 同步间隔（秒）
    sync_interval_secs: u64,
}

impl MemoryBackup {
    /// 创建内存备份管理器
    ///
    /// # 参数
    /// * `tmpfs_dir` - 内存文件系统目录
    /// * `disk_dir` - 磁盘备份目录
    /// * `sync_interval_secs` - 同步间隔（秒）
    pub fn new(tmpfs_dir: &str, disk_dir: &str, sync_interval_secs: u64) -> Self {
        Self {
            tmpfs_dir: tmpfs_dir.to_string(),
            disk_dir: disk_dir.to_string(),
            sync_interval_secs,
        }
    }

    /// 启动定时同步任务
    ///
    /// 在后台启动一个定时任务，每隔 sync_interval_secs 秒同步一次数据。
    pub async fn start_sync_task(self: std::sync::Arc<Self>) {
        let mut timer = interval(Duration::from_secs(self.sync_interval_secs));
        loop {
            timer.tick().await;
            if let Err(e) = self.sync_to_disk().await {
                tracing::error!(error = %e, "内存备份同步失败");
            }
        }
    }

    /// 同步到磁盘
    ///
    /// 将高速内存盘中的所有数据同步到磁盘备份目录。
    pub async fn sync_to_disk(&self) -> Result<(), EngineError> {
        // 1. 读取 E:/shm/backup/ 所有文件
        // 2. 复制到 E:/backup/sync/
        // 3. 记录同步时间

        let tmp_path = Path::new(&self.tmpfs_dir);
        let disk_path = Path::new(&self.disk_dir);

        // 确保磁盘目录存在
        fs::create_dir_all(disk_path).await.map_err(|e| {
            EngineError::MemoryBackup(format!("创建磁盘备份目录失败: {}", e))
        })?;

        // 同步各个子目录
        self.sync_directory(tmp_path, disk_path).await?;

        tracing::debug!("内存备份已同步到磁盘");
        Ok(())
    }

    /// 同步单个目录
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
                // 使用 Box::pin 解决异步递归问题
                Box::pin(self.sync_directory(&src_path, &dst_path)).await?;
            } else {
                // 文件直接复制
                fs::copy(&src_path, &dst_path).await.map_err(|e| {
                    EngineError::MemoryBackup(format!("复制文件失败: {}", e))
                })?;
            }
        }

        Ok(())
    }

    /// 保存实时K线
    ///
    /// # 参数
    /// * `symbol` - 交易对
    /// * `kxian` - K线数据
    pub async fn save_kxian(&self, symbol: &str, kxian: &KxianData) -> Result<(), EngineError> {
        let path = format!("{}/symbols/{}/kxian.json", self.tmpfs_dir, symbol);
        self.ensure_dir(&path).await?;

        let mut data = self.load_json::<KxianCache>(&path).await.unwrap_or_default();

        // 添加新的K线数据
        data.m1.extend_from_slice(&kxian.m1);
        data.m5.extend_from_slice(&kxian.m5);
        data.m15.extend_from_slice(&kxian.m15);
        data.d1.extend_from_slice(&kxian.d1);

        // 限制大小
        self.trim_entries(&mut data.m1, MAX_KXIAN_ENTRIES);
        self.trim_entries(&mut data.m5, MAX_KXIAN_ENTRIES);
        self.trim_entries(&mut data.m15, MAX_KXIAN_ENTRIES);
        self.trim_entries(&mut data.d1, MAX_KXIAN_ENTRIES);

        self.write_json(&path, &data).await
    }

    /// 保存深度数据
    pub async fn save_depth(&self, symbol: &str, depth: &DepthData) -> Result<(), EngineError> {
        let path = format!("{}/symbols/{}/depth.json", self.tmpfs_dir, symbol);
        self.ensure_dir(&path).await?;

        let mut data = depth.clone();

        // 限制深度条目数量
        self.trim_depth_entries(&mut data.bids, MAX_DEPTH_ENTRIES);
        self.trim_depth_entries(&mut data.asks, MAX_DEPTH_ENTRIES);

        self.write_json(&path, &data).await
    }

    /// 保存实时成交
    pub async fn save_trades(&self, symbol: &str, trades: &RealtimeTradesData) -> Result<(), EngineError> {
        let path = format!("{}/symbols/{}/trades.json", self.tmpfs_dir, symbol);
        self.ensure_dir(&path).await?;

        let mut data: RealtimeTradesData = self.load_json(&path).await.unwrap_or_default();
        data.trades.extend_from_slice(&trades.trades);
        self.trim_entries(&mut data.trades, MAX_TRADES_ENTRIES);

        self.write_json(&path, &data).await
    }

    /// 保存指标数据
    pub async fn save_indicators(&self, symbol: &str, indicators: &IndicatorsData) -> Result<(), EngineError> {
        let path = format!("{}/symbols/{}/indicators.json", self.tmpfs_dir, symbol);
        self.ensure_dir(&path).await?;

        self.write_json(&path, indicators).await
    }

    /// 保存账户信息
    pub async fn save_account(&self, account: &AccountSnapshot) -> Result<(), EngineError> {
        let path = format!("{}/{}", self.tmpfs_dir, ACCOUNT_FILE);
        self.ensure_dir(&path).await?;
        self.write_json(&path, account).await
    }

    /// 保存持仓信息
    pub async fn save_position(&self, symbol: &str, position: &PositionSnapshot) -> Result<(), EngineError> {
        let path = format!("{}/symbols/{}/position.json", self.tmpfs_dir, symbol);
        self.ensure_dir(&path).await?;
        self.write_json(&path, position).await
    }

    /// 保存订单信息
    pub async fn save_order(&self, order: &OrderSnapshot) -> Result<(), EngineError> {
        let path = format!("{}/order/{}.json", self.tmpfs_dir, order.order_id);
        self.ensure_dir(&path).await?;
        self.write_json(&path, order).await
    }

    /// 保存交易信息
    pub async fn save_trade(&self, trade: &TradeSnapshot) -> Result<(), EngineError> {
        let path = format!("{}/trade/{}.json", self.tmpfs_dir, trade.trade_id);
        self.ensure_dir(&path).await?;
        self.write_json(&path, trade).await
    }

    /// 保存交易规则
    pub async fn save_symbol_rules(&self, rules: &SymbolRulesData) -> Result<(), EngineError> {
        let path = format!("{}/{}", self.tmpfs_dir, SYMBOL_RULES_FILE);
        self.ensure_dir(&path).await?;
        self.write_json(&path, rules).await
    }

    // ============================================================================
    // 通道数据操作
    // ============================================================================

    /// 保存通道数据
    pub async fn save_channel(&self, channel: &ChannelData) -> Result<(), EngineError> {
        let path = match channel.channel_type.as_str() {
            "minute" => format!("{}/{}", self.tmpfs_dir, CHANNEL_MINUTE_FILE),
            "daily" => format!("{}/{}", self.tmpfs_dir, CHANNEL_DAILY_FILE),
            _ => return Err(EngineError::MemoryBackup(format!("未知通道类型: {}", channel.channel_type))),
        };
        self.ensure_dir(&path).await?;
        self.write_json(&path, channel).await
    }

    /// 加载通道数据
    pub async fn load_channel(&self, channel_type: &str) -> Result<Option<ChannelData>, EngineError> {
        let path = match channel_type {
            "minute" => format!("{}/{}", self.tmpfs_dir, CHANNEL_MINUTE_FILE),
            "daily" => format!("{}/{}", self.tmpfs_dir, CHANNEL_DAILY_FILE),
            _ => return Err(EngineError::MemoryBackup(format!("未知通道类型: {}", channel_type))),
        };

        match self.load_json::<ChannelData>(&path).await {
            Ok(data) => Ok(Some(data)),
            Err(e) => {
                // 文件不存在时返回 None
                if let EngineError::MemoryBackup(ref msg) = e {
                    if msg.contains("打开文件失败") {
                        return Ok(None);
                    }
                }
                Err(e)
            }
        }
    }

    // ============================================================================
    // 任务池操作
    // ============================================================================

    /// 保存任务池
    pub async fn save_task_pool(&self, pool_type: &str, pool: &TaskPool) -> Result<(), EngineError> {
        let path = match pool_type {
            "minute" => format!("{}/pool.json", TASKS_MINUTE_DIR),
            "daily" => format!("{}/pool.json", TASKS_DAILY_DIR),
            _ => return Err(EngineError::MemoryBackup(format!("未知任务池类型: {}", pool_type))),
        };
        let full_path = format!("{}/{}", self.tmpfs_dir, path);
        self.ensure_dir(&full_path).await?;

        let mut data: TaskPool = self.load_json(&full_path).await.unwrap_or_default();
        // 合并数据
        data.active_tasks.extend_from_slice(&pool.active_tasks);
        data.completed_count = pool.completed_count;
        data.failed_count = pool.failed_count;
        data.updated_at = pool.updated_at.clone();

        // 限制任务数量
        self.trim_entries(&mut data.active_tasks, MAX_TASKS_ENTRIES);

        self.write_json(&full_path, &data).await
    }

    /// 保存品种任务
    pub async fn save_symbol_task(&self, pool_type: &str, symbol: &str, task: &TaskInfo) -> Result<(), EngineError> {
        let path = match pool_type {
            "minute" => format!("{}/{}.json", TASKS_MINUTE_DIR, symbol),
            "daily" => format!("{}/{}.json", TASKS_DAILY_DIR, symbol),
            _ => return Err(EngineError::MemoryBackup(format!("未知任务池类型: {}", pool_type))),
        };
        let full_path = format!("{}/{}", self.tmpfs_dir, path);
        self.ensure_dir(&full_path).await?;
        self.write_json(&full_path, task).await
    }

    /// 加载任务池
    pub async fn load_task_pool(&self, pool_type: &str) -> Result<Option<TaskPool>, EngineError> {
        let path = match pool_type {
            "minute" => format!("{}/pool.json", TASKS_MINUTE_DIR),
            "daily" => format!("{}/pool.json", TASKS_DAILY_DIR),
            _ => return Err(EngineError::MemoryBackup(format!("未知任务池类型: {}", pool_type))),
        };
        let full_path = format!("{}/{}", self.tmpfs_dir, path);

        match self.load_json::<TaskPool>(&full_path).await {
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

    // ============================================================================
    // 交易品种列表操作
    // ============================================================================

    /// 保存交易品种列表
    pub async fn save_trading_pairs(&self, pairs: &TradingPairs) -> Result<(), EngineError> {
        let path = format!("{}/{}", self.tmpfs_dir, TRADING_PAIRS_FILE);
        self.ensure_dir(&path).await?;
        self.write_json(&path, pairs).await
    }

    /// 加载交易品种列表
    pub async fn load_trading_pairs(&self) -> Result<Option<TradingPairs>, EngineError> {
        let path = format!("{}/{}", self.tmpfs_dir, TRADING_PAIRS_FILE);

        match self.load_json::<TradingPairs>(&path).await {
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

    // ============================================================================
    // 规则汇总操作
    // ============================================================================

    /// 保存规则汇总
    pub async fn save_symbol_rules_summary(&self, summary: &SymbolRulesSummary) -> Result<(), EngineError> {
        let path = format!("{}/{}", self.tmpfs_dir, SYMBOL_RULES_FILE);
        self.ensure_dir(&path).await?;
        self.write_json(&path, summary).await
    }

    /// 加载规则汇总
    pub async fn load_symbol_rules_summary(&self) -> Result<Option<SymbolRulesSummary>, EngineError> {
        let path = format!("{}/{}", self.tmpfs_dir, SYMBOL_RULES_FILE);

        match self.load_json::<SymbolRulesSummary>(&path).await {
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

    /// 限制条目数量（从前面删除旧数据）
    fn trim_entries<T>(&self, v: &mut Vec<T>, max: usize) {
        while v.len() > max {
            v.remove(0);
        }
    }

    /// 限制深度条目数量
    fn trim_depth_entries(&self, v: &mut Vec<DepthEntry>, max: usize) {
        while v.len() > max {
            v.remove(0);
        }
        // 保持深度按价格排序（从高到低）
        v.sort_by(|a, b| b.price.cmp(&a.price));
    }

    /// 加载 JSON 文件
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

    /// 写入 JSON 文件
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

    /// 确保目录存在
    async fn ensure_dir(&self, path: &str) -> Result<(), EngineError> {
        let path = Path::new(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await.map_err(|e| {
                EngineError::MemoryBackup(format!("创建目录失败: {}", e))
            })?;
        }
        Ok(())
    }

    /// 获取内存备份目录路径
    pub fn tmpfs_dir(&self) -> &str {
        &self.tmpfs_dir
    }

    /// 获取磁盘备份目录路径
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
        let paths = crate::platform::Paths::new();
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
    async fn test_kxian_cache_default() {
        let cache = KxianCache::default();
        assert!(cache.m1.is_empty());
        assert!(cache.m5.is_empty());
        assert!(cache.m15.is_empty());
        assert!(cache.d1.is_empty());
    }
}
