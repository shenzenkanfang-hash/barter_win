#![forbid(unsafe_code)]

//! 合约数据同步服务
//!
//! 一键获取币安合约账户+持仓数据，进行业务处理后存储到高速盘。
//!
//! # 功能
//! 1. 并发获取账户数据和持仓数据
//! 2. 计算有效保证金 (effective_margin = total_margin + unrealized_pnl)
//! 3. 存储到高速内存盘 (E:/shm/backup/account.json, positions.json)
//!
//! # 与风控的区别
//! 本模块仅做数据获取和整理，不涉及任何风控逻辑。

use super::{FuturesAccountData, FuturesPositionData};
use a_common::api::{BinanceApiGateway, FuturesAccountResponse, FuturesPositionResponse};
use a_common::backup::{AccountSnapshot, MemoryBackup, PositionSnapshot, Positions};
use a_common::MarketError;
use chrono::Utc;
use rust_decimal::Decimal;
use std::str::FromStr;

/// 合约数据同步器
pub struct FuturesDataSyncer {
    gateway: BinanceApiGateway,
    memory_backup: Option<MemoryBackup>,
}

impl FuturesDataSyncer {
    /// 创建新的数据同步器
    pub fn new() -> Self {
        Self {
            gateway: BinanceApiGateway::new_futures(),
            memory_backup: None,
        }
    }

    /// 创建带高速盘备份的数据同步器
    pub fn with_backup(backup: MemoryBackup) -> Self {
        Self {
            gateway: BinanceApiGateway::new_futures(),
            memory_backup: Some(backup),
        }
    }

    /// 一键获取并同步账户+持仓数据
    ///
    /// 并发请求账户和持仓，进行业务处理后存储到高速盘
    pub async fn sync_all(&self) -> Result<FuturesSyncResult, MarketError> {
        // 并发获取账户和持仓数据
        let (account_result, positions_result) = tokio::join!(
            self.gateway.fetch_futures_account(),
            self.gateway.fetch_futures_positions(),
        );

        let account_resp = account_result.map_err(|e| MarketError::NetworkError(e.to_string()))?;
        let positions_resp = positions_result.map_err(|e| MarketError::NetworkError(e.to_string()))?;

        // 处理账户数据
        let account = FuturesAccountData::from_response(account_resp);

        // 处理持仓数据
        let positions: Vec<FuturesPositionData> = positions_resp
            .into_iter()
            .map(FuturesPositionData::from_response)
            .collect();

        // 存储到高速盘
        if let Some(ref backup) = self.memory_backup {
            let now = Utc::now().to_rfc3339();

            // 保存账户快照
            let account_snapshot = AccountSnapshot {
                equity: account.total_margin_balance,
                available: account.available,
                frozen: account.margin_used,
                unrealized_pnl: account.unrealized_pnl,
                updated_at: now.clone(),
            };
            if let Err(e) = backup.save_account(&account_snapshot).await {
                tracing::warn!(error = %e, "保存账户快照失败");
            }

            // 保存持仓快照
            let positions_data = Positions {
                positions: positions.iter().map(|p| {
                    PositionSnapshot {
                        symbol: p.symbol.clone(),
                        long_qty: if p.side == "LONG" { p.qty } else { Decimal::ZERO },
                        long_avg_price: if p.side == "LONG" { p.entry_price } else { Decimal::ZERO },
                        short_qty: if p.side == "SHORT" { p.qty.abs() } else { Decimal::ZERO },
                        short_avg_price: if p.side == "SHORT" { p.entry_price } else { Decimal::ZERO },
                        updated_at: now.clone(),
                    }
                }).collect(),
                updated_at: now,
            };
            if let Err(e) = backup.save_positions(&positions_data).await {
                tracing::warn!(error = %e, "保存持仓快照失败");
            }

            tracing::info!(
                account = %account.total_margin_balance,
                positions = positions.len(),
                "合约数据同步完成"
            );
        }

        Ok(FuturesSyncResult {
            account,
            positions,
        })
    }

    /// 仅获取账户数据
    pub async fn fetch_account(&self) -> Result<FuturesAccountData, MarketError> {
        let resp = self.gateway
            .fetch_futures_account()
            .await
            .map_err(|e| MarketError::NetworkError(e.to_string()))?;
        Ok(FuturesAccountData::from_response(resp))
    }

    /// 仅获取持仓数据
    pub async fn fetch_positions(&self) -> Result<Vec<FuturesPositionData>, MarketError> {
        let resp = self.gateway
            .fetch_futures_positions()
            .await
            .map_err(|e| MarketError::NetworkError(e.to_string()))?;
        Ok(resp.into_iter().map(FuturesPositionData::from_response).collect())
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

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_futures_data_syncer_creation() {
        let _syncer = FuturesDataSyncer::new();
    }

    #[test]
    fn test_futures_sync_result() {
        let account = FuturesAccountData {
            total_margin_balance: Decimal::from_str("10000").unwrap(),
            unrealized_pnl: Decimal::from_str("500").unwrap(),
            available: Decimal::from_str("8000").unwrap(),
            margin_used: Decimal::from_str("200").unwrap(),
            effective_margin: Decimal::from_str("10500").unwrap(),
            update_time: 1234567890,
        };

        let positions = vec![FuturesPositionData {
            symbol: "BTCUSDT".to_string(),
            side: "LONG".to_string(),
            qty: Decimal::from_str("1.5").unwrap(),
            entry_price: Decimal::from_str("50000").unwrap(),
            mark_price: Decimal::from_str("51000").unwrap(),
            unrealized_pnl: Decimal::from_str("1500").unwrap(),
            leverage: 10,
        }];

        let result = FuturesSyncResult { account, positions };
        assert_eq!(result.account.effective_margin, Decimal::from_str("10500").unwrap());
        assert_eq!(result.positions.len(), 1);
    }
}
