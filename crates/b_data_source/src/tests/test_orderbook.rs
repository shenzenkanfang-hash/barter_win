//! 订单簿功能测试

use crate::order_books::OrderBook;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

#[test]
fn test_orderbook_new() {
    let ob = OrderBook::new("BTCUSDT".to_string());
    assert_eq!(ob.symbol(), "BTCUSDT");
    assert_eq!(ob.last_update_id(), 0);
    assert!(ob.best_bid().is_none());
    assert!(ob.best_ask().is_none());
}

#[test]
fn test_orderbook_update() {
    let mut ob = OrderBook::new("BTCUSDT".to_string());
    ob.update(1,
        vec![(dec!(100), dec!(10)), (dec!(99), dec!(20))],
        vec![(dec!(101), dec!(15)), (dec!(102), dec!(25))],
    );

    assert_eq!(ob.last_update_id(), 1);
    assert_eq!(ob.best_bid(), Some(dec!(100)));
    assert_eq!(ob.best_ask(), Some(dec!(101)));
    assert_eq!(ob.depth(), (2, 2));
}

#[test]
fn test_orderbook_update_rejects_old_id() {
    let mut ob = OrderBook::new("BTCUSDT".to_string());
    ob.update(10,
        vec![(dec!(100), dec!(10))],
        vec![(dec!(101), dec!(15))],
    );

    // Try to update with older id - should be rejected
    ob.update(5,
        vec![(dec!(200), dec!(100))],
        vec![(dec!(201), dec!(100))],
    );

    assert_eq!(ob.last_update_id(), 10);
    assert_eq!(ob.best_bid(), Some(dec!(100)));
    assert_eq!(ob.best_ask(), Some(dec!(101)));
}

#[test]
fn test_orderbook_update_accepts_new_id() {
    let mut ob = OrderBook::new("BTCUSDT".to_string());
    ob.update(1,
        vec![(dec!(100), dec!(10))],
        vec![(dec!(101), dec!(15))],
    );

    ob.update(2,
        vec![(dec!(200), dec!(100))],
        vec![(dec!(201), dec!(100))],
    );

    assert_eq!(ob.last_update_id(), 2);
    assert_eq!(ob.best_bid(), Some(dec!(200)));
    assert_eq!(ob.best_ask(), Some(dec!(201)));
}

#[test]
fn test_orderbook_depth_indicator_equal() {
    let mut ob = OrderBook::new("BTCUSDT".to_string());
    ob.update(1,
        vec![(dec!(100), dec!(10))],
        vec![(dec!(101), dec!(10))],
    );

    let indicator = ob.depth_indicator();
    assert_eq!(indicator, dec!(1));
}

#[test]
fn test_orderbook_depth_indicator_bid_heavy() {
    let mut ob = OrderBook::new("BTCUSDT".to_string());
    ob.update(1,
        vec![(dec!(100), dec!(100))],
        vec![(dec!(101), dec!(50))],
    );

    let indicator = ob.depth_indicator();
    // bid_depth=100, ask_depth=50, ratio=2
    assert_eq!(indicator, dec!(2));
}

#[test]
fn test_orderbook_depth_indicator_ask_heavy() {
    let mut ob = OrderBook::new("BTCUSDT".to_string());
    ob.update(1,
        vec![(dec!(100), dec!(50))],
        vec![(dec!(101), dec!(100))],
    );

    let indicator = ob.depth_indicator();
    // bid_depth=50, ask_depth=100, ratio=0.5
    assert_eq!(indicator, dec!(0.5));
}

#[test]
fn test_orderbook_depth_indicator_empty_asks() {
    let mut ob = OrderBook::new("BTCUSDT".to_string());
    ob.update(1,
        vec![(dec!(100), dec!(100))],
        vec![],
    );

    let indicator = ob.depth_indicator();
    // When ask_depth=0, returns 1 (neutral)
    assert_eq!(indicator, dec!(1));
}

#[test]
fn test_orderbook_best_bid_ask() {
    let mut ob = OrderBook::new("BTCUSDT".to_string());
    ob.update(1,
        vec![(dec!(100), dec!(10)), (dec!(99), dec!(20)), (dec!(98), dec!(30))],
        vec![(dec!(101), dec!(15)), (dec!(102), dec!(25)), (dec!(103), dec!(35))],
    );

    // Best bid = highest bid price
    assert_eq!(ob.best_bid(), Some(dec!(100)));
    // Best ask = lowest ask price
    assert_eq!(ob.best_ask(), Some(dec!(101)));
}

#[test]
fn test_orderbook_depth() {
    let mut ob = OrderBook::new("BTCUSDT".to_string());
    ob.update(1,
        vec![(dec!(100), dec!(10)), (dec!(99), dec!(20))],
        vec![(dec!(101), dec!(15))],
    );

    let (bid_depth, ask_depth) = ob.depth();
    assert_eq!(bid_depth, 2);
    assert_eq!(ask_depth, 1);
}

#[test]
fn test_orderbook_spread() {
    let mut ob = OrderBook::new("BTCUSDT".to_string());
    ob.update(1,
        vec![(dec!(100), dec!(10))],
        vec![(dec!(105), dec!(10))],
    );

    let best_bid = ob.best_bid().unwrap();
    let best_ask = ob.best_ask().unwrap();
    let spread = best_ask - best_bid;

    // Spread should be 5 (105 - 100)
    assert_eq!(spread, dec!(5));
}

#[test]
fn test_orderbook_multiple_updates_same_id() {
    let mut ob = OrderBook::new("BTCUSDT".to_string());
    // Same id twice
    ob.update(1,
        vec![(dec!(100), dec!(10))],
        vec![(dec!(101), dec!(10))],
    );
    ob.update(1,
        vec![(dec!(200), dec!(100))],
        vec![(dec!(201), dec!(100))],
    );

    // First update should be kept (id not greater)
    assert_eq!(ob.last_update_id(), 1);
    assert_eq!(ob.best_bid(), Some(dec!(100)));
}

#[test]
fn test_orderbook_large_depth() {
    let mut ob = OrderBook::new("BTCUSDT".to_string());
    let bids: Vec<_> = vec![
        (dec!(100), dec!(1)),
        (dec!(101), dec!(2)),
        (dec!(102), dec!(3)),
        (dec!(103), dec!(4)),
        (dec!(104), dec!(5)),
        (dec!(105), dec!(6)),
        (dec!(106), dec!(7)),
        (dec!(107), dec!(8)),
        (dec!(108), dec!(9)),
        (dec!(109), dec!(10)),
        (dec!(110), dec!(11)),
        (dec!(111), dec!(12)),
        (dec!(112), dec!(13)),
        (dec!(113), dec!(14)),
        (dec!(114), dec!(15)),
        (dec!(115), dec!(16)),
        (dec!(116), dec!(17)),
        (dec!(117), dec!(18)),
        (dec!(118), dec!(19)),
        (dec!(119), dec!(20)),
    ];
    let asks: Vec<_> = vec![
        (dec!(200), dec!(1)),
        (dec!(201), dec!(2)),
        (dec!(202), dec!(3)),
        (dec!(203), dec!(4)),
        (dec!(204), dec!(5)),
        (dec!(205), dec!(6)),
        (dec!(206), dec!(7)),
        (dec!(207), dec!(8)),
        (dec!(208), dec!(9)),
        (dec!(209), dec!(10)),
        (dec!(210), dec!(11)),
        (dec!(211), dec!(12)),
        (dec!(212), dec!(13)),
        (dec!(213), dec!(14)),
        (dec!(214), dec!(15)),
        (dec!(215), dec!(16)),
        (dec!(216), dec!(17)),
        (dec!(217), dec!(18)),
        (dec!(218), dec!(19)),
        (dec!(219), dec!(20)),
    ];

    ob.update(1, bids.clone(), asks.clone());

    let (bid_depth, ask_depth) = ob.depth();
    assert_eq!(bid_depth, 20);
    assert_eq!(ask_depth, 20);
}
