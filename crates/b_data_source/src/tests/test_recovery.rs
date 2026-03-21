//! 数据恢复功能测试

use crate::recovery::CheckpointData;

#[test]
fn test_checkpoint_data_creation() {
    let data = CheckpointData {
        symbol: "BTCUSDT".to_string(),
        timestamp: 1710000000,
        channel_type: "High".to_string(),
        is_in_high_vol_window: true,
        high_vol_window_start: Some(1709999900),
        indicator_snapshot: r#"{"ema_fast": 50000}"#.to_string(),
    };

    assert_eq!(data.symbol, "BTCUSDT");
    assert_eq!(data.timestamp, 1710000000);
    assert_eq!(data.channel_type, "High");
    assert!(data.is_in_high_vol_window);
    assert!(data.high_vol_window_start.is_some());
}

#[test]
fn test_checkpoint_data_serialization() {
    let data = CheckpointData {
        symbol: "ETHUSDT".to_string(),
        timestamp: 1710000000,
        channel_type: "Low".to_string(),
        is_in_high_vol_window: false,
        high_vol_window_start: None,
        indicator_snapshot: r#"{"ema_fast": 2000, "rsi": 65}"#.to_string(),
    };

    let json = serde_json::to_string(&data).unwrap();
    assert!(json.contains("ETHUSDT"));
    assert!(json.contains("Low"));
    assert!(json.contains("1710000000"));

    let restored: CheckpointData = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.symbol, "ETHUSDT");
    assert_eq!(restored.channel_type, "Low");
    assert!(!restored.is_in_high_vol_window);
    assert!(restored.high_vol_window_start.is_none());
}

#[test]
fn test_checkpoint_data_high_vol_window() {
    let data = CheckpointData {
        symbol: "BTCUSDT".to_string(),
        timestamp: 1710000000,
        channel_type: "High".to_string(),
        is_in_high_vol_window: true,
        high_vol_window_start: Some(1709999900),
        indicator_snapshot: r#"{"ema_fast": 50000}"#.to_string(),
    };

    assert!(data.is_in_high_vol_window);
    assert_eq!(data.high_vol_window_start.unwrap(), 1709999900);
}

#[test]
fn test_checkpoint_data_no_high_vol_window() {
    let data = CheckpointData {
        symbol: "BTCUSDT".to_string(),
        timestamp: 1710000000,
        channel_type: "Low".to_string(),
        is_in_high_vol_window: false,
        high_vol_window_start: None,
        indicator_snapshot: r#"{"ema_fast": 50000}"#.to_string(),
    };

    assert!(!data.is_in_high_vol_window);
    assert!(data.high_vol_window_start.is_none());
}

#[test]
fn test_checkpoint_data_complex_snapshot() {
    let data = CheckpointData {
        symbol: "BTCUSDT".to_string(),
        timestamp: 1710000000,
        channel_type: "High".to_string(),
        is_in_high_vol_window: true,
        high_vol_window_start: Some(1709999900),
        indicator_snapshot: r#"
        {
            "ema_fast": 50000,
            "ema_slow": 49500,
            "rsi": 70,
            "macd": 150.5,
            "atr": 200.25
        }
        "#.to_string(),
    };

    let json = serde_json::to_string(&data).unwrap();
    let restored: CheckpointData = serde_json::from_str(&json).unwrap();

    assert!(restored.indicator_snapshot.contains("ema_fast"));
    assert!(restored.indicator_snapshot.contains("rsi"));
    assert!(restored.indicator_snapshot.contains("macd"));
}

#[test]
fn test_checkpoint_data_empty_snapshot() {
    let data = CheckpointData {
        symbol: "BTCUSDT".to_string(),
        timestamp: 1710000000,
        channel_type: "Low".to_string(),
        is_in_high_vol_window: false,
        high_vol_window_start: None,
        indicator_snapshot: String::new(),
    };

    assert!(data.indicator_snapshot.is_empty());
}

#[test]
fn test_checkpoint_data_clone() {
    let data = CheckpointData {
        symbol: "BTCUSDT".to_string(),
        timestamp: 1710000000,
        channel_type: "High".to_string(),
        is_in_high_vol_window: true,
        high_vol_window_start: Some(1709999900),
        indicator_snapshot: r#"{"ema_fast": 50000}"#.to_string(),
    };

    let cloned = data.clone();
    assert_eq!(cloned.symbol, data.symbol);
    assert_eq!(cloned.timestamp, data.timestamp);
    assert_eq!(cloned.channel_type, data.channel_type);
    assert_eq!(cloned.is_in_high_vol_window, data.is_in_high_vol_window);
}

#[test]
fn test_checkpoint_data_symbols() {
    let symbols = vec!["BTCUSDT", "ETHUSDT", "SOLUSDT"];
    for symbol in symbols {
        let data = CheckpointData {
            symbol: symbol.to_string(),
            timestamp: 1710000000,
            channel_type: "High".to_string(),
            is_in_high_vol_window: true,
            high_vol_window_start: Some(1709999900),
            indicator_snapshot: r#"{"ema_fast": 50000}"#.to_string(),
        };
        assert_eq!(data.symbol, symbol);
    }
}

#[test]
fn test_checkpoint_data_all_channel_types() {
    let channel_types = vec!["High", "Low", "Medium", "Fast", "Slow"];
    for channel_type in channel_types {
        let data = CheckpointData {
            symbol: "BTCUSDT".to_string(),
            timestamp: 1710000000,
            channel_type: channel_type.to_string(),
            is_in_high_vol_window: false,
            high_vol_window_start: None,
            indicator_snapshot: r#"{"ema_fast": 50000}"#.to_string(),
        };
        assert_eq!(data.channel_type, channel_type);
    }
}
