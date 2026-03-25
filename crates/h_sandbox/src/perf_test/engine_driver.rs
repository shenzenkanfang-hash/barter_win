//! EngineDriver - 引擎封装
//!
//! 封装引擎调用，处理 tick，测量延迟

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::mpsc;
use rust_decimal::Decimal;

use crate::perf_test::tracker::PerformanceTracker;
use super::tick_driver::TimedTick;

/// 引擎驱动配置
#[derive(Debug, Clone)]
pub struct EngineDriverConfig {
    /// 是否静默模式（减少日志输出）
    pub silent: bool,
    /// 打印进度的间隔
    pub progress_interval: u64,
}

impl Default for EngineDriverConfig {
    fn default() -> Self {
        Self {
            silent: false,
            progress_interval: 100,
        }
    }
}

/// EngineDriver - 引擎封装
pub struct EngineDriver {
    config: EngineDriverConfig,
    tracker: Arc<PerformanceTracker>,
    total_ticks: u64,
}

impl EngineDriver {
    pub fn new(config: EngineDriverConfig, tracker: Arc<PerformanceTracker>, total_ticks: u64) -> Self {
        Self {
            config,
            tracker,
            total_ticks,
        }
    }

    /// 运行处理循环
    pub async fn run(&self, mut receiver: mpsc::Receiver<TimedTick>) {
        let mut processed = 0u64;

        while let Some(timed_tick) = receiver.recv().await {
            let t0 = timed_tick.t0;
            let t1 = Instant::now();

            // 计算延迟
            let latency = t1.duration_since(t0);

            // 处理 tick（这里简化处理，实际应该调用引擎）
            let success = self.process_tick(&timed_tick).await;

            // 记录性能
            self.tracker.record(latency, success);

            processed += 1;

            // 打印进度
            if !self.config.silent && processed % self.config.progress_interval == 0 {
                let stats = self.tracker.stats();
                println!(
                    "进度: {}/{} | 平均: {:.2}ms | P99: {:.2}ms | 积压: {}",
                    processed,
                    self.total_ticks,
                    stats.avg_ms(),
                    stats.p99_ms(),
                    self.tracker.backlog()
                );
            }
        }

        if !self.config.silent {
            println!("处理完成: {} ticks", processed);
        }
    }

    /// 处理单个 tick
    ///
    /// 这里应该调用实际引擎的 process_tick 方法
    /// 当前是模拟实现，用于测试框架
    async fn process_tick(&self, _timed_tick: &TimedTick) -> bool {
        // 模拟引擎处理
        // 实际应该调用: engine.process_tick(...)
        // let tick = &timed_tick.tick;

        // 模拟处理时间（1-5ms）
        let process_time = Duration::from_micros(1000 + (self.rand_simple() % 4000) as u64);

        tokio::time::sleep(process_time).await;

        // 模拟偶尔失败（1%概率）
        if self.rand_simple() % 100 == 0 {
            return false;
        }

        true
    }

    /// 简单随机数
    fn rand_simple(&self) -> u64 {
        use std::time::Instant;
        static LAST: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let now = Instant::now();
        let seed = now.elapsed().as_nanos() as u64;
        LAST.fetch_add(seed, std::sync::atomic::Ordering::SeqCst)
    }

    /// 获取总 tick 数
    pub fn total_ticks(&self) -> u64 {
        self.total_ticks
    }
}

/// 模拟账户状态
pub struct SimulatedAccount {
    /// 可用资金
    pub available: Decimal,
    /// 持仓数量
    pub position_qty: Decimal,
    /// 持仓均价
    pub position_price: Decimal,
    /// 持仓方向
    pub side: PositionSide,
}

/// 持仓方向
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PositionSide {
    None,
    Long,
    Short,
}

impl Default for SimulatedAccount {
    fn default() -> Self {
        Self {
            available: Decimal::from(10000),
            position_qty: Decimal::ZERO,
            position_price: Decimal::ZERO,
            side: PositionSide::None,
        }
    }
}

impl SimulatedAccount {
    /// 开多
    pub fn open_long(&mut self, price: Decimal, qty: Decimal) -> Result<(), &'static str> {
        let cost = price * qty;
        if cost > self.available {
            return Err("资金不足");
        }
        // 简化计算
        self.position_qty = qty;
        self.position_price = price;
        self.side = PositionSide::Long;
        Ok(())
    }

    /// 平多
    pub fn close_long(&mut self, price: Decimal) -> Result<Decimal, &'static str> {
        if self.side != PositionSide::Long {
            return Err("无持仓");
        }
        let pnl = (price - self.position_price) * self.position_qty;
        self.position_qty = Decimal::ZERO;
        self.side = PositionSide::None;
        Ok(pnl)
    }

    /// 获取当前持仓市值
    pub fn position_value(&self, price: Decimal) -> Decimal {
        self.position_qty * price
    }

    /// 获取总资产
    pub fn total_assets(&self, price: Decimal) -> Decimal {
        self.available + self.position_value(price)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_account() {
        let mut account = SimulatedAccount::default();

        // 开多
        assert!(account.open_long(dec!(50000), dec!(0.1)).is_ok());
        assert_eq!(account.side, PositionSide::Long);

        // 平多盈利
        let pnl = account.close_long(dec!(51000)).unwrap();
        assert!(pnl > Decimal::ZERO);
    }
}
