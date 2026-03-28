//! 模拟交易设置
//!
//! 用于沙盒测试，不调用真实 API

use rust_decimal::Decimal;
use rust_decimal_macros::dec;

/// 持仓模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionMode {
    /// 双向持仓模式（对冲模式）
    Hedge,
    /// 单向持仓模式
    OneWay,
}

impl PositionMode {
    pub fn as_bool(&self) -> bool {
        match self {
            PositionMode::Hedge => true,
            PositionMode::OneWay => false,
        }
    }
}

/// 模拟交易设置器
pub struct TradeSettings {
    /// 当前持仓模式
    position_mode: PositionMode,
    /// 各品种杠杆
    leverage: std::collections::HashMap<String, i32>,
    /// 手续费率
    fee_rate: Decimal,
}

impl TradeSettings {
    pub fn new() -> Self {
        Self {
            position_mode: PositionMode::OneWay,
            leverage: std::collections::HashMap::new(),
            fee_rate: dec!(0.0004),
        }
    }

    /// 设置持仓模式
    pub async fn set_position_mode(&mut self, mode: PositionMode) -> Result<bool, a_common::EngineError> {
        self.position_mode = mode;
        tracing::info!("Mock: position mode set to {:?}", mode);
        Ok(true)
    }

    /// 设置杠杆
    pub async fn set_leverage(&mut self, symbol: &str, leverage: i32) -> Result<bool, a_common::EngineError> {
        self.leverage.insert(symbol.to_uppercase(), leverage);
        tracing::info!(symbol = symbol, leverage = leverage, "Mock: leverage set");
        Ok(true)
    }

    /// 获取手续费率
    pub fn get_fee_rate(&self) -> Decimal {
        self.fee_rate
    }

    /// 获取品种杠杆
    pub fn get_leverage(&self, symbol: &str) -> i32 {
        self.leverage.get(&symbol.to_uppercase()).copied().unwrap_or(1)
    }

    /// 获取持仓模式
    pub fn get_position_mode(&self) -> PositionMode {
        self.position_mode
    }
}

impl Default for TradeSettings {
    fn default() -> Self {
        Self::new()
    }
}
