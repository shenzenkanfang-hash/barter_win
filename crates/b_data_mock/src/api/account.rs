//! 模拟 USDT 合约账户数据
//!
//! 使用 MockAccount 替代真实 Binance API

use rust_decimal::Decimal;

/// USDT 合约账户数据获取器（模拟）
pub struct FuturesAccount {
    /// 内部引用 MockAccount
    #[allow(dead_code)]
    mock_account: Option<crate::api::mock_account::Account>,
}

impl FuturesAccount {
    pub fn new() -> Self {
        Self {
            mock_account: None,
        }
    }

    /// 从模拟账户获取账户数据
    pub async fn fetch(&self) -> Result<FuturesAccountData, a_common::MarketError> {
        // 如果有 mock_account，从其中获取
        if let Some(ref account) = self.mock_account {
            let ex_account = account.to_exchange_account();
            return Ok(FuturesAccountData {
                total_margin_balance: ex_account.total_equity,
                unrealized_pnl: ex_account.unrealized_pnl,
                available: ex_account.available,
                margin_used: ex_account.frozen_margin,
                effective_margin: ex_account.total_equity,
                update_time: ex_account.update_ts,
            });
        }

        // 默认返回空数据
        Ok(FuturesAccountData {
            total_margin_balance: Decimal::ZERO,
            unrealized_pnl: Decimal::ZERO,
            available: Decimal::ZERO,
            margin_used: Decimal::ZERO,
            effective_margin: Decimal::ZERO,
            update_time: 0,
        })
    }
}

impl Default for FuturesAccount {
    fn default() -> Self {
        Self::new()
    }
}

/// USDT 合约账户数据
#[derive(Debug, Clone)]
pub struct FuturesAccountData {
    pub total_margin_balance: Decimal,
    pub unrealized_pnl: Decimal,
    pub available: Decimal,
    pub margin_used: Decimal,
    pub effective_margin: Decimal,
    pub update_time: i64,
}

impl FuturesAccountData {
    /// 从模拟账户数据创建
    pub fn from_mock(
        total_equity: Decimal,
        available: Decimal,
        frozen_margin: Decimal,
        unrealized_pnl: Decimal,
    ) -> Self {
        Self {
            total_margin_balance: total_equity,
            unrealized_pnl,
            available,
            margin_used: frozen_margin,
            effective_margin: total_equity + unrealized_pnl,
            update_time: chrono::Utc::now().timestamp(),
        }
    }
}
