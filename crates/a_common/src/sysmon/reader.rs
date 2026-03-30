//! 心跳读取器 - 汇聚中心读取所有模块心跳

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use super::types::{
    Config, ModuleId, ModuleStatus, ModuleSummary, OverviewStats, SystemOverview,
};

/// 心跳读取器
pub struct HeartbeatReader {
    config: Config,
}

impl HeartbeatReader {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    /// 读取单个模块的心跳
    fn read_module(&self, module: ModuleId) -> Option<ModuleSummary> {
        let path = self.config.path.join(module.file_name());

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return None, // 文件不存在或读取失败
        };

        let heartbeat: super::types::Heartbeat = match serde_json::from_str(&content) {
            Ok(h) => h,
            Err(_) => return None, // JSON 解析失败
        };

        // 计算 freshness
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let fresh_ms = now_ms - heartbeat.t;

        // 状态判定：OK<1000ms, WARN<3000ms, ERROR>=3000ms
        let status = if fresh_ms < self.config.ok_threshold_ms {
            ModuleStatus::Ok
        } else if fresh_ms < self.config.warn_threshold_ms {
            ModuleStatus::Warn
        } else {
            ModuleStatus::Error
        };

        // 生成摘要
        let summary = self.generate_summary(module, &heartbeat, fresh_ms);

        Some(ModuleSummary {
            id: module,
            status,
            fresh_ms,
            summary,
            detail: Some(heartbeat.m),
        })
    }

    /// 生成模块摘要文本
    fn generate_summary(
        &self,
        module: ModuleId,
        hb: &super::types::Heartbeat,
        fresh_ms: i64,
    ) -> String {
        match module {
            ModuleId::Kline => {
                if let (Some(symbols), Some(delay_ms)) = (
                    hb.m.get("symbols").and_then(|v| v.as_u64()),
                    hb.m.get("delay_ms").and_then(|v| v.as_i64()),
                ) {
                    format!("{}品种 延迟{}ms", symbols, delay_ms)
                } else {
                    format!("延迟{}ms", fresh_ms)
                }
            }
            ModuleId::Indicator => {
                if let (Some(ready), Some(latency)) = (
                    hb.m.get("ready").and_then(|v| v.as_u64()),
                    hb.m.get("latency_ms").and_then(|v| v.as_i64()),
                ) {
                    format!("{}就绪 延迟{}ms", ready, latency)
                } else {
                    format!("延迟{}ms", fresh_ms)
                }
            }
            ModuleId::Strategy => {
                if let (Some(running), Some(calc_ms)) = (
                    hb.m.get("running").and_then(|v| v.as_u64()),
                    hb.m.get("calc_ms").and_then(|v| v.as_i64()),
                ) {
                    format!("{}运行 延迟{}ms", running, calc_ms)
                } else {
                    format!("延迟{}ms", fresh_ms)
                }
            }
            ModuleId::Order => {
                if let (Some(conn), Some(pending)) = (
                    hb.m.get("conn").and_then(|v| v.as_str()),
                    hb.m.get("pending").and_then(|v| v.as_u64()),
                ) {
                    format!("连接{} 待执行{}", conn, pending)
                } else {
                    format!("延迟{}ms", fresh_ms)
                }
            }
            ModuleId::Position => {
                if let (Some(synced), Some(diff)) = (
                    hb.m.get("synced").and_then(|v| v.as_u64()),
                    hb.m.get("diff").and_then(|v| v.as_i64()),
                ) {
                    format!("{}同步 差异{}", synced, diff)
                } else {
                    format!("延迟{}ms", fresh_ms)
                }
            }
            ModuleId::Risk => {
                if let Some(level) = hb.m.get("level").and_then(|v| v.as_str()) {
                    format!("{}", level)
                } else {
                    format!("延迟{}ms", fresh_ms)
                }
            }
            ModuleId::Storage => {
                if let Some(pct) = hb.m.get("used_pct").and_then(|v| v.as_u64()) {
                    format!("tmpfs {}%", pct)
                } else {
                    format!("延迟{}ms", fresh_ms)
                }
            }
            ModuleId::Market => {
                if let (Some(ws), Some(latency)) = (
                    hb.m.get("ws").and_then(|v| v.as_str()),
                    hb.m.get("latency_ms").and_then(|v| v.as_i64()),
                ) {
                    format!("WS{} 延迟{}ms", ws, latency)
                } else {
                    format!("延迟{}ms", fresh_ms)
                }
            }
        }
    }

    /// 格式化时间戳
    #[allow(dead_code)]
    fn format_timestamp(ts_ms: i64) -> String {
        use chrono::{DateTime, Utc};
        let dt = DateTime::<Utc>::from_timestamp_millis(ts_ms)
            .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).unwrap());
        dt.format("%H:%M:%S").to_string()
    }

    /// 读取所有模块，生成系统总览
    pub fn read_all(&self) -> SystemOverview {
        let modules: Vec<ModuleSummary> = ModuleId::all()
            .iter()
            .filter_map(|&m| self.read_module(m))
            .collect();

        let stats = OverviewStats::from_modules(&modules);

        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let uptime_sec = self.config.start_time.elapsed().as_secs();

        // 生成告警
        let alert = if stats.error > 0 {
            Some(format!("{}个模块ERROR", stats.error))
        } else if stats.warn > 0 {
            Some(format!("{}个模块WARN", stats.warn))
        } else {
            None
        };

        SystemOverview {
            system_time: now_ms,
            uptime_sec,
            modules,
            stats,
            alert,
        }
    }
}
