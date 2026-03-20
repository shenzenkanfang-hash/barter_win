//! 订单簿模块 - 20档深度 + 深度指标计算

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

    /// 更新订单簿 (增量更新)
    pub fn update(&mut self, last_update_id: u64, bids: Vec<(Decimal, Decimal)>, asks: Vec<(Decimal, Decimal)>) {
        // 确保 update_id 递增
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

    /// 订单簿总档数
    pub fn depth(&self) -> (usize, usize) {
        (self.bids.len(), self.asks.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_depth_indicator() {
        let mut ob = OrderBook::new("BTCUSDT".to_string());
        ob.update(1,
            vec![(dec!(100), dec!(10)), (dec!(99), dec!(20))],
            vec![(dec!(101), dec!(15)), (dec!(102), dec!(25))],
        );

        let indicator = ob.depth_indicator();
        // bid_depth = 10 + 20 = 30
        // ask_depth = 15 + 25 = 40
        // ratio = 30 / 40 = 0.75
        assert!(indicator < dec!(0.8) && indicator > dec!(0.7));
    }
}