#![forbid(unsafe_code)]

//! 平台路径配置 - 自动检测 Windows/Linux 并选择对应路径
//!
//! # 路径策略
//!
//! | 数据类型 | Windows | Linux |
//! |---------|---------|-------|
//! | SQLite/CSV | E:/backup/ | data/ |
//! | 内存备份(高速) | E:/shm/backup/ | /dev/shm/backup/ |
//! | 磁盘备份同步 | E:/backup/sync/ | data/backup/ |

use std::path::PathBuf;

/// 平台类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    /// Windows (检测到 E: 盘或 Windows 环境)
    Windows,
    /// Linux (检测到 /dev/shm 或 Linux 环境)
    Linux,
}

impl Platform {
    /// 自动检测当前平台
    pub fn detect() -> Self {
        #[cfg(target_os = "windows")]
        {
            Platform::Windows
        }

        #[cfg(target_os = "linux")]
        {
            // 优先检查 /dev/shm 是否存在且可写
            if std::path::Path::new("/dev/shm").exists() {
                Platform::Linux
            } else {
                // Fallback 到 data/ 目录
                Platform::Linux
            }
        }

        #[cfg(not(any(target_os = "windows", target_os = "linux")))]
        {
            Platform::Linux
        }
    }

    /// 是否是 Windows
    pub fn is_windows(&self) -> bool {
        *self == Platform::Windows
    }

    /// 是否是 Linux
    pub fn is_linux(&self) -> bool {
        *self == Platform::Linux
    }
}

/// 路径配置
#[derive(Debug, Clone)]
pub struct Paths {
    /// 高速内存盘路径 (K线、depth、trades 等高频数据)
    pub memory_backup_dir: String,
    /// 磁盘备份同步路径 (从内存定期同步)
    pub disk_sync_dir: String,
    /// SQLite 数据库路径
    pub sqlite_db_path: PathBuf,
    /// CSV 输出目录
    pub csv_output_path: PathBuf,
}

impl Default for Paths {
    fn default() -> Self {
        Self::new()
    }
}

impl Paths {
    /// 创建默认路径配置（自动检测平台）
    pub fn new() -> Self {
        let platform = Platform::detect();

        match platform {
            Platform::Windows => Self::windows(),
            Platform::Linux => Self::linux(),
        }
    }

    /// Windows 路径配置
    pub fn windows() -> Self {
        Self {
            memory_backup_dir: "E:/shm/backup".to_string(),
            disk_sync_dir: "E:/backup/sync".to_string(),
            sqlite_db_path: PathBuf::from("E:/backup/trading_events.db"),
            csv_output_path: PathBuf::from("E:/backup/output/indicator_comparison.csv"),
        }
    }

    /// Linux 路径配置
    pub fn linux() -> Self {
        Self {
            memory_backup_dir: "/dev/shm/backup".to_string(),
            disk_sync_dir: "data/backup".to_string(),
            sqlite_db_path: PathBuf::from("data/trading_events.db"),
            csv_output_path: PathBuf::from("output/indicator_comparison.csv"),
        }
    }

    /// 获取当前平台
    pub fn platform(&self) -> Platform {
        Platform::detect()
    }

    /// 创建 SQLite 服务使用的路径
    pub fn sqlite_db(&self) -> PathBuf {
        self.sqlite_db_path.clone()
    }

    /// 创建 CSV 写入器使用的路径
    pub fn csv_output(&self) -> PathBuf {
        self.csv_output_path.clone()
    }

    /// 创建内存备份管理器使用的参数
    pub fn memory_backup_params(&self) -> (String, String, u64) {
        (
            self.memory_backup_dir.clone(),
            self.disk_sync_dir.clone(),
            30, // 默认 30 秒同步间隔
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_detection() {
        let platform = Platform::detect();
        #[cfg(target_os = "windows")]
        assert_eq!(platform, Platform::Windows);
        #[cfg(target_os = "linux")]
        assert_eq!(platform, Platform::Linux);
    }

    #[test]
    fn test_windows_paths() {
        let paths = Paths::windows();
        assert_eq!(paths.memory_backup_dir, "E:/shm/backup");
        assert_eq!(paths.disk_sync_dir, "E:/backup/sync");
        assert_eq!(paths.sqlite_db_path, PathBuf::from("E:/backup/trading_events.db"));
    }

    #[test]
    fn test_linux_paths() {
        let paths = Paths::linux();
        assert_eq!(paths.memory_backup_dir, "/dev/shm/backup");
        assert_eq!(paths.disk_sync_dir, "data/backup");
        assert_eq!(paths.sqlite_db_path, PathBuf::from("data/trading_events.db"));
    }

    #[test]
    fn test_default_paths_auto_detect() {
        let paths = Paths::default();
        let platform = Platform::detect();

        match platform {
            Platform::Windows => {
                assert_eq!(paths.memory_backup_dir, "E:/shm/backup");
                assert!(paths.sqlite_db_path.to_str().unwrap().starts_with("E:"));
            }
            Platform::Linux => {
                assert_eq!(paths.memory_backup_dir, "/dev/shm/backup");
                assert!(!paths.sqlite_db_path.to_str().unwrap().starts_with("E:"));
            }
        }
    }
}
