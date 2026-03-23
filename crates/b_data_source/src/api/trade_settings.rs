#![forbid(unsafe_code)]

//! 交易设置模块
//!
//! 提供交易前的设置功能：杠杆、持仓模式、手续费率等。

use a_common::api::BinanceApiGateway;
use a_common::EngineError;
use rust_decimal::Decimal;

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

/// 交易设置器
pub struct TradeSettings {
    gateway: BinanceApiGateway,
}

impl TradeSettings {
    /// 创建新的交易设置器
    pub fn new() -> Self {
        Self {
            gateway: BinanceApiGateway::new_futures(),
        }
    }

    /// 使用实盘+测试网账户模式
    pub fn with_testnet() -> Self {
        Self {
            gateway: BinanceApiGateway::new_futures_with_testnet(),
        }
    }

    /// 使用自定义网关
    pub fn with_gateway(gateway: BinanceApiGateway) -> Self {
        Self { gateway }
    }

    /// 设置持仓模式
    ///
    /// # 参数
    /// * `mode` - 持仓模式 (Hedge=双向, OneWay=单向)
    pub async fn set_position_mode(&self, mode: PositionMode) -> Result<bool, EngineError> {
        self.gateway.change_position_mode(mode.as_bool()).await
    }

    /// 设置交易对杠杆
    ///
    /// # 参数
    /// * `symbol` - 交易对，如 "BTCUSDT"
    /// * `leverage` - 杠杆倍数 (1-125)
    pub async fn set_leverage(&self, symbol: &str, leverage: i32) -> Result<bool, EngineError> {
        self.gateway.change_leverage(symbol, leverage).await
    }

    /// 获取交易手续费率
    ///
    /// # 参数
    /// * `symbol` - 交易对，如 "BTCUSDT"
    ///
    /// # 返回
    /// * `(maker_fee, taker_fee)` - (maker费率, taker费率)
    pub async fn get_commission_rate(&self, symbol: &str) -> Result<(Decimal, Decimal), EngineError> {
        self.gateway.get_commission_rate(symbol).await
    }

    /// 设置最高杠杆（自动）
    ///
    /// 自动获取该交易对最大可用杠杆并设置
    pub async fn set_max_leverage(&self, symbol: &str) -> Result<i32, EngineError> {
        let max_leverage = self.gateway.get_max_leverage(symbol).await?;
        self.gateway.change_leverage(symbol, max_leverage).await?;
        tracing::info!(symbol = symbol, leverage = max_leverage, "已设置为最大杠杆");
        Ok(max_leverage)
    }

    /// 一键初始化交易设置
    ///
    /// 设置持仓模式 + 自动设置各交易对最大杠杆
    pub async fn initialize(&self, mode: PositionMode, symbols: &[&str]) -> Result<(), EngineError> {
        // 1. 设置持仓模式
        self.set_position_mode(mode).await?;

        // 2. 设置各交易对最大杠杆
        for symbol in symbols {
            if let Err(e) = self.set_max_leverage(symbol).await {
                tracing::warn!(symbol = symbol, error = %e, "设置杠杆失败");
            }
        }

        Ok(())
    }
}

impl Default for TradeSettings {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_mode_as_bool() {
        assert_eq!(PositionMode::Hedge.as_bool(), true);
        assert_eq!(PositionMode::OneWay.as_bool(), false);
    }

    #[test]
    fn test_trade_settings_creation() {
        let _settings = TradeSettings::new();
        let _settings_testnet = TradeSettings::with_testnet();
    }
}
