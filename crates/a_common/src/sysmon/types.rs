//! 系统心跳监控数据类型

use serde::{Deserialize, Serialize};

/// 模块ID枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModuleId {
    Kline,
    Indicator,
    Strategy,
    Order,
    Position,
    Risk,
    Storage,
    Market,
}

impl ModuleId {
    /// 获取文件名
    pub fn file_name(&self) -> &'static str {
        match self {
            Self::Kline => "kline.json",
            Self::Indicator => "indicator.json",
            Self::Strategy => "strategy.json",
            Self::Order => "order.json",
            Self::Position => "position.json",
            Self::Risk => "risk.json",
            Self::Storage => "storage.json",
            Self::Market => "market.json",
        }
    }

    /// 获取显示名称
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Kline => "K线",
            Self::Indicator => "指标",
            Self::Strategy => "策略",
            Self::Order => "订单",
            Self::Position => "仓位",
            Self::Risk => "风控",
            Self::Storage => "存储",
            Self::Market => "行情",
        }
    }

    /// 返回所有模块
    pub fn all() -> [Self; 8] {
        [
            Self::Kline,
            Self::Indicator,
            Self::Strategy,
            Self::Order,
            Self::Position,
            Self::Risk,
            Self::Storage,
            Self::Market,
        ]
    }
}

/// 模块状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModuleStatus {
    Ok,
    Warn,
    Error,
    Unknown,
}

impl Default for ModuleStatus {
    fn default() -> Self {
        Self::Unknown
    }
}

/// 心跳包基础结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Heartbeat {
    /// 时间戳（毫秒）
    pub t: i64,
    /// 状态
    pub s: ModuleStatus,
    /// 序号
    pub seq: u64,
    /// 模块特定指标
    pub m: serde_json::Value,
    /// 最后动作
    pub a: String,
    /// 下次预期
    pub n: String,
}

/// 模块状态摘要（汇聚中心使用）
#[derive(Debug, Clone)]
pub struct ModuleSummary {
    pub id: ModuleId,
    pub status: ModuleStatus,
    /// freshness: 当前时间 - t
    pub fresh_ms: i64,
    pub summary: String,
    pub detail: Option<serde_json::Value>,
}

/// 系统总览
#[derive(Debug, Clone)]
pub struct SystemOverview {
    pub system_time: i64,
    pub uptime_sec: u64,
    pub modules: Vec<ModuleSummary>,
    pub stats: OverviewStats,
    pub alert: Option<String>,
}

/// 状态统计
#[derive(Debug, Clone, Default)]
pub struct OverviewStats {
    pub ok: usize,
    pub warn: usize,
    pub error: usize,
    pub unknown: usize,
}

impl OverviewStats {
    /// 从模块列表生成统计
    pub fn from_modules(modules: &[ModuleSummary]) -> Self {
        let mut stats = Self::default();
        for m in modules {
            match m.status {
                ModuleStatus::Ok => stats.ok += 1,
                ModuleStatus::Warn => stats.warn += 1,
                ModuleStatus::Error => stats.error += 1,
                ModuleStatus::Unknown => stats.unknown += 1,
            }
        }
        stats
    }
}

/// 配置
#[derive(Debug, Clone)]
pub struct Config {
    /// 存储路径
    pub path: std::path::PathBuf,
    /// OK 阈值（毫秒）
    pub ok_threshold_ms: i64,
    /// WARN 阈值（毫秒）
    pub warn_threshold_ms: i64,
    /// 刷新间隔（秒）
    pub refresh_interval_secs: u64,
    /// 系统启动时间
    pub start_time: std::time::Instant,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            // Linux: /dev/shm/trade_sysmon/, Windows: ./sysmon/
            path: if cfg!(target_os = "linux") {
                std::path::PathBuf::from("/dev/shm/trade_sysmon")
            } else {
                std::path::PathBuf::from("./sysmon")
            },
            ok_threshold_ms: 1000,
            warn_threshold_ms: 3000,
            refresh_interval_secs: 10,
            start_time: std::time::Instant::now(),
        }
    }
}
