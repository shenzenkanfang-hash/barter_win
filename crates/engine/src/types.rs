use account::types::{Order as AccountOrder, OrderStatus, Side as AccountSide};
use rust_decimal::Decimal;
use strategy::types::{OrderRequest, Side as StrategySide};

/// 将策略层的 Side 转换为账户层的 Side
impl From<StrategySide> for AccountSide {
    fn from(side: StrategySide) -> Self {
        match side {
            StrategySide::Long => AccountSide::Buy,
            StrategySide::Short => AccountSide::Sell,
        }
    }
}

/// 将策略层的 OrderRequest 转换为账户层的 Order
impl From<&OrderRequest> for AccountOrder {
    fn from(req: &OrderRequest) -> Self {
        AccountOrder {
            order_id: 0, // 由账户层生成
            symbol: req.symbol.clone(),
            side: req.side.into(),
            order_type: req.order_type.into(),
            price: req.price.unwrap_or(Decimal::ZERO),
            qty: req.qty,
            status: OrderStatus::Pending,
        }
    }
}

use strategy::types::OrderType as StrategyOrderType;
use account::types::OrderType as AccountOrderType;

impl From<StrategyOrderType> for AccountOrderType {
    fn from(order_type: StrategyOrderType) -> Self {
        match order_type {
            StrategyOrderType::Market => AccountOrderType::Market,
            StrategyOrderType::Limit => AccountOrderType::Limit,
        }
    }
}
