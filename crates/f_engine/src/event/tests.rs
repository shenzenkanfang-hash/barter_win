//!
//! f_engine Event 模块测试
//!

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;

    // 导入被测试的类型
    use crate::event::{EventBus, TickEvent, EngineConfig, EngineState};
    use crate::{TradingDecision, OrderRequest, Side, OrderType, TradingAction};
    use a_common::OrderStatus;

    // ============================================================================
    // FE-003: EventBus 事件总线测试
    // ============================================================================

    #[tokio::test]
    async fn test_event_bus_create() {
        let (mut bus, _handle) = EventBus::default();
        assert!(bus.tick_rx_mut().is_some());
        assert!(bus.kline_rx_mut().is_some());
        assert!(bus.order_rx_mut().is_some());
    }

    #[tokio::test]
    async fn test_event_bus_send_and_receive_tick() {
        let (bus, handle) = EventBus::default();
        let mut tick_rx = bus.into_tick_rx();

        let tick = TickEvent {
            symbol: "BTCUSDT".to_string(),
            price: dec!(50000),
            qty: dec!(1.0),
            timestamp: Utc::now(),
            kline: None,
            is_kline_closed: false,
        };

        // 通过 handle 发送
        handle.send_tick(tick.clone()).await.unwrap();

        // 接收并验证
        let received = tick_rx.recv().await.unwrap();
        assert_eq!(received.symbol, "BTCUSDT");
        assert_eq!(received.price, dec!(50000));
    }

    #[tokio::test]
    async fn test_event_bus_try_send_tick() {
        let (bus, handle) = EventBus::default();
        let _tick_rx = bus.into_tick_rx();

        let tick = TickEvent {
            symbol: "ETHUSDT".to_string(),
            price: dec!(3000),
            qty: dec!(0.1),
            timestamp: Utc::now(),
            kline: None,
            is_kline_closed: false,
        };

        assert!(handle.try_send_tick(tick).is_ok());
    }

    #[tokio::test]
    async fn test_event_bus_channel_remaining() {
        let (bus, handle) = EventBus::default();
        let _tick_rx = bus.into_tick_rx();

        let remaining = handle.tick_channel_remaining();
        assert!(remaining > 0);
        assert!(!handle.is_tick_channel_full());
    }

    // ============================================================================
    // FE-001: EventEngine 创建测试
    // ============================================================================

    #[test]
    fn test_engine_config_default() {
        let config = EngineConfig::default();
        assert_eq!(config.symbol, "BTCUSDT");
        assert_eq!(config.initial_fund, dec!(10000));
        assert_eq!(config.max_position, dec!(0.15));
        assert_eq!(config.initial_ratio, dec!(0.05));
        assert_eq!(config.lot_size, dec!(0.001));
        assert!(config.enable_risk_check);
        assert!(config.enable_strategy);
    }

    #[test]
    fn test_engine_state_default() {
        let state = EngineState::default();
        assert!(!state.has_position);
        assert_eq!(state.position_qty, Decimal::ZERO);
        assert_eq!(state.tick_count, 0);
        assert_eq!(state.total_orders, 0);
        assert_eq!(state.filled_orders, 0);
        assert_eq!(state.rejected_orders, 0);
    }

    // ============================================================================
    // FE-010: TradingDecision 测试
    // ============================================================================

    #[test]
    fn test_trading_decision_creation() {
        let decision = TradingDecision::new(
            TradingAction::Long,
            "Test signal",
            80,
            "BTCUSDT".to_string(),
            dec!(0.01),
            dec!(50000),
            Utc::now().timestamp(),
        );

        assert_eq!(decision.action, TradingAction::Long);
        assert_eq!(decision.symbol, "BTCUSDT");
        assert_eq!(decision.qty, dec!(0.01));
        assert!(decision.is_entry());
        assert!(!decision.is_exit());
    }

    #[test]
    fn test_trading_decision_exit() {
        let decision = TradingDecision::new(
            TradingAction::Flat,
            "Exit signal",
            100,
            "BTCUSDT".to_string(),
            dec!(0.01),
            dec!(51000),
            Utc::now().timestamp(),
        );

        assert!(decision.is_exit());
        assert!(!decision.is_entry());
    }

    #[test]
    fn test_trading_decision_short() {
        let decision = TradingDecision::new(
            TradingAction::Short,
            "Short signal",
            75,
            "ETHUSDT".to_string(),
            dec!(0.1),
            dec!(3000),
            Utc::now().timestamp(),
        );

        assert_eq!(decision.action, TradingAction::Short);
        assert!(decision.is_entry());
    }

    // ============================================================================
    // FE-011: OrderRequest 测试
    // ============================================================================

    #[test]
    fn test_order_request_market() {
        let order = OrderRequest::new_market(
            "BTCUSDT".to_string(),
            Side::Buy,
            dec!(0.01),
        );

        assert_eq!(order.symbol, "BTCUSDT");
        assert_eq!(order.side, Side::Buy);
        assert_eq!(order.order_type, OrderType::Market);
        assert!(order.price.is_none());
    }

    #[test]
    fn test_order_request_limit() {
        let order = OrderRequest::new_limit(
            "ETHUSDT".to_string(),
            Side::Sell,
            dec!(0.1),
            dec!(3000),
        );

        assert_eq!(order.symbol, "ETHUSDT");
        assert_eq!(order.side, Side::Sell);
        assert_eq!(order.order_type, OrderType::Limit);
        assert!(order.price.is_some());
        assert_eq!(order.price.unwrap(), dec!(3000));
    }

    #[test]
    fn test_order_request_fields() {
        let order = OrderRequest {
            symbol: "BTCUSDT".to_string(),
            side: Side::Buy,
            order_type: OrderType::Market,
            qty: dec!(0.05),
            price: None,
        };

        assert_eq!(order.symbol, "BTCUSDT");
        assert_eq!(order.qty, dec!(0.05));
    }

    // ============================================================================
    // FE-007: EventDrivenEngine 核心引擎测试
    // ============================================================================

    #[test]
    fn test_engine_config_custom() {
        let config = EngineConfig {
            symbol: "ETHUSDT".to_string(),
            initial_fund: dec!(50000),
            max_position: dec!(0.2),
            initial_ratio: dec!(0.1),
            lot_size: dec!(0.01),
            enable_risk_check: true,
            enable_strategy: true,
            log_timing: true,
        };

        assert_eq!(config.symbol, "ETHUSDT");
        assert_eq!(config.initial_fund, dec!(50000));
        assert!(config.enable_risk_check);
        assert!(config.enable_strategy);
    }

    #[test]
    fn test_engine_state_with_position() {
        let mut state = EngineState::default();
        state.has_position = true;
        state.position_qty = dec!(0.1);
        state.position_price = dec!(50000);
        state.position_side = Some(Side::Buy);

        assert!(state.has_position);
        assert_eq!(state.position_qty, dec!(0.1));
        assert_eq!(state.position_side, Some(Side::Buy));
    }

    // ============================================================================
    // 边界条件测试
    // ============================================================================

    #[test]
    fn test_trading_decision_zero_qty() {
        // 零数量应该被允许创建（风控层面检查）
        let decision = TradingDecision::new(
            TradingAction::Long,
            "Zero qty test",
            50,
            "BTCUSDT".to_string(),
            dec!(0),
            dec!(50000),
            Utc::now().timestamp(),
        );

        assert_eq!(decision.qty, dec!(0));
    }

    #[test]
    fn test_order_request_zero_price_limit() {
        // 限价单价格为 Some(0) 是允许的
        let order = OrderRequest::new_limit(
            "BTCUSDT".to_string(),
            Side::Buy,
            dec!(0.01),
            dec!(0),
        );

        assert!(order.price.is_some());
        assert_eq!(order.price.unwrap(), dec!(0));
    }

    #[test]
    fn test_trading_decision_high_confidence() {
        let decision = TradingDecision::new(
            TradingAction::Long,
            "High confidence signal",
            100,
            "BTCUSDT".to_string(),
            dec!(0.01),
            dec!(50000),
            Utc::now().timestamp(),
        );

        assert_eq!(decision.confidence, 100);
    }

    #[test]
    fn test_engine_state_tick_count_increment() {
        let mut state = EngineState::default();
        assert_eq!(state.tick_count, 0);

        state.tick_count += 1;
        state.tick_count += 1;
        assert_eq!(state.tick_count, 2);
    }

    #[test]
    fn test_order_side_values() {
        assert_eq!(Side::Buy, Side::Buy);
        assert_eq!(Side::Sell, Side::Sell);
        assert_ne!(Side::Buy, Side::Sell);
    }

    // ============================================================================
    // TradingAction 测试
    // ============================================================================

    #[test]
    fn test_trading_action_variants() {
        let actions = vec![
            TradingAction::Long,
            TradingAction::Short,
            TradingAction::Flat,
            TradingAction::Add,
            TradingAction::Reduce,
            TradingAction::Hedge,
            TradingAction::Wait,
        ];

        for action in actions {
            let decision = TradingDecision::new(
                action,
                "Test",
                50,
                "BTCUSDT".to_string(),
                dec!(0.01),
                dec!(50000),
                Utc::now().timestamp(),
            );
            assert_eq!(decision.action, action);
        }
    }
}
