#![forbid(unsafe_code)]

//! USDT 合约持仓数据获取模块
//!
//! 纯数据获取层，只做字段解析，不涉及业务逻辑判断。

use a_common::api::{BinanceApiGateway, FuturesPositionResponse};
use rust_decimal::Decimal;
use std::str::FromStr;

/// USDT 合约持仓数据获取器
pub struct FuturesPosition {
    gateway: BinanceApiGateway,
}

impl FuturesPosition {
    /// 创建新的持仓数据获取器
    pub fn new() -> Self {
        Self {
            gateway: BinanceApiGateway::new_futures(),
        }
    }

    /// 从币安 API 获取持仓数据
    pub async fn fetch(&self) -> Result<Vec<FuturesPositionData>, a_common::MarketError> {
        let response = self
            .gateway
            .fetch_futures_positions()
            .await
            .map_err(|e| a_common::MarketError::NetworkError(e.to_string()))?;

        Ok(response
            .into_iter()
            .map(FuturesPositionData::from_response)
            .collect())
    }
}

impl Default for FuturesPosition {
    fn default() -> Self {
        Self::new()
    }
}

/// USDT 合约持仓数据（解析后的结构）
#[derive(Debug, Clone)]
pub struct FuturesPositionData {
    /// 交易对
    pub symbol: String,
    /// 持仓方向: LONG / SHORT
    pub side: String,
    /// 持仓数量
    pub qty: Decimal,
    /// 入场价格
    pub entry_price: Decimal,
    /// 标记价格
    pub mark_price: Decimal,
    /// 未实现盈亏
    pub unrealized_pnl: Decimal,
    /// 杠杆倍数
    pub leverage: i32,
}

impl FuturesPositionData {
    /// 从 API 响应解析
    pub fn from_response(resp: FuturesPositionResponse) -> Self {
        let qty = Decimal::from_str(&resp.position_amt).unwrap_or_default();
        let entry_price = Decimal::from_str(&resp.entry_price).unwrap_or_default();
        let mark_price = Decimal::from_str(&resp.mark_price).unwrap_or_default();
        let unrealized_pnl = Decimal::from_str(&resp.unrealized_profit).unwrap_or_default();
        let leverage = resp.leverage.parse().unwrap_or(1);

        Self {
            symbol: resp.symbol,
            side: resp.position_side,
            qty,
            entry_price,
            mark_price,
            unrealized_pnl,
            leverage,
        }
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_futures_position_creation() {
        let _position = FuturesPosition::new();
    }

    #[test]
    fn test_futures_position_data_from_response() {
        let json = r#"{
            "symbol": "BTCUSDT",
            "positionSide": "LONG",
            "positionAmt": "1.5",
            "entryPrice": "50000.00",
            "markPrice": "51000.00",
            "unrealizedProfit": "1500.00",
            "leverage": "10",
            "marginRatio": "0.02"
        }"#;

        let resp: FuturesPositionResponse = serde_json::from_str(json).unwrap();
        let data = FuturesPositionData::from_response(resp);

        assert_eq!(data.symbol, "BTCUSDT");
        assert_eq!(data.side, "LONG");
        assert_eq!(data.qty, Decimal::from_str("1.5").unwrap());
        assert_eq!(data.entry_price, Decimal::from_str("50000.00").unwrap());
        assert_eq!(data.mark_price, Decimal::from_str("51000.00").unwrap());
        assert_eq!(data.unrealized_pnl, Decimal::from_str("1500.00").unwrap());
        assert_eq!(data.leverage, 10);
    }

    #[test]
    fn test_futures_position_data_default_values() {
        let json = r#"{
            "symbol": "",
            "positionSide": "",
            "positionAmt": "",
            "entryPrice": "",
            "markPrice": "",
            "unrealizedProfit": "",
            "leverage": "",
            "marginRatio": ""
        }"#;

        let resp: FuturesPositionResponse = serde_json::from_str(json).unwrap();
        let data = FuturesPositionData::from_response(resp);

        assert_eq!(data.symbol, "");
        assert_eq!(data.side, "");
        assert_eq!(data.qty, Decimal::ZERO);
        assert_eq!(data.entry_price, Decimal::ZERO);
        assert_eq!(data.mark_price, Decimal::ZERO);
        assert_eq!(data.unrealized_pnl, Decimal::ZERO);
        assert_eq!(data.leverage, 1); // 默认杠杆为 1
    }

    #[test]
    fn test_futures_position_data_short_side() {
        let json = r#"{
            "symbol": "ETHUSDT",
            "positionSide": "SHORT",
            "positionAmt": "10.0",
            "entryPrice": "2000.00",
            "markPrice": "1900.00",
            "unrealizedProfit": "1000.00",
            "leverage": "5",
            "marginRatio": "0.01"
        }"#;

        let resp: FuturesPositionResponse = serde_json::from_str(json).unwrap();
        let data = FuturesPositionData::from_response(resp);

        assert_eq!(data.symbol, "ETHUSDT");
        assert_eq!(data.side, "SHORT");
        assert_eq!(data.leverage, 5);
    }
}
