#![forbid(unsafe_code)]

//! Futures 账户/持仓 API 功能测试


#[test]
fn test_futures_account_creation() {
    let account = FuturesAccount::new();
    assert!(std::mem::size_of_val(&account) > 0);
}

#[test]
fn test_futures_position_creation() {
    let position = FuturesPosition::new();
    assert!(std::mem::size_of_val(&position) > 0);
}

#[test]
fn test_futures_account_data_default() {
    let json = r#"{
        "totalMarginBalance": "",
        "totalUnrealizedProfit": "",
        "availableBalance": "",
        "totalMaintMargin": "",
        "maxWithdrawAmount": "",
        "updateTime": 0,
        "assets": []
    }"#;

    let resp: a_common::api::FuturesAccountResponse = serde_json::from_str(json).unwrap();
    assert_eq!(resp.total_margin_balance, "");
    assert_eq!(resp.update_time, 0);
}

#[test]
fn test_futures_account_data_parsing() {
    let json = r#"{
        "totalMarginBalance": "10000.00",
        "totalUnrealizedProfit": "500.00",
        "availableBalance": "8000.00",
        "totalMaintMargin": "100.00",
        "maxWithdrawAmount": "8000.00",
        "updateTime": 1234567890,
        "assets": [
            {
                "asset": "USDT",
                "marginBalance": "10000.00",
                "unrealizedProfit": "500.00",
                "availableBalance": "8000.00"
            }
        ]
    }"#;

    let resp: a_common::api::FuturesAccountResponse = serde_json::from_str(json).unwrap();
    assert_eq!(resp.total_margin_balance, "10000.00");
    assert_eq!(resp.total_unrealized_profit, "500.00");
    assert_eq!(resp.available_balance, "8000.00");
    assert_eq!(resp.assets.len(), 1);
    assert_eq!(resp.assets[0].asset, "USDT");
}

#[test]
fn test_futures_position_data_parsing() {
    let json = r#"{
        "symbol": "BTCUSDT",
        "positionSide": "LONG",
        "positionAmt": "1.5",
        "entryPrice": "50000.00",
        "markPrice": "51000.00",
        "unrealizedProfit": "1500.00",
        "leverage": "10",
        "marginRatio": "0.02"
    }"#;

    let resp: a_common::api::FuturesPositionResponse = serde_json::from_str(json).unwrap();
    assert_eq!(resp.symbol, "BTCUSDT");
    assert_eq!(resp.position_side, "LONG");
    assert_eq!(resp.position_amt, "1.5");
    assert_eq!(resp.leverage, "10");
}

#[test]
fn test_futures_position_data_short_side() {
    let json = r#"{
        "symbol": "ETHUSDT",
        "positionSide": "SHORT",
        "positionAmt": "10.0",
        "entryPrice": "2000.00",
        "markPrice": "1900.00",
        "unrealizedProfit": "1000.00",
        "leverage": "5",
        "marginRatio": "0.01"
    }"#;

    let resp: a_common::api::FuturesPositionResponse = serde_json::from_str(json).unwrap();
    assert_eq!(resp.symbol, "ETHUSDT");
    assert_eq!(resp.position_side, "SHORT");
    assert_eq!(resp.leverage, "5");
}

#[test]
fn test_futures_position_data_both_side() {
    let json = r#"{
        "symbol": "BNBUSDT",
        "positionSide": "BOTH",
        "positionAmt": "100.0",
        "entryPrice": "300.00",
        "markPrice": "310.00",
        "unrealizedProfit": "1000.00",
        "leverage": "3",
        "marginRatio": "0.01"
    }"#;

    let resp: a_common::api::FuturesPositionResponse = serde_json::from_str(json).unwrap();
    assert_eq!(resp.position_side, "BOTH");
}

#[test]
fn test_futures_account_data_multiple_assets() {
    let json = r#"{
        "totalMarginBalance": "50000.00",
        "totalUnrealizedProfit": "2000.00",
        "availableBalance": "40000.00",
        "totalMaintMargin": "500.00",
        "maxWithdrawAmount": "40000.00",
        "updateTime": 1234567890,
        "assets": [
            {"asset": "USDT", "marginBalance": "30000.00", "unrealizedProfit": "1000.00", "availableBalance": "25000.00"},
            {"asset": "BNB", "marginBalance": "20000.00", "unrealizedProfit": "1000.00", "availableBalance": "15000.00"}
        ]
    }"#;

    let resp: a_common::api::FuturesAccountResponse = serde_json::from_str(json).unwrap();
    assert_eq!(resp.assets.len(), 2);
    assert_eq!(resp.assets[0].asset, "USDT");
    assert_eq!(resp.assets[1].asset, "BNB");
}
