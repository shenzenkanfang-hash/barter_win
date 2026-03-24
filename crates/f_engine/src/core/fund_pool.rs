//! 资金池管理模块
//!
//! 实现分钟级和日线级资金池，支持严格扣减→执行→返还回滚机制。

#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use std::sync::Arc;
use parking_lot::RwLock;

use crate::core::{ChannelType, FundPool};

/// 资金池管理器
pub struct FundPoolManager {
    /// 分钟级资金池
    minute_pool: Arc<RwLock<FundPool>>,
    /// 日线级资金池
    daily_pool: Arc<RwLock<FundPool>>,
}

impl FundPoolManager {
    /// 创建新的资金池管理器
    pub fn new(
        minute_allocated: Decimal,
        daily_allocated: Decimal,
    ) -> Self {
        Self {
            minute_pool: Arc::new(RwLock::new(FundPool::new("minute", minute_allocated))),
            daily_pool: Arc::new(RwLock::new(FundPool::new("daily", daily_allocated))),
        }
    }

    /// 根据通道类型获取资金池
    pub fn get_pool(&self, channel_type: ChannelType) -> Arc<RwLock<FundPool>> {
        match channel_type {
            ChannelType::HighSpeed => Arc::clone(&self.minute_pool),
            ChannelType::LowSpeed => Arc::clone(&self.daily_pool),
        }
    }

    /// 冻结资金
    ///
    /// # 返回
    /// - `true` 冻结成功
    /// - `false` 资金不足
    pub fn freeze(&self, channel_type: ChannelType, amount: Decimal) -> bool {
        let pool = self.get_pool(channel_type);
        let result = pool.write().freeze(amount);
        result
    }

    /// 确认使用（从冻结转为已使用）
    pub fn confirm_usage(&self, channel_type: ChannelType, amount: Decimal) {
        let pool = self.get_pool(channel_type);
        pool.write().confirm_usage(amount);
    }

    /// 释放冻结（回滚）
    pub fn release_frozen(&self, channel_type: ChannelType, amount: Decimal) {
        let pool = self.get_pool(channel_type);
        pool.write().release_frozen(amount);
    }

    /// 回滚（释放冻结）
    pub fn rollback(&self, channel_type: ChannelType, amount: Decimal) {
        let pool = self.get_pool(channel_type);
        pool.write().rollback(amount);
    }

    /// 获取可用资金
    pub fn available(&self, channel_type: ChannelType) -> Decimal {
        let pool = self.get_pool(channel_type);
        let available = pool.read().available();
        available
    }

    /// 检查资金是否充足
    pub fn has_sufficient_funds(&self, channel_type: ChannelType, amount: Decimal) -> bool {
        self.available(channel_type) >= amount
    }

    /// 获取分钟级资金池信息
    pub fn minute_pool_info(&self) -> (Decimal, Decimal, Decimal) {
        let pool = self.minute_pool.read();
        (pool.allocated, pool.used, pool.frozen)
    }

    /// 获取日线级资金池信息
    pub fn daily_pool_info(&self) -> (Decimal, Decimal, Decimal) {
        let pool = self.daily_pool.read();
        (pool.allocated, pool.used, pool.frozen)
    }

    /// 获取分钟级资金池使用率
    pub fn minute_usage_rate(&self) -> Decimal {
        self.minute_pool.read().usage_rate()
    }

    /// 获取日线级资金池使用率
    pub fn daily_usage_rate(&self) -> Decimal {
        self.daily_pool.read().usage_rate()
    }

    /// 检查资源是否满
    ///
    /// # 参数
    /// - channel_type: 通道类型
    /// - max_symbols: 最大品种数
    /// - current_symbols: 当前品种数
    ///
    /// # 返回
    /// - `true` 资源满
    /// - `false` 资源未满
    pub fn is_resource_full(
        &self,
        channel_type: ChannelType,
        _max_symbols: usize,
        _current_symbols: usize,
        _amount: Decimal,
    ) -> bool {
        // 检查品种数量
        // if current_symbols >= max_symbols {
        //     return true;
        // }

        // 检查资金池
        self.get_pool(channel_type).read().is_full()
    }
}

impl Clone for FundPoolManager {
    fn clone(&self) -> Self {
        Self {
            minute_pool: Arc::clone(&self.minute_pool),
            daily_pool: Arc::clone(&self.daily_pool),
        }
    }
}
