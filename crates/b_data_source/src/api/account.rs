#![forbid(unsafe_code)]

//! USDT 合约账户数据获取模块
//!
//! 纯数据获取层，只做字段解析，不涉及业务逻辑判断。

use a_common::api::{BinanceApiGateway, FuturesAccountResponse};
use rust_decimal::Decimal;
use std::str::FromStr;

/// USDT 合约账户数据获取器
pub struct FuturesAccount {
    gateway: BinanceApiGateway,
}

impl FuturesAccount {
    /// 创建新的账户数据获取器
    pub fn new() -> Self {
        Self {
            gateway: BinanceApiGateway::new_futures(),
        }
    }

    /// 从币安 API 获取账户数据
    pub async fn fetch(&self) -> Result<FuturesAccountData, a_common::MarketError> {
        let response = self
            .gateway
            .fetch_futures_account()
            .await
            .map_err(|e| a_common::MarketError::NetworkError(e.to_string()))?;

        Ok(FuturesAccountData::from_response(response))
    }
}

impl Default for FuturesAccount {
    fn default() -> Self {
        Self::new()
    }
}

/// USDT 合约账户数据（解析后的结构）
#[derive(Debug, Clone)]
pub struct FuturesAccountData {
    /// 总保证金余额
    pub total_margin_balance: Decimal,
    /// 未实现盈亏
    pub unrealized_pnl: Decimal,
    /// 可用余额
    pub available: Decimal,
    /// 已用保证金
    pub margin_used: Decimal,
    /// 有效保证金 = 总保证金 + 未实现盈亏
    pub effective_margin: Decimal,
    /// 更新时间戳
    pub update_time: i64,
}

impl FuturesAccountData {
    /// 从 API 响应解析
    pub fn from_response(resp: FuturesAccountResponse) -> Self {
        let total_margin_balance =
            Decimal::from_str(&resp.total_margin_balance).unwrap_or_default();
        let unrealized_pnl = Decimal::from_str(&resp.total_unrealized_profit).unwrap_or_default();
        let available = Decimal::from_str(&resp.available_balance).unwrap_or_default();
        let margin_used = Decimal::from_str(&resp.total_maint_margin).unwrap_or_default();

        // 有效保证金 = 总保证金 + 未实现盈亏
        let effective_margin = total_margin_balance + unrealized_pnl;

        Self {
            total_margin_balance,
            unrealized_pnl,
            available,
            margin_used,
            effective_margin,
            update_time: resp.update_time,
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
    fn test_futures_account_creation() {
        let _account = FuturesAccount::new();
    }

    #[test]
    fn test_futures_account_data_from_response() {
        let json = r#"{
            "totalMarginBalance": "10000.00",
            "totalUnrealizedProfit": "500.00",
            "availableBalance": "8000.00",
            "totalMaintMargin": "100.00",
            "maxWithdrawAmount": "8000.00",
            "updateTime": 1234567890,
            "assets": []
        }"#;

        let resp: FuturesAccountResponse = serde_json::from_str(json).unwrap();
        let data = FuturesAccountData::from_response(resp);

        assert_eq!(data.total_margin_balance, Decimal::from_str("10000.00").unwrap());
        assert_eq!(data.unrealized_pnl, Decimal::from_str("500.00").unwrap());
        assert_eq!(data.available, Decimal::from_str("8000.00").unwrap());
        assert_eq!(data.margin_used, Decimal::from_str("100.00").unwrap());
        // 有效保证金 = 总保证金 + 未实现盈亏 = 10000 + 500 = 10500
        assert_eq!(data.effective_margin, Decimal::from_str("10500.00").unwrap());
        assert_eq!(data.update_time, 1234567890);
    }

    #[test]
    fn test_futures_account_data_default_values() {
        let json = r#"{
            "totalMarginBalance": "",
            "totalUnrealizedProfit": "",
            "availableBalance": "",
            "totalMaintMargin": "",
            "maxWithdrawAmount": "",
            "updateTime": 0,
            "assets": []
        }"#;

        let resp: FuturesAccountResponse = serde_json::from_str(json).unwrap();
        let data = FuturesAccountData::from_response(resp);

        assert_eq!(data.total_margin_balance, Decimal::ZERO);
        assert_eq!(data.unrealized_pnl, Decimal::ZERO);
        assert_eq!(data.available, Decimal::ZERO);
        assert_eq!(data.margin_used, Decimal::ZERO);
        assert_eq!(data.update_time, 0);
    }
}
