//! MockApiGateway 测试 - 模拟交易网关

use b_data_mock::{MockApiGateway, MockConfig};
use a_common::models::types::Side;
use rust_decimal_macros::dec;

#[test]
fn test_gateway_create() {
    let gateway = MockApiGateway::with_default_config(dec!(10000.0));

    assert_eq!(gateway.get_current_price("BTCUSDT"), dec!(0.0));
}

#[test]
fn test_gateway_update_price() {
    let gateway = MockApiGateway::with_default_config(dec!(10000.0));

    gateway.update_price("BTCUSDT", dec!(50000.0));

    assert_eq!(gateway.get_current_price("BTCUSDT"), dec!(50000.0));
}

#[test]
fn test_gateway_place_order_buy() {
    let config = MockConfig::default();
    let gateway = MockApiGateway::new(dec!(10000.0), config);

    gateway.update_price("BTCUSDT", dec!(50000.0));

    // place_order: symbol, side, qty, price(None=当前价)
    let result = gateway.place_order("BTCUSDT", Side::Buy, dec!(0.1), None).unwrap();

    assert_eq!(result.status, a_common::models::types::OrderStatus::Filled);
    assert_eq!(result.filled_qty, dec!(0.1));
    assert_eq!(result.filled_price, dec!(50000.0));
}

#[test]
fn test_gateway_place_order_sell() {
    let config = MockConfig::default();
    let gateway = MockApiGateway::new(dec!(10000.0), config);

    gateway.update_price("BTCUSDT", dec!(50000.0));

    // 先买入开多仓
    let _ = gateway.place_order("BTCUSDT", Side::Buy, dec!(0.1), None);
    // 卖出平仓
    let result = gateway.place_order("BTCUSDT", Side::Sell, dec!(0.1), None).unwrap();

    assert_eq!(result.status, a_common::models::types::OrderStatus::Filled);
}

#[test]
fn test_gateway_get_account() {
    let config = MockConfig::default();
    let gateway = MockApiGateway::new(dec!(10000.0), config);

    let account = gateway.get_account().unwrap();

    assert_eq!(account.available, dec!(10000.0));
}

#[test]
fn test_gateway_position_after_buy() {
    let config = MockConfig::default();
    let gateway = MockApiGateway::new(dec!(10000.0), config);

    gateway.update_price("BTCUSDT", dec!(50000.0));
    let _ = gateway.place_order("BTCUSDT", Side::Buy, dec!(0.1), None);

    let pos = gateway.get_position("BTCUSDT").unwrap();

    assert!(pos.is_some());
    assert_eq!(pos.unwrap().long_qty, dec!(0.1));
}

#[test]
fn test_gateway_clone() {
    let gateway = MockApiGateway::with_default_config(dec!(10000.0));

    let gateway2 = gateway.clone();

    gateway.update_price("BTCUSDT", dec!(50000.0));
    gateway2.update_price("BTCUSDT", dec!(51000.0));

    // MockApiGateway Clone = Arc级别浅克隆，共享底层 OrderEngine 状态
    // 两次 update_price 写入同一个 OrderEngine，后者覆盖前者
    // gateway 和 gateway2 的 get_current_price 都返回 51000（共享状态）
    assert_eq!(gateway.get_current_price("BTCUSDT"), dec!(51000.0));
    assert_eq!(gateway2.get_current_price("BTCUSDT"), dec!(51000.0));
}

#[test]
fn test_gateway_no_liquidation_initial() {
    let config = MockConfig::default();
    let gateway = MockApiGateway::new(dec!(10000.0), config);

    // 初始不应强平
    assert!(!gateway.check_liquidation());
}

#[test]
fn test_gateway_multiple_orders() {
    let config = MockConfig::default();
    let gateway = MockApiGateway::new(dec!(10000.0), config);

    gateway.update_price("BTCUSDT", dec!(50000.0));

    // 多笔买入
    let _ = gateway.place_order("BTCUSDT", Side::Buy, dec!(0.05), None);
    let _ = gateway.place_order("BTCUSDT", Side::Buy, dec!(0.05), None);

    let pos = gateway.get_position("BTCUSDT").unwrap();

    assert!(pos.is_some());
    assert_eq!(pos.unwrap().long_qty, dec!(0.1));
}
