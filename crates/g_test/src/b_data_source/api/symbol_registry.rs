#![forbid(unsafe_code)]

//! 品种注册中心功能测试


#[test]
fn test_symbol_registry_new_mock() {
    let registry = SymbolRegistry::new_mock();
    // Mock mode should work without Redis
    assert!(std::mem::size_of_val(&registry) > 0);
}

#[test]
fn test_symbol_registry_needs_update_after_creation() {
    let registry = SymbolRegistry::new_mock();
    // Initially should NOT need update (just set to now)
    assert!(!registry.needs_update());
}

#[tokio::test]
async fn test_symbol_registry_get_trading_symbols_empty() {
    let registry = SymbolRegistry::new_mock();
    let symbols: FnvHashSet<String> = registry.get_trading_symbols().await;
    assert!(symbols.is_empty());
}

#[tokio::test]
async fn test_symbol_registry_update_mock_skips() {
    let mut registry = SymbolRegistry::new_mock();
    // In mock mode, update should skip API call
    let result: Result<(), b_data_source::MarketError> = registry.update_symbols().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_symbol_registry_get_symbol_info_mock() {
    let mut registry = SymbolRegistry::new_mock();
    // Mock mode should return None for symbol info
    let info: Option<String> = registry.get_symbol_info("BTCUSDT").await;
    assert!(info.is_none());
}

#[test]
fn test_symbol_registry_default_values() {
    let registry = SymbolRegistry::new_mock();
    // Check internal state is initialized (no update needed immediately)
    assert!(!registry.needs_update());
}
