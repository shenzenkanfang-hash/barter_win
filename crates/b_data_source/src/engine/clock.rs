//! 引擎时钟系统
//!
//! 支持两种时钟模式：
//! - `LiveClock`: 实盘使用，返回 `Utc::now()`
//! - `HistoricalClock`: 回测使用，基于事件时间戳推进

use chrono::{DateTime, Utc};
use std::sync::Arc;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

/// 时钟 trait
pub trait EngineClock: Send + Sync {
    /// 获取当前时间
    fn time(&self) -> DateTime<Utc>;
}

/// 实时时钟（实盘使用）
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct LiveClock;

impl EngineClock for LiveClock {
    fn time(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// 历史时钟（回测使用）
///
/// 基于事件的交易所时间戳推进时钟，
/// 同时记录真实流逝时间用于性能测量
#[derive(Debug, Clone)]
pub struct HistoricalClock {
    inner: Arc<RwLock<HistoricalClockInner>>,
}

#[derive(Debug, Clone)]
struct HistoricalClockInner {
    /// 最后一个事件的交易所时间
    time_exchange_last: DateTime<Utc>,
    /// 收到最后一个事件的真实时间（用于计算流逝）
    time_live_last_event: DateTime<Utc>,
}

impl HistoricalClock {
    /// 从第一个事件时间创建时钟
    pub fn new(first_event_time: DateTime<Utc>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(HistoricalClockInner {
                time_exchange_last: first_event_time,
                time_live_last_event: Utc::now(),
            })),
        }
    }

    /// 从 DateTime 直接创建（用于测试）
    pub fn from_datetime(time: DateTime<Utc>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(HistoricalClockInner {
                time_exchange_last: time,
                time_live_last_event: time,
            })),
        }
    }

    /// 更新时钟（基于事件时间戳）
    /// 只有事件时间更晚时才更新（忽略乱序）
    pub fn update(&self, time: DateTime<Utc>) {
        let mut inner = self.inner.write();
        if time >= inner.time_exchange_last {
            inner.time_exchange_last = time;
            inner.time_live_last_event = Utc::now();
        }
    }
}

impl EngineClock for HistoricalClock {
    fn time(&self) -> DateTime<Utc> {
        let inner = self.inner.read();

        // 计算从上一个事件到现在真实流逝的时间
        let delta = Utc::now().signed_duration_since(inner.time_live_last_event);

        // 如果流逝时间为正，则加上；否则返回上一个事件时间
        match delta.num_milliseconds() >= 0 {
            true => inner.time_exchange_last.checked_add_signed(delta).unwrap_or(inner.time_exchange_last),
            false => inner.time_exchange_last,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeDelta;

    #[test]
    fn test_live_clock() {
        let clock = LiveClock;
        let t1 = clock.time();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let t2 = clock.time();
        assert!(t2 >= t1);
    }

    #[test]
    fn test_historical_clock_update() {
        use chrono::Months;

        let base = DateTime::from_timestamp(1700000000, 0).unwrap();
        let clock = HistoricalClock::new(base);

        assert!(clock.time() >= base);

        // 更新到更晚的时间
        let later = base + TimeDelta::hours(1);
        clock.update(later);

        // 时间应该更新
        let current = clock.time();
        assert!(current >= later - chrono::Duration::seconds(1));
    }

    #[test]
    fn test_historical_clock_ignores_old_events() {
        let base = DateTime::from_timestamp(1700000000, 0).unwrap();
        let clock = HistoricalClock::new(base);

        // 尝试用更早的时间更新
        let earlier = base - chrono::TimeDelta::hours(1);
        clock.update(earlier);

        // 时间不应该回退
        assert!(clock.time() >= base);
    }
}
