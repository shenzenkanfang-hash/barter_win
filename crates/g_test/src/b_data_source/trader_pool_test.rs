//! TraderPool 黑盒测试
//!
//! 测试交易品种池的完整功能

#![forbid(unsafe_code)]


#[test]
fn test_trader_pool_register_and_unregister() {
    let pool = TraderPool::new();

    // 注册品种
    pool.register(SymbolMeta::new("BTCUSDT".to_string()));
    assert!(pool.is_trading("BTCUSDT"));

    // 注销品种
    pool.unregister("BTCUSDT");
    assert!(!pool.is_trading("BTCUSDT"));
}

#[test]
fn test_trader_pool_register_multiple() {
    let pool = TraderPool::new();

    pool.register(SymbolMeta::new("BTCUSDT".to_string()));
    pool.register(SymbolMeta::new("ETHUSDT".to_string()));
    pool.register(SymbolMeta::new("BNBUSDT".to_string()));

    assert_eq!(pool.count(), 3);
    assert!(pool.is_trading("BTCUSDT"));
    assert!(pool.is_trading("ETHUSDT"));
    assert!(pool.is_trading("BNBUSDT"));
}

#[test]
fn test_trader_pool_duplicate_register() {
    let pool = TraderPool::new();

    // 注册两次相同的品种
    pool.register(SymbolMeta::new("BTCUSDT".to_string()).with_status(TradingStatus::Active));
    pool.register(SymbolMeta::new("BTCUSDT".to_string()).with_status(TradingStatus::Pending));

    assert_eq!(pool.count(), 1, "重复注册不应增加数量");
}

#[test]
fn test_trader_pool_unregister_nonexistent() {
    let pool = TraderPool::new();

    // 注销不存在的品种不应出错
    pool.unregister("NONEXIST");
    assert_eq!(pool.count(), 0);
}

#[test]
fn test_trader_pool_get_meta() {
    let pool = TraderPool::new();

    pool.register(SymbolMeta::new("BTCUSDT".to_string()).with_status(TradingStatus::Active));

    let meta = pool.get_meta("BTCUSDT");
    assert!(meta.is_some());

    let meta = meta.unwrap();
    // TraderPool 统一存储小写 symbol
    assert_eq!(meta.symbol, "btcusdt");
    assert_eq!(meta.status, TradingStatus::Active);
}

#[test]
fn test_trader_pool_get_nonexistent_meta() {
    let pool = TraderPool::new();

    let meta = pool.get_meta("NONEXIST");
    assert!(meta.is_none());
}

#[test]
fn test_trader_pool_update_status() {
    let pool = TraderPool::new();

    pool.register(SymbolMeta::new("BTCUSDT".to_string()).with_status(TradingStatus::Pending));
    
    assert!(!pool.is_active("BTCUSDT"));

    pool.update_status("BTCUSDT", TradingStatus::Active);
    
    assert!(pool.is_active("BTCUSDT"));
    assert_eq!(pool.get_status("BTCUSDT"), Some(TradingStatus::Active));
}

#[test]
fn test_trader_pool_is_active() {
    let pool = TraderPool::new();

    // Pending 状态不算 Active
    pool.register(SymbolMeta::new("BTCUSDT".to_string()).with_status(TradingStatus::Pending));
    assert!(!pool.is_active("BTCUSDT"));

    // Active 状态才算 Active
    pool.update_status("BTCUSDT", TradingStatus::Active);
    assert!(pool.is_active("BTCUSDT"));

    // Paused 状态不算 Active
    pool.update_status("BTCUSDT", TradingStatus::Paused);
    assert!(!pool.is_active("BTCUSDT"));
}

#[test]
fn test_trader_pool_get_trading_symbols() {
    let pool = TraderPool::new();

    pool.register(SymbolMeta::new("BTCUSDT".to_string()).with_status(TradingStatus::Active));
    pool.register(SymbolMeta::new("ETHUSDT".to_string()).with_status(TradingStatus::Pending));

    let symbols = pool.get_trading_symbols();
    assert_eq!(symbols.len(), 2);
    // TraderPool 统一返回小写 symbol
    assert!(symbols.contains(&"btcusdt".to_string()));
    assert!(symbols.contains(&"ethusdt".to_string()));
}

#[test]
fn test_trader_pool_get_by_status() {
    let pool = TraderPool::new();

    pool.register(SymbolMeta::new("BTCUSDT".to_string()).with_status(TradingStatus::Active));
    pool.register(SymbolMeta::new("ETHUSDT".to_string()).with_status(TradingStatus::Active));
    pool.register(SymbolMeta::new("BNBUSDT".to_string()).with_status(TradingStatus::Paused));

    let active_symbols = pool.get_by_status(TradingStatus::Active);
    assert_eq!(active_symbols.len(), 2);
    // TraderPool 统一返回小写 symbol
    assert!(active_symbols.contains(&"btcusdt".to_string()));
    assert!(active_symbols.contains(&"ethusdt".to_string()));

    let paused_symbols = pool.get_by_status(TradingStatus::Paused);
    assert_eq!(paused_symbols.len(), 1);
    assert_eq!(paused_symbols[0], "bnbusdt");
}

#[test]
fn test_trader_pool_get_all_meta() {
    let pool = TraderPool::new();

    pool.register(SymbolMeta::new("BTCUSDT".to_string()).with_status(TradingStatus::Active).with_priority(80));
    pool.register(SymbolMeta::new("ETHUSDT".to_string()).with_status(TradingStatus::Active).with_priority(50));

    let all_meta = pool.get_all_meta();
    assert_eq!(all_meta.len(), 2);

    // TraderPool 统一返回小写 symbol
    let btc_meta = all_meta.iter().find(|m| m.symbol == "btcusdt").unwrap();
    assert_eq!(btc_meta.priority, 80);
}

#[test]
fn test_trader_pool_register_batch() {
    let pool = TraderPool::new();

    let symbols = vec![
        SymbolMeta::new("BTCUSDT".to_string()).with_status(TradingStatus::Active),
        SymbolMeta::new("ETHUSDT".to_string()).with_status(TradingStatus::Active),
        SymbolMeta::new("BNBUSDT".to_string()).with_status(TradingStatus::Active),
    ];

    pool.register_batch(symbols);

    assert_eq!(pool.count(), 3);
}

#[test]
fn test_trader_pool_clear() {
    let pool = TraderPool::new();

    pool.register(SymbolMeta::new("BTCUSDT".to_string()).with_status(TradingStatus::Active));
    pool.register(SymbolMeta::new("ETHUSDT".to_string()).with_status(TradingStatus::Active));

    assert_eq!(pool.count(), 2);

    pool.clear();

    assert_eq!(pool.count(), 0);
    assert!(pool.get_trading_symbols().is_empty());
}

#[test]
fn test_trader_pool_pause_all() {
    let pool = TraderPool::new();

    pool.register(SymbolMeta::new("BTCUSDT".to_string()).with_status(TradingStatus::Active));
    pool.register(SymbolMeta::new("ETHUSDT".to_string()).with_status(TradingStatus::Pending));

    pool.pause_all();

    assert!(!pool.is_active("BTCUSDT")); // Active -> Paused
    assert!(!pool.is_active("ETHUSDT")); // Pending 不变
}

#[test]
fn test_trader_pool_activate_all() {
    let pool = TraderPool::new();

    pool.register(SymbolMeta::new("BTCUSDT".to_string()).with_status(TradingStatus::Pending));
    pool.register(SymbolMeta::new("ETHUSDT".to_string()).with_status(TradingStatus::Paused));

    pool.activate_all();

    assert!(pool.is_active("BTCUSDT"));
    assert!(pool.is_active("ETHUSDT"));
}

#[test]
fn test_symbol_meta_builder() {
    let meta = SymbolMeta::new("BTCUSDT".to_string())
        .with_status(TradingStatus::Active)
        .with_priority(90);

    // SymbolMeta 统一存储小写 symbol
    assert_eq!(meta.symbol, "btcusdt");
    assert_eq!(meta.status, TradingStatus::Active);
    assert_eq!(meta.priority, 90);
    assert_eq!(meta.min_qty, 0.001);
    assert_eq!(meta.price_precision, 2);
    assert_eq!(meta.qty_precision, 3);
}

#[test]
fn test_symbol_meta_default() {
    let meta = SymbolMeta::default();
    assert_eq!(meta.symbol, "");
    assert_eq!(meta.status, TradingStatus::Pending);
    assert_eq!(meta.priority, 50);
}

#[test]
fn test_trading_status_default() {
    let status = TradingStatus::default();
    assert_eq!(status, TradingStatus::Pending);
}
