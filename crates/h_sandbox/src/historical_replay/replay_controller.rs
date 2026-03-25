//! ReplayController - 流式回放控制器
//!
//! 控制 Tick 生成间隔、回放速度、引擎驱动循环。

use std::sync::Arc;
use std::time::{Duration, Instant};
use parking_lot::RwLock;
use tokio::time::sleep;
use tracing::{info, warn, error};

use b_data_source::KLine;
use super::kline_loader::KlineLoader;
use super::tick_generator::StreamTickGenerator;
use super::memory_injector::{MemoryInjector, SharedMarketData, MemoryInjectorConfig};
use super::tick_generator::SimulatedTick;

/// 回放控制器配置
#[derive(Debug, Clone)]
pub struct ReplayConfig {
    /// 回放速度倍率（1.0=实时，10.0=10倍速）
    pub playback_speed: f64,
    /// Tick 间隔（毫秒，原始 1000ms / 60）
    pub tick_interval_ms: u64,
    /// 是否输出详细日志
    pub verbose: bool,
    /// 启动前等待时间（秒）
    pub warmup_seconds: u64,
}

impl Default for ReplayConfig {
    fn default() -> Self {
        Self {
            playback_speed: 1.0,
            tick_interval_ms: 16, // ~60fps
            verbose: false,
            warmup_seconds: 0,
        }
    }
}

impl ReplayConfig {
    /// 设置回放速度
    pub fn with_speed(mut self, speed: f64) -> Self {
        self.playback_speed = speed;
        self
    }

    /// 设置详细日志
    pub fn with_verbose(mut self) -> Self {
        self.verbose = true;
        self
    }

    /// 计算实际 Tick 间隔
    pub fn effective_interval_ms(&self) -> u64 {
        let base = self.tick_interval_ms;
        if self.playback_speed <= 0.0 {
            return base;
        }
        (base as f64 / self.playback_speed) as u64
    }
}

/// 回放状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayState {
    /// 初始化
    Init,
    /// 运行中
    Running,
    /// 暂停
    Paused,
    /// 完成
    Completed,
    /// 错误
    Error,
}

/// 回放统计
#[derive(Debug, Clone, Default)]
pub struct ReplayStats {
    /// 已发送 Tick 数
    pub ticks_sent: u64,
    /// 已完成 K线数
    pub klines_completed: u64,
    /// 开始时间
    pub start_time: Option<Instant>,
    /// 最后 Tick 时间
    pub last_tick_time: Option<Instant>,
}

impl ReplayStats {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn elapsed(&self) -> Duration {
        self.start_time
            .map(|t| t.elapsed())
            .unwrap_or(Duration::ZERO)
    }
}

/// 流式回放控制器
///
/// 协调 KlineLoader → StreamTickGenerator → MemoryInjector → Engine 的流程。
pub struct ReplayController {
    /// 配置
    config: ReplayConfig,
    /// 状态
    state: ReplayState,
    /// 统计
    stats: ReplayStats,
    /// 共享内存
    shared_data: Arc<RwLock<SharedMarketData>>,
    /// 内存写入器
    injector: MemoryInjector,
}

impl ReplayController {
    /// 创建控制器
    pub fn new(config: ReplayConfig) -> Self {
        let shared_data = Arc::new(RwLock::new(SharedMarketData::new()));
        let injector_config = MemoryInjectorConfig {
            write_interval_ms: config.tick_interval_ms,
            debug_log: config.verbose,
            ..Default::default()
        };

        Self {
            config,
            state: ReplayState::Init,
            stats: ReplayStats::new(),
            shared_data,
            injector: MemoryInjector::with_config(shared_data.clone(), injector_config),
        }
    }

    /// 创建控制器（使用已有 shared_data）
    pub fn with_shared_data(
        config: ReplayConfig,
        shared_data: Arc<RwLock<SharedMarketData>>,
    ) -> Self {
        let injector_config = MemoryInjectorConfig {
            write_interval_ms: config.tick_interval_ms,
            debug_log: config.verbose,
            ..Default::default()
        };

        Self {
            config,
            state: ReplayState::Init,
            stats: ReplayStats::new(),
            shared_data,
            injector: MemoryInjector::with_config(shared_data.clone(), injector_config),
        }
    }

    /// 获取共享内存引用
    pub fn shared_data(&self) -> Arc<RwLock<SharedMarketData>> {
        self.shared_data.clone()
    }

    /// 获取当前状态
    pub fn state(&self) -> ReplayState {
        self.state
    }

    /// 获取统计信息
    pub fn stats(&self) -> &ReplayStats {
        &self.stats
    }

    /// 启动回放（同步版本）
    pub fn run(&mut self, parquet_path: &str, symbol: &str) -> Result<(), ReplayError> {
        info!("启动回放: path={}, symbol={}, speed={}x",
            parquet_path, symbol, self.config.playback_speed);

        // 加载 Parquet
        let loader = KlineLoader::new(parquet_path)
            .map_err(|e| ReplayError::LoadError(e.to_string()))?
            .with_symbol(symbol);

        let total_rows = loader.total_rows();
        info!("加载 Parquet: {} rows", total_rows);

        // 创建 Tick 生成器
        let generator = StreamTickGenerator::from_loader(symbol.to_string(), loader);

        // 开始回放
        self.state = ReplayState::Running;
        self.stats.start_time = Some(Instant::now());

        // 主循环
        let effective_interval = Duration::from_millis(self.config.effective_interval_ms());

        for tick in generator {
            if self.state != ReplayState::Running {
                break;
            }

            // 写入内存
            self.injector.write_tick(tick.clone());
            self.stats.ticks_sent += 1;
            self.stats.last_tick_time = Some(Instant::now());

            // 检测 K线完成
            if let Some(ref kline) = self.injector.current_kline() {
                if self.stats.klines_completed == 0 ||
                   kline.timestamp.timestamp_millis() != self.last_kline_ts() {
                    if self.stats.klines_completed > 0 {
                        // 新 K线开始，上一根完成
                    }
                    self.stats.klines_completed += 1;
                }
            }

            if self.config.verbose {
                if self.stats.ticks_sent % 60 == 0 {
                    info!("进度: {} ticks, {} klines, elapsed={:?}",
                        self.stats.ticks_sent,
                        self.stats.klines_completed,
                        self.stats.elapsed());
                }
            }

            // 控制速度
            sleep(effective_interval).await;
        }

        self.state = ReplayState::Completed;
        info!("回放完成: {} ticks, {} klines, elapsed={:?}",
            self.stats.ticks_sent,
            self.stats.klines_completed,
            self.stats.elapsed());

        Ok(())
    }

    /// 内部：记录上一根 K线时间戳
    fn last_kline_ts(&self) -> i64 {
        self.injector.current_kline()
            .map(|k| k.timestamp.timestamp_millis())
            .unwrap_or(-1)
    }

    /// 暂停回放
    pub fn pause(&mut self) {
        if self.state == ReplayState::Running {
            self.state = ReplayState::Paused;
            info!("回放已暂停");
        }
    }

    /// 恢复回放
    pub fn resume(&mut self) {
        if self.state == ReplayState::Paused {
            self.state = ReplayState::Running;
            info!("回放已恢复");
        }
    }

    /// 停止回放
    pub fn stop(&mut self) {
        self.state = ReplayState::Completed;
        info!("回放已停止");
    }
}

/// 回放错误类型
#[derive(Debug, Clone)]
pub enum ReplayError {
    /// 加载错误
    LoadError(String),
    /// 运行错误
    RuntimeError(String),
}

impl std::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReplayError::LoadError(msg) => write!(f, "加载错误: {}", msg),
            ReplayError::RuntimeError(msg) => write!(f, "运行时错误: {}", msg),
        }
    }
}

impl std::error::Error for ReplayError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = ReplayConfig::default();
        assert_eq!(config.playback_speed, 1.0);
        assert_eq!(config.tick_interval_ms, 16);
    }

    #[test]
    fn test_config_with_speed() {
        let config = ReplayConfig::default().with_speed(10.0);
        assert_eq!(config.playback_speed, 10.0);
        assert_eq!(config.effective_interval_ms(), 1); // 16 / 10 = 1
    }

    #[test]
    fn test_replay_state() {
        assert_eq!(ReplayState::Init, ReplayState::Init);
        assert_ne!(ReplayState::Init, ReplayState::Running);
    }
}
