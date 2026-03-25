//! TickDriver - 自循环推送 Tick 到 DataFeeder
//!
//! 从 TickGenerator 读取 tick，自循环推送到 DataFeeder

use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use parking_lot::RwLock;

use super::generator::TickGenerator;
use b_data_source::DataFeeder;

/// TickDriver - 自循环推送 Tick 到 DataFeeder
pub struct TickDriver {
    generator: Arc<RwLock<TickGenerator>>,
    data_feeder: Arc<DataFeeder>,
    tick_interval: Duration,
    /// 是否在运行
    running: RwLock<bool>,
}

impl TickDriver {
    /// 创建驱动器
    ///
    /// # Arguments
    /// * `generator` - Tick 生成器
    /// * `data_feeder` - DataFeeder 实例
    /// * `tick_interval_ms` - tick 间隔（毫秒），默认 16ms（约60 ticks/秒）
    pub fn new(
        generator: TickGenerator,
        data_feeder: Arc<DataFeeder>,
        tick_interval_ms: u64,
    ) -> Self {
        Self {
            generator: Arc::new(RwLock::new(generator)),
            data_feeder,
            tick_interval: Duration::from_millis(tick_interval_ms),
            running: RwLock::new(false),
        }
    }

    /// 创建驱动器（默认 16ms 间隔）
    pub fn with_default_interval(generator: TickGenerator, data_feeder: Arc<DataFeeder>) -> Self {
        Self::new(generator, data_feeder, 16)
    }

    /// 运行循环
    ///
    /// 会阻塞直到所有 tick 发送完毕或被停止
    pub async fn run(&self) {
        {
            let mut running = self.running.write();
            if *running {
                tracing::warn!("TickDriver already running");
                return;
            }
            *running = true;
        }

        let mut ticker = interval(self.tick_interval);
        let total_klines = self.generator.read().total_klines();

        tracing::info!(
            "TickDriver started: {} K-lines, {}ms interval",
            total_klines,
            self.tick_interval.as_millis()
        );

        loop {
            ticker.tick().await;

            // 检查是否停止
            if !*self.running.read() {
                tracing::info!("TickDriver stopped by signal");
                break;
            }

            // 生成 tick
            let tick = {
                let mut gen = self.generator.write();
                if gen.is_exhausted() {
                    tracing::info!("TickGenerator exhausted, stopping");
                    break;
                }
                gen.next_tick()
            };

            match tick {
                Some(t) => {
                    // 转换为 DataFeeder 需要的 Tick 格式
                    let tick = b_data_source::Tick {
                        symbol: t.symbol,
                        price: t.price,
                        qty: t.qty,
                        timestamp: t.timestamp,
                        kline_1m: None, // 可扩展：附带当前 K 线
                        kline_15m: None,
                        kline_1d: None,
                    };

                    // 推送到 DataFeeder
                    self.data_feeder.update_tick(tick);

                    // 调试日志（每100个tick打印一次）
                    let tick_count = self.generator.read().tick_count();
                    if tick_count % 100 == 0 {
                        tracing::debug!(
                            "Tick #{:06} | {} @ {} | H:{} L:{}",
                            tick_count,
                            t.symbol,
                            t.price,
                            t.high,
                            t.low
                        );
                    }
                }
                None => {
                    tracing::warn!("No tick generated, stopping");
                    break;
                }
            }
        }

        *self.running.write() = false;
        tracing::info!("TickDriver finished");
    }

    /// 停止运行
    pub fn stop(&self) {
        *self.running.write() = false;
        tracing::info!("TickDriver stop signal sent");
    }

    /// 检查是否在运行
    pub fn is_running(&self) -> bool {
        *self.running.read()
    }

    /// 获取进度
    ///
    /// 返回 (已发送tick数, 估计总tick数)
    pub fn progress(&self) -> (u64, u64) {
        let gen = self.generator.read();
        let total_klines = gen.total_klines();
        let remaining = gen.remaining_in_current_kline() as u64;
        let total_ticks = (total_klines as u64) * 60;
        let sent = total_ticks.saturating_sub(remaining);
        (sent, total_ticks)
    }

    /// 获取生成器引用
    pub fn generator(&self) -> Arc<RwLock<TickGenerator>> {
        Arc::clone(&self.generator)
    }
}

impl Default for TickDriver {
    fn default() -> Self {
        Self {
            generator: Arc::new(RwLock::new(TickGenerator::default())),
            data_feeder: Arc::new(DataFeeder::new()),
            tick_interval: Duration::from_millis(16),
            running: RwLock::new(false),
        }
    }
}

/// Builder 模式配置 TickDriver
pub struct TickDriverBuilder {
    generator: Option<TickGenerator>,
    data_feeder: Option<Arc<DataFeeder>>,
    tick_interval_ms: u64,
}

impl TickDriverBuilder {
    pub fn new() -> Self {
        Self {
            generator: None,
            data_feeder: None,
            tick_interval_ms: 16,
        }
    }

    /// 设置生成器
    pub fn generator(mut self, generator: TickGenerator) -> Self {
        self.generator = Some(generator);
        self
    }

    /// 设置 DataFeeder
    pub fn data_feeder(mut self, data_feeder: Arc<DataFeeder>) -> Self {
        self.data_feeder = Some(data_feeder);
        self
    }

    /// 设置 tick 间隔（毫秒）
    pub fn tick_interval(mut self, ms: u64) -> Self {
        self.tick_interval_ms = ms;
        self
    }

    /// 构建 TickDriver
    pub fn build(self) -> Result<TickDriver, &'static str> {
        let generator = self.generator.ok_or("generator not set")?;
        let data_feeder = self.data_feeder.ok_or("data_feeder not set")?;

        Ok(TickDriver::new(generator, data_feeder, self.tick_interval_ms))
    }
}

impl Default for TickDriverBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_generator() -> TickGenerator {
        let klines = vec![
            b_data_source::KLine {
                symbol: "BTCUSDT".to_string(),
                period: b_data_source::Period::Minute(1),
                open: dec!(50000),
                high: dec!(50100),
                low: dec!(49900),
                close: dec!(50050),
                volume: dec!(100),
                timestamp: chrono::Utc::now(),
            },
            b_data_source::KLine {
                symbol: "BTCUSDT".to_string(),
                period: b_data_source::Period::Minute(1),
                open: dec!(50050),
                high: dec!(50200),
                low: dec!(50000),
                close: dec!(50150),
                volume: dec!(120),
                timestamp: chrono::Utc::now() + chrono::Duration::minutes(1),
            },
        ];

        TickGenerator::from_klines("BTCUSDT".to_string(), klines)
    }

    #[test]
    fn test_driver_creation() {
        let gen = create_test_generator();
        let feeder = Arc::new(DataFeeder::new());
        let driver = TickDriverBuilder::new()
            .generator(gen)
            .data_feeder(feeder)
            .tick_interval(16)
            .build()
            .unwrap();

        assert!(!driver.is_running());
    }

    #[test]
    fn test_driver_progress() {
        let gen = create_test_generator();
        let feeder = Arc::new(DataFeeder::new());
        let driver = TickDriverBuilder::new()
            .generator(gen)
            .data_feeder(feeder)
            .build()
            .unwrap();

        let (sent, total) = driver.progress();
        assert_eq!(total, 120); // 2 K-lines * 60 ticks
        assert_eq!(sent, 0);
    }

    #[tokio::test]
    async fn test_driver_run() {
        let gen = create_test_generator();
        let feeder = Arc::new(DataFeeder::new());
        let driver = TickDriverBuilder::new()
            .generator(gen)
            .data_feeder(feeder.clone())
            .tick_interval(1) // 1ms for fast test
            .build()
            .unwrap();

        // 运行（带超时）
        let handle = tokio::spawn(async move {
            driver.run().await;
        });

        // 等待最多 1 秒
        let result = tokio::time::timeout(Duration::from_secs(1), handle).await;

        match result {
            Ok(_) => {
                // 正常完成
                let (sent, total) = driver.progress();
                assert_eq!(sent, total);
            }
            Err(_) => {
                // 超时
                driver.stop();
                panic!("Driver took too long");
            }
        }
    }
}
