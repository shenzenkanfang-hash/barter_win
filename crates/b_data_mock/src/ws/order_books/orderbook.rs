//! 订单簿模块 - 20档深度
//!
//! 复制自 b_data_source::ws::order_books::orderbook

use rust_decimal::Decimal;
use rust_decimal_macros::dec;

#[derive(Debug, Clone)]
pub struct OrderBook {
    symbol: String,
    bids: Vec<(Decimal, Decimal)>,  // (price, qty)
    asks: Vec<(Decimal, Decimal)>,
    last_update_id: u64,
}

impl OrderBook {
    pub fn new(symbol: String) -> Self {
        Self {
            symbol,
            bids: Vec::with_capacity(20),
            asks: Vec::with_capacity(20),
            last_update_id: 0,
        }
    }

    /// 深度指标: 买盘/卖盘 厚度比率
    pub fn depth_indicator(&self) -> Decimal {
        let bid_depth: Decimal = self.bids.iter().take(20).map(|(_, q)| q).sum();
        let ask_depth: Decimal = self.asks.iter().take(20).map(|(_, q)| q).sum();
        if ask_depth > dec!(0) {
            bid_depth / ask_depth
        } else {
            dec!(1)
        }
    }

    /// 更新订单簿
    pub fn update(&mut self, last_update_id: u64, bids: Vec<(Decimal, Decimal)>, asks: Vec<(Decimal, Decimal)>) {
        if last_update_id > self.last_update_id {
            self.bids = bids;
            self.asks = asks;
            self.last_update_id = last_update_id;
        }
    }

    pub fn last_update_id(&self) -> u64 {
        self.last_update_id
    }

    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    pub fn best_bid(&self) -> Option<Decimal> {
        self.bids.first().map(|(p, _)| *p)
    }

    pub fn best_ask(&self) -> Option<Decimal> {
        self.asks.first().map(|(p, _)| *p)
    }
}
