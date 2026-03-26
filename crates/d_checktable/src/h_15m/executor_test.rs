#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;

    fn executor() -> Executor {
        Executor::new(ExecutorConfig {
            symbol: "BTCUSDT".to_string(),
            order_interval_ms: 100,
            initial_ratio: dec!(0.05),
            lot_size: dec!(0.001),
            max_position: dec!(0.15),
        })
    }

    #[test]
    fn test_rate_limit_first_call() {
        let ex = executor();
        // 首次调用应通过
        assert!(ex.rate_limit_check(100));
    }

    #[test]
    fn test_rate_limit_within_window() {
        let ex = executor();
        let _ = ex.rate_limit_check(100);
        // 立即调用应被限制
        assert!(!ex.rate_limit_check(100));
    }

    #[test]
    fn test_calculate_initial_open() {
        let ex = executor();
        let qty = ex.calculate_order_qty(OrderType::InitialOpen, Decimal::ZERO, None);
        assert_eq!(qty, dec!(0.05));
    }

    #[test]
    fn test_calculate_double_add() {
        let ex = executor();
        let qty = ex.calculate_order_qty(
            OrderType::DoubleAdd,
            dec!(0.1),
            Some(PositionSide::Long),
        );
        assert_eq!(qty, dec!(0.05));
    }

    #[test]
    fn test_lot_size_rounding() {
        let ex = executor();
        // 原始数量: 0.123456, 步长: 0.001
        // 向下取整: floor(0.123456 / 0.001) * 0.001 = 123 * 0.001 = 0.123
        let qty = ex.calculate_order_qty(
            OrderType::InitialOpen,
            dec!(0.123456),
            None,
        );
        assert_eq!(qty, dec!(0.123));
    }

    #[test]
    fn test_send_order_success() {
        let ex = executor();
        let result = ex.send_order(OrderType::InitialOpen, Decimal::ZERO, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_send_order_rate_limited() {
        let ex = executor();
        ex.send_order(OrderType::InitialOpen, Decimal::ZERO, None).unwrap();
        // 第二次应被频率限制
        let result = ex.send_order(OrderType::InitialOpen, Decimal::ZERO, None);
        assert!(matches!(result, Err(ExecutorError::RateLimited)));
    }
}
