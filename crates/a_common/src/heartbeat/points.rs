/// 测试点名称映射表
pub const TEST_POINT_NAMES: &[(&str, &str)] = &[
    // a_common
    ("AC-001", "BinanceApiGateway"),
    ("AC-004", "BinanceWsConnector"),
    ("AC-005", "BinanceWsConnector"),
    ("AC-006", "BinanceWsConnector"),

    // b_data_source
    ("BS-001", "Kline1mStream"),
    ("BS-003", "Kline1dStream"),
    ("BS-004", "DepthStream"),
    ("BS-007", "FuturesDataSyncer"),

    // c_data_process
    ("CP-001", "SignalProcessor"),
    ("CP-007", "StrategyStateManager"),

    // d_checktable
    ("DT-001", "CheckTable"),
    ("DT-002", "h_15m::Trader"),

    // e_risk_monitor
    ("ER-001", "RiskPreChecker"),
    ("ER-003", "OrderCheck"),

    // f_engine
    ("FE-001", "EventEngine"),
    ("FE-003", "EventBus"),
];

/// 根据测试点ID获取名称
pub fn get_point_name(point_id: &str) -> Option<&'static str> {
    TEST_POINT_NAMES.iter()
        .find(|(id, _)| *id == point_id)
        .map(|(_, name)| *name)
}
