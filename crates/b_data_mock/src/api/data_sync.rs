//! 模拟合约数据同步服务
//!
//! 使用 MockAccount 获取模拟账户和持仓数据

use rust_decimal::Decimal;
use crate::api::account::FuturesAccountData;
use crate::api::position::FuturesPositionData;
use a_common::MarketError;

/// 模拟数据同步器
pub struct FuturesDataSyncer {
    #[allow(dead_code)]
    mock_account: Option<crate::api::mock_account::Account>,
}

impl FuturesDataSyncer {
    pub fn new() -> Self {
        Self {
            mock_account: None,
        }
    }

    /// 创建带模拟账户的同步器
    pub fn with_mock_account(account: crate::api::mock_account::Account) -> Self {
        Self {
            mock_account: Some(account),
        }
    }

    /// 一键获取并同步账户+持仓数据
    pub async fn sync_all(&self) -> Result<FuturesSyncResult, MarketError> {
        let account = self.fetch_account().await?;
        let positions = self.fetch_positions().await?;

        Ok(FuturesSyncResult {
            account,
            positions,
        })
    }

    /// 仅获取账户数据
    pub async fn fetch_account(&self) -> Result<FuturesAccountData, MarketError> {
        if let Some(ref account) = self.mock_account {
            let ex_account = account.to_exchange_account();
            return Ok(FuturesAccountData::from_mock(
                ex_account.total_equity,
                ex_account.available,
                ex_account.frozen_margin,
                ex_account.unrealized_pnl,
            ));
        }

        Ok(FuturesAccountData {
            total_margin_balance: Decimal::ZERO,
            unrealized_pnl: Decimal::ZERO,
            available: Decimal::ZERO,
            margin_used: Decimal::ZERO,
            effective_margin: Decimal::ZERO,
            update_time: 0,
        })
    }

    /// 仅获取持仓数据
    pub async fn fetch_positions(&self) -> Result<Vec<FuturesPositionData>, MarketError> {
        if let Some(ref account) = self.mock_account {
            let positions = account.get_all_positions();
            return Ok(positions.into_iter().map(|(symbol, pos)| {
                FuturesPositionData {
                    symbol,
                    side: if pos.long_qty > Decimal::ZERO {
                        "LONG".to_string()
                    } else {
                        "SHORT".to_string()
                    },
                    qty: pos.long_qty + pos.short_qty,
                    entry_price: if pos.long_qty > Decimal::ZERO {
                        pos.long_entry_price
                    } else {
                        pos.short_entry_price
                    },
                    mark_price: Decimal::ZERO,
                    unrealized_pnl: pos.total_unrealized_pnl(Decimal::ZERO),
                    leverage: 10,
                }
            }).collect());
        }

        Ok(Vec::new())
    }
}

impl Default for FuturesDataSyncer {
    fn default() -> Self {
        Self::new()
    }
}

/// 同步结果
#[derive(Debug, Clone)]
pub struct FuturesSyncResult {
    pub account: FuturesAccountData,
    pub positions: Vec<FuturesPositionData>,
}
