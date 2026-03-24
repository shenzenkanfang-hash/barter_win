//! 风控管理模块
//!
//! 实现两级风控检查：
//! - 一次检查（锁外）：轻量快速
//! - 二次检查（锁内）：精确一致

#![forbid(unsafe_code)]

use rust_decimal::Decimal;

use crate::core::fund_pool::FundPoolManager;
use crate::core::{
    StrategyResponse, RiskCheckResult, ChannelType,
};

/// 风控配置
#[derive(Debug, Clone)]
pub struct RiskConfig {
    /// 池使用率阈值（超过此值只平不开）
    pub pool_usage_threshold: Decimal,
    /// 最大同时运行品种数
    pub max_running_symbols: usize,
    /// 单品种最大订单价值
    pub max_order_value: Decimal,
    /// 最小订单价值
    pub min_order_value: Decimal,
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            pool_usage_threshold: Decimal::from(80) / Decimal::from(100), // 80%
            max_running_symbols: 10,
            max_order_value: Decimal::from(10000),
            min_order_value: Decimal::from(10),
        }
    }
}

impl RiskConfig {
    pub fn production() -> Self {
        Self::default()
    }

    pub fn backtest() -> Self {
        Self {
            pool_usage_threshold: Decimal::from(95) / Decimal::from(100),
            max_running_symbols: 50,
            max_order_value: Decimal::from(50000),
            min_order_value: Decimal::from(1),
        }
    }
}

/// 风控管理器
pub struct RiskManager {
    config: RiskConfig,
    fund_pool_manager: FundPoolManager,
}

impl RiskManager {
    pub fn new(config: RiskConfig, fund_pool_manager: FundPoolManager) -> Self {
        Self {
            config,
            fund_pool_manager,
        }
    }

    /// 风控一次预检（锁外）
    ///
    /// 快速、轻量，用于快速拒绝无效请求。
    ///
    /// # 检查项
    /// - 账户风险状态
    /// - 资金池余额
    /// - 品种数量上限
    /// - 订单价值范围
    pub fn pre_check(
        &self,
        response: &StrategyResponse,
        account_can_trade: bool,
        current_symbols: usize,
    ) -> RiskCheckResult {
        // 1. 检查是否需要执行
        if !response.should_execute {
            return RiskCheckResult::new(false, false);
        }

        // 2. 检查账户状态
        if !account_can_trade {
            return RiskCheckResult::new(false, false);
        }

        // 3. 检查品种数量上限
        if current_symbols >= self.config.max_running_symbols {
            return RiskCheckResult::new(false, false);
        }

        // 4. 检查资金池是否充足
        let channel_type = response.channel_type;
        let required = response.quantity * response.target_price;
        if !self.fund_pool_manager.has_sufficient_funds(channel_type, required) {
            return RiskCheckResult::new(false, false);
        }

        // 5. 检查订单价值范围
        let order_value = response.quantity * response.target_price;
        if order_value > self.config.max_order_value {
            return RiskCheckResult::new(false, false);
        }
        if order_value < self.config.min_order_value {
            return RiskCheckResult::new(false, false);
        }

        // 6. 检查池使用率（超过阈值只平不开）
        let usage_rate = match channel_type {
            ChannelType::HighSpeed => self.fund_pool_manager.minute_usage_rate(),
            ChannelType::LowSpeed => self.fund_pool_manager.daily_usage_rate(),
        };

        let is_opening = !matches!(
            response.action,
            c_data_process::types::TradingAction::Flat
        );
        if is_opening && usage_rate >= self.config.pool_usage_threshold {
            return RiskCheckResult::new(false, false);
        }

        RiskCheckResult::new(true, false)
    }

    /// 风控二次检查（加锁后）
    ///
    /// 精确、一致，用于确保执行前状态正确。
    ///
    /// # 检查项
    /// - 实时价格偏差
    /// - 实时持仓状态
    /// - 资金池实时余额
    pub fn lock_check(
        &self,
        response: &StrategyResponse,
        current_price: Decimal,
        exchange_position_qty: Decimal,
        exchange_available: Decimal,
    ) -> RiskCheckResult {
        // 1. 检查是否需要执行
        if !response.should_execute {
            return RiskCheckResult::new(true, false);
        }

        // 2. 检查价格偏差
        if response.target_price > Decimal::ZERO {
            let deviation = (current_price - response.target_price).abs() / current_price;
            let max_deviation = Decimal::from(5) / Decimal::from(100); // 5%
            if deviation > max_deviation {
                return RiskCheckResult::new(true, false);
            }
        }

        // 3. 检查实时持仓状态
        let is_opening = !matches!(
            response.action,
            c_data_process::types::TradingAction::Flat
        );

        // 开仓时检查是否已有反向持仓
        if is_opening && exchange_position_qty > Decimal::ZERO {
            // 已有持仓，不能重复开仓
            return RiskCheckResult::new(true, false);
        }

        // 平仓时检查是否真的有持仓
        if !is_opening && exchange_position_qty <= Decimal::ZERO {
            // 没有持仓，不需要平
            return RiskCheckResult::new(true, false);
        }

        // 4. 检查实时资金池余额
        let required = response.quantity * response.target_price;
        if !self.fund_pool_manager.has_sufficient_funds(response.channel_type, required) {
            return RiskCheckResult::new(true, false);
        }

        // 5. 检查账户实时可用资金
        if exchange_available < required {
            return RiskCheckResult::new(true, false);
        }

        RiskCheckResult::new(true, true)
    }

    /// 记录风控拒绝（用于监控）
    pub fn record_rejection(&self, reason: &str) {
        tracing::warn!("风控拒绝: {}", reason);
    }

    /// 获取配置
    pub fn config(&self) -> &RiskConfig {
        &self.config
    }

    /// 获取资金池管理器
    pub fn fund_pool_manager(&self) -> &FundPoolManager {
        &self.fund_pool_manager
    }
}
