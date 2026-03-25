//! 资金池管理器

#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use std::sync::Arc;
use parking_lot::RwLock;

/// 资金池管理器
pub struct FundPoolManager {
    /// 分钟级资金池
    minute_pool: Arc<RwLock<FundPool>>,
    /// 日线级资金池
    daily_pool: Arc<RwLock<FundPool>>,
}

impl FundPoolManager {
    pub fn new(minute_allocated: Decimal, daily_allocated: Decimal) -> Self {
        Self {
            minute_pool: Arc::new(RwLock::new(FundPool::new("minute", minute_allocated))),
            daily_pool: Arc::new(RwLock::new(FundPool::new("daily", daily_allocated))),
        }
    }
}
