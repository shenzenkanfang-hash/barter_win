//! MockApiGateway 配置模块

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

/// MockApiGateway 配置（向后兼容）
#[derive(Debug, Clone)]
pub struct MockConfig {
    pub initial_balance: Decimal,
    pub fee_rate: Decimal,
    pub slippage_rate: Decimal,
    pub maintenance_margin_rate: Decimal,
    pub max_position_ratio: Decimal,
    pub min_reserve_ratio: Decimal,
}

impl Default for MockConfig {
    fn default() -> Self {
        Self {
            initial_balance: dec!(100000.0),
            fee_rate: dec!(0.0004),
            slippage_rate: dec!(0.0),
            maintenance_margin_rate: dec!(0.005),
            max_position_ratio: dec!(0.95),
            min_reserve_ratio: dec!(0.05),
        }
    }
}

impl MockConfig {
    pub fn new(initial_balance: Decimal) -> Self {
        Self {
            initial_balance,
            ..Default::default()
        }
    }

    pub fn with_fee_rate(mut self, fee_rate: Decimal) -> Self {
        self.fee_rate = fee_rate;
        self
    }

    pub fn with_slippage(mut self, slippage_rate: Decimal) -> Self {
        self.slippage_rate = slippage_rate;
        self
    }
}

/// 模拟执行配置（新结构，细粒度控制）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockExecutionConfig {
    /// 模拟网络延迟（毫秒）
    pub latency_ms: u64,
    /// maker 手续费率（百分比，如 0.1 表示 0.1%）
    pub maker_fee: Decimal,
    /// taker 手续费率
    pub taker_fee: Decimal,
    /// 滑点率（百分比）
    pub slippage: Decimal,
    /// 初始余额
    pub initial_balance: Decimal,
}

impl Default for MockExecutionConfig {
    fn default() -> Self {
        Self {
            latency_ms: 50,
            maker_fee: dec!(0.02),
            taker_fee: dec!(0.04),
            slippage: dec!(0.01),
            initial_balance: dec!(10000),
        }
    }
}

impl MockExecutionConfig {
    pub fn with_balance(mut self, balance: Decimal) -> Self {
        self.initial_balance = balance;
        self
    }

    pub fn with_latency(mut self, ms: u64) -> Self {
        self.latency_ms = ms;
        self
    }
}
