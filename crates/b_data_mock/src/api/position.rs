//! 模拟 USDT 合约持仓数据
//!
//! 使用 MockAccount 替代真实 Binance API

use rust_decimal::Decimal;

/// USDT 合约持仓数据获取器（模拟）
pub struct FuturesPosition {
    #[allow(dead_code)]
    mock_account: Option<crate::api::mock_account::Account>,
}

impl FuturesPosition {
    pub fn new() -> Self {
        Self {
            mock_account: None,
        }
    }

    /// 从模拟账户获取持仓数据
    pub async fn fetch(&self) -> Result<Vec<FuturesPositionData>, a_common::MarketError> {
        if let Some(ref account) = self.mock_account {
            let positions = account.get_all_positions();
            return Ok(positions.into_iter().map(|(symbol, pos)| {
                FuturesPositionData {
                    symbol,
                    side: if pos.long_qty > Decimal::ZERO {
                        "LONG".to_string()
                    } else if pos.short_qty > Decimal::ZERO {
                        "SHORT".to_string()
                    } else {
                        "NONE".to_string()
                    },
                    qty: pos.long_qty + pos.short_qty,
                    entry_price: if pos.long_qty > Decimal::ZERO {
                        pos.long_entry_price
                    } else {
                        pos.short_entry_price
                    },
                    mark_price: Decimal::ZERO, // 需要从价格服务获取
                    unrealized_pnl: pos.total_unrealized_pnl(Decimal::ZERO),
                    leverage: 10,
                }
            }).collect());
        }

        Ok(Vec::new())
    }
}

impl Default for FuturesPosition {
    fn default() -> Self {
        Self::new()
    }
}

/// USDT 合约持仓数据
#[derive(Debug, Clone)]
pub struct FuturesPositionData {
    pub symbol: String,
    pub side: String,
    pub qty: Decimal,
    pub entry_price: Decimal,
    pub mark_price: Decimal,
    pub unrealized_pnl: Decimal,
    pub leverage: i32,
}
