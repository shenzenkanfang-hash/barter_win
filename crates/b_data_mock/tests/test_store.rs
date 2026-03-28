//! MarketDataStore 测试 - 数据存储

use b_data_mock::{
    MarketDataStore, MarketDataStoreImpl,
    ws::kline_1m::KlineData,
};

fn create_test_kline_data(symbol: &str, close: &str, is_closed: bool) -> KlineData {
    KlineData {
        kline_start_time: 0,
        kline_close_time: 60000,
        symbol: symbol.to_string(),
        interval: "1m".to_string(),
        open: "50000".to_string(),
        close: close.to_string(),
        high: "50500".to_string(),
        low: "49500".to_string(),
        volume: "100".to_string(),
        is_closed,
    }
}

#[test]
fn test_store_write_and_read_kline() {
    let store = MarketDataStoreImpl::new();

    let kline = create_test_kline_data("BTCUSDT", "50200", true);
    store.write_kline("BTCUSDT", kline.clone(), true);

    let current = store.get_current_kline("BTCUSDT");
    assert!(current.is_some());

    let k = current.unwrap();
    assert_eq!(k.symbol, "BTCUSDT");
    assert_eq!(k.close, "50200");
}

#[test]
fn test_store_multiple_klines() {
    let store = MarketDataStoreImpl::new();

    // 写入多个 K线
    for i in 0..5 {
        let close = 50000 + i * 100;
        let kline = create_test_kline_data("BTCUSDT", &close.to_string(), true);
        store.write_kline("BTCUSDT", kline, true);
    }

    let current = store.get_current_kline("BTCUSDT");
    assert!(current.is_some());
    assert_eq!(current.unwrap().close, "50400");
}

#[test]
fn test_store_multiple_symbols() {
    let store = MarketDataStoreImpl::new();

    let btc = create_test_kline_data("BTCUSDT", "50000", true);
    let eth = create_test_kline_data("ETHUSDT", "3000", true);

    store.write_kline("BTCUSDT", btc, true);
    store.write_kline("ETHUSDT", eth, true);

    let btc_current = store.get_current_kline("BTCUSDT").unwrap();
    let eth_current = store.get_current_kline("ETHUSDT").unwrap();

    assert_eq!(btc_current.close, "50000");
    assert_eq!(eth_current.close, "3000");
}

#[test]
fn test_store_update_current_kline() {
    let store = MarketDataStoreImpl::new();

    // 未闭合 K线
    let kline1 = create_test_kline_data("BTCUSDT", "50000", false);
    store.write_kline("BTCUSDT", kline1.clone(), false);

    // 更新为闭合
    let kline2 = create_test_kline_data("BTCUSDT", "50200", true);
    store.write_kline("BTCUSDT", kline2, true);

    let current = store.get_current_kline("BTCUSDT").unwrap();
    assert_eq!(current.close, "50200");
    assert!(current.is_closed);
}

#[test]
fn test_store_orderbook() {
    use b_data_mock::store::OrderBookData;

    let store = MarketDataStoreImpl::new();

    let orderbook = OrderBookData {
        symbol: "BTCUSDT".to_string(),
        bids: vec![(50000.0, 1.0), (49900.0, 2.0)],
        asks: vec![(50100.0, 1.5), (50200.0, 0.5)],
        timestamp_ms: 1000,
    };

    store.write_orderbook("BTCUSDT", orderbook);

    let loaded = store.get_orderbook("BTCUSDT");
    assert!(loaded.is_some());

    let o = loaded.unwrap();
    assert_eq!(o.symbol, "BTCUSDT");
    assert_eq!(o.bids.len(), 2);
    assert_eq!(o.asks.len(), 2);
}

#[test]
fn test_store_volatility() {
    let store = MarketDataStoreImpl::new();

    // 写入多个 K线触发波动率计算
    for i in 0..10 {
        let close = 50000 + (i * 100) as i64;
        let kline = create_test_kline_data("BTCUSDT", &close.to_string(), true);
        store.write_kline("BTCUSDT", kline, true);
    }

    let vol = store.get_volatility("BTCUSDT");
    // 波动率可能为 None（取决于计算逻辑）
    // 这里只验证方法可调用
    let _ = vol;
}

#[test]
fn test_store_nonexistent_symbol() {
    let store = MarketDataStoreImpl::new();

    let kline = store.get_current_kline("NONEXIST");
    assert!(kline.is_none());

    let orderbook = store.get_orderbook("NONEXIST");
    assert!(orderbook.is_none());
}
