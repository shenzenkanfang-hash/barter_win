//! 系统心跳监控配置

use super::types::Config;

/// 默认配置
pub fn default_config() -> Config {
    Config::default()
}

/// 从环境变量加载配置
pub fn load_from_env() -> Config {
    let mut config = Config::default();

    if let Ok(path) = std::env::var("SYSMON_PATH") {
        config.path = std::path::PathBuf::from(path);
    }

    if let Ok(ok_ms) = std::env::var("SYSMON_OK_MS").and_then(|s| s.parse()) {
        config.ok_threshold_ms = ok_ms;
    }

    if let Ok(warn_ms) = std::env::var("SYSMON_WARN_MS").and_then(|s| s.parse()) {
        config.warn_threshold_ms = warn_ms;
    }

    if let Ok(interval) = std::env::var("SYSMON_INTERVAL").and_then(|s| s.parse()) {
        config.refresh_interval_secs = interval;
    }

    config
}
