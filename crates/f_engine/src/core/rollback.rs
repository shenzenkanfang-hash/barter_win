//! 回滚管理器
//!
//! 处理交易失败、部分成交等异常情况下的回滚和补偿。

#![forbid(unsafe_code)]

use rust_decimal::Decimal;

use crate::core::fund_pool::FundPoolManager;
use crate::core::ChannelType;

/// 回滚类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RollbackType {
    /// 订单发送失败
    OrderSendFailed,
    /// 订单超时
    OrderTimeout,
    /// 部分成交
    PartialFilled,
    /// 状态不一致
    StateInconsistent,
}

/// 回滚结果
#[derive(Debug, Clone)]
pub struct RollbackResult {
    /// 是否成功
    pub success: bool,
    /// 回滚类型
    pub rollback_type: RollbackType,
    /// 消息
    pub message: String,
}

impl RollbackResult {
    pub fn success(rollback_type: RollbackType, message: impl Into<String>) -> Self {
        Self {
            success: true,
            rollback_type,
            message: message.into(),
        }
    }

    pub fn failure(rollback_type: RollbackType, message: impl Into<String>) -> Self {
        Self {
            success: false,
            rollback_type,
            message: message.into(),
        }
    }
}

/// 回滚管理器
pub struct RollbackManager {
    fund_pool_manager: FundPoolManager,
}

impl RollbackManager {
    pub fn new(fund_pool_manager: FundPoolManager) -> Self {
        Self { fund_pool_manager }
    }

    /// 回滚订单资金
    ///
    /// 下单失败后，释放冻结的资金。
    pub fn rollback_order(
        &self,
        channel_type: ChannelType,
        frozen_amount: Decimal,
    ) -> RollbackResult {
        self.fund_pool_manager.rollback(channel_type, frozen_amount);

        RollbackResult::success(
            RollbackType::OrderSendFailed,
            format!("已回滚冻结资金: {:?}", frozen_amount),
        )
    }

    /// 回滚部分成交
    ///
    /// 部分成交后，撤单并回滚剩余冻结资金。
    pub fn rollback_partial_fill(
        &self,
        channel_type: ChannelType,
        filled_amount: Decimal,
        frozen_amount: Decimal,
    ) -> RollbackResult {
        // 释放未成交部分的冻结资金
        let remaining = frozen_amount - filled_amount;
        if remaining > Decimal::ZERO {
            self.fund_pool_manager.rollback(channel_type, remaining);
        }

        RollbackResult::success(
            RollbackType::PartialFilled,
            format!("部分成交回滚: 成交={:?}, 回滚={:?}", filled_amount, remaining),
        )
    }

    /// 回滚状态不一致
    ///
    /// 状态不一致后，重新对齐状态。
    pub fn rollback_state_inconsistency(
        &self,
        channel_type: ChannelType,
        local_amount: Decimal,
        exchange_amount: Decimal,
    ) -> RollbackResult {
        // 计算差异
        let diff = local_amount - exchange_amount;

        if diff > Decimal::ZERO {
            // 本地比交易所多，需要回滚
            self.fund_pool_manager.rollback(channel_type, diff);
        }

        RollbackResult::success(
            RollbackType::StateInconsistent,
            format!(
                "状态不一致回滚: 本地={:?}, 交易所={:?}, 差异={:?}",
                local_amount, exchange_amount, diff
            ),
        )
    }

    /// 获取资金池管理器
    pub fn fund_pool_manager(&self) -> &FundPoolManager {
        &self.fund_pool_manager
    }
}

/// 订单回滚助手
pub struct OrderRollbackHelper {
    channel_type: ChannelType,
    order_id: String,
    quantity: Decimal,
    target_price: Decimal,
    frozen_amount: Decimal,
}

impl OrderRollbackHelper {
    pub fn new(
        order_id: String,
        channel_type: ChannelType,
        quantity: Decimal,
        target_price: Decimal,
    ) -> Self {
        let frozen_amount = quantity * target_price;
        Self {
            channel_type,
            order_id,
            quantity,
            target_price,
            frozen_amount,
        }
    }

    /// 获取订单价值
    pub fn order_value(&self) -> Decimal {
        self.frozen_amount
    }

    /// 获取通道类型
    pub fn channel_type(&self) -> ChannelType {
        self.channel_type
    }

    /// 获取订单ID
    pub fn order_id(&self) -> &str {
        &self.order_id
    }

    /// 获取冻结金额
    pub fn frozen_amount(&self) -> Decimal {
        self.frozen_amount
    }
}
