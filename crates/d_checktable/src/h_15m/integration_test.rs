#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;
    use tokio::time::sleep as tokio_sleep;

    use crate::h_15m::executor::{Executor, ExecutorConfig, OrderType};
    use crate::h_15m::repository::{Repository, TradeRecord};
    use crate::h_15m::trader::{Trader, TraderConfig};
    use x_data::position::PositionSide;

    #[tokio::test]
    async fn test_trader_lifecycle() {
        // 1. 创建 Repository
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let repo = Arc::new(
            Repository::new("BTCUSDT", db_path.to_str().unwrap()).unwrap()
        );

        // 2. 创建 Executor
        let executor = Arc::new(Executor::new(ExecutorConfig {
            symbol: "BTCUSDT".to_string(),
            order_interval_ms: 100,
            initial_ratio: dec!(0.05),
            lot_size: dec!(0.001),
            max_position: dec!(0.15),
        }));

        // 3. 创建 Trader
        let config = TraderConfig {
            symbol: "BTCUSDT".to_string(),
            interval_ms: 50,
            db_path: db_path.to_str().unwrap().to_string(),
            order_interval_ms: 100,
            lot_size: dec!(0.001),
            initial_ratio: dec!(0.05),
            max_position: dec!(0.15),
        };
        let trader = Arc::new(Trader::new(config, executor.clone(), repo.clone()));

        // 4. 启动（短暂运行）
        let trader_clone = trader.clone();
        let handle = tokio::spawn(async move {
            trader_clone.start().await;
        });

        tokio_sleep(Duration::from_millis(200)).await;

        // 5. 停止
        trader.stop();
        let _ = handle.await;

        // 6. 验证健康状态
        let health = trader.health().await;
        assert_eq!(health.symbol, "BTCUSDT");
        assert!(!health.is_running);
    }

    #[tokio::test]
    async fn test_rate_limit_atomic() {
        let executor = Arc::new(Executor::new(ExecutorConfig {
            symbol: "BTCUSDT".to_string(),
            order_interval_ms: 50,
            ..Default::default()
        }));

        // 并发下单测试
        let handles: Vec<_> = (0..10)
            .map(|_| {
                let ex = executor.clone();
                tokio::spawn(async move {
                    ex.send_order(OrderType::InitialOpen, Decimal::ZERO, None)
                })
            })
            .collect();

        let results: Vec<_> = futures::future::join_all(handles).await;
        let success_count = results.iter().filter(|r| r.as_ref().unwrap().is_ok()).count();

        // 应该只有 1 个成功（原子 CAS 保证）
        assert_eq!(success_count, 1);
    }
}
