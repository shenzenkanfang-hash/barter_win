//! StrategyExecutor 黑盒测试
//!
//! 测试策略调度器的完整功能

#![forbid(unsafe_code)]

use parking_lot::RwLock;

use f_engine::strategy::{
    Strategy, StrategyKLine,
    StrategyState, TradingSignal,
};

/// 测试策略实现
#[allow(dead_code)]
struct TestStrategy {
    id: String,
    name: String,
    symbols: Vec<String>,
    enabled: RwLock<bool>,
    state: StrategyState,
    signals_to_return: RwLock<Vec<TradingSignal>>,
}

#[allow(dead_code)]
impl TestStrategy {
    fn new(id: &str, symbols: Vec<String>) -> Self {
        Self {
            id: id.to_string(),
            name: id.to_string(),
            symbols,
            enabled: RwLock::new(true),
            state: StrategyState::new(id.to_string()),
            signals_to_return: RwLock::new(Vec::new()),
        }
    }

    fn set_signals(&self, signals: Vec<TradingSignal>) {
        *self.signals_to_return.write() = signals;
    }

    fn set_enabled(&self, enabled: bool) {
        *self.enabled.write() = enabled;
    }
}

impl Strategy for TestStrategy {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn symbols(&self) -> Vec<String> {
        self.symbols.clone()
    }

    fn is_enabled(&self) -> bool {
        *self.enabled.read()
    }

    fn on_bar(&self, _bar: &StrategyKLine) -> Option<TradingSignal> {
        self.signals_to_return.read().first().cloned()
    }

    fn state(&self) -> &StrategyState {
        &self.state
    }
}

// ============================================================================
// StrategyExecutor 基本功能测试
// ============================================================================

#[test]
fn test_executor_register_and_count() {
    let executor = StrategyExecutor::new();

    assert_eq!(executor.count(), 0, "初始策略数量应为 0");

    let strategy = Arc::new(TestStrategy::new("test1", vec!["BTCUSDT".to_string()]));
    executor.register(strategy);

    assert_eq!(executor.count(), 1, "注册后策略数量应为 1");
    assert_eq!(executor.symbol_count(), 1, "品种数量应为 1");
}

#[test]
fn test_executor_register_multiple_symbols() {
    let executor = StrategyExecutor::new();

    let strategy = Arc::new(TestStrategy::new(
        "multi_symbol",
        vec!["BTCUSDT".to_string(), "ETHUSDT".to_string()],
    ));
    executor.register(strategy);

    assert_eq!(executor.count(), 1);
    assert_eq!(executor.symbol_count(), 2, "两个品种都应注册");
}

#[test]
fn test_executor_unregister() {
    let executor = StrategyExecutor::new();

    let strategy = Arc::new(TestStrategy::new("test1", vec!["BTCUSDT".to_string()]));
    executor.register(strategy.clone());

    assert_eq!(executor.count(), 1);

    executor.unregister("test1");

    assert_eq!(executor.count(), 0, "注销后策略数量应为 0");
}

#[test]
fn test_executor_dispatch_to_single_strategy() {
    let executor = StrategyExecutor::new();

    let signal = TradingSignal::new(
        "BTCUSDT".to_string(),
        Direction::Long,
        dec!(0.1),
        "test1".to_string(),
    );

    let strategy = Arc::new(TestStrategy::new("test1", vec!["BTCUSDT".to_string()]));
    strategy.set_signals(vec![signal.clone()]);
    executor.register(strategy);

    let bar = StrategyKLine {
        symbol: "BTCUSDT".to_string(),
        period: "1m".to_string(),
        open: dec!(50000),
        high: dec!(51000),
        low: dec!(49000),
        close: dec!(50500),
        volume: dec!(100),
        timestamp: Utc::now(),
    };

    let signals = executor.dispatch(&bar);

    assert_eq!(signals.len(), 1, "应返回 1 个信号");
    assert_eq!(signals[0].symbol, "BTCUSDT");
    assert_eq!(signals[0].direction, Direction::Long);
}

#[test]
fn test_executor_dispatch_to_multiple_strategies() {
    let executor = StrategyExecutor::new();

    let signal1 = TradingSignal::new("BTCUSDT".to_string(), Direction::Long, dec!(0.1), "test1".to_string());
    let signal2 = TradingSignal::new("BTCUSDT".to_string(), Direction::Long, dec!(0.2), "test2".to_string());

    let strategy1 = Arc::new(TestStrategy::new("test1", vec!["BTCUSDT".to_string()]));
    strategy1.set_signals(vec![signal1]);

    let strategy2 = Arc::new(TestStrategy::new("test2", vec!["BTCUSDT".to_string()]));
    strategy2.set_signals(vec![signal2]);

    executor.register(strategy1);
    executor.register(strategy2);

    let bar = StrategyKLine {
        symbol: "BTCUSDT".to_string(),
        period: "1m".to_string(),
        open: dec!(50000),
        high: dec!(51000),
        low: dec!(49000),
        close: dec!(50500),
        volume: dec!(100),
        timestamp: Utc::now(),
    };

    let signals = executor.dispatch(&bar);

    assert_eq!(signals.len(), 2, "应返回 2 个信号");
}

#[test]
fn test_executor_dispatch_no_strategies_for_symbol() {
    let executor = StrategyExecutor::new();

    let strategy = Arc::new(TestStrategy::new("test1", vec!["ETHUSDT".to_string()]));
    executor.register(strategy);

    let bar = StrategyKLine {
        symbol: "BTCUSDT".to_string(), // 不在策略关注列表
        period: "1m".to_string(),
        open: dec!(50000),
        high: dec!(51000),
        low: dec!(49000),
        close: dec!(50500),
        volume: dec!(100),
        timestamp: Utc::now(),
    };

    let signals = executor.dispatch(&bar);

    assert!(signals.is_empty(), "不应返回任何信号");
}

#[test]
fn test_executor_dispatch_disabled_strategy() {
    let executor = StrategyExecutor::new();

    let signal = TradingSignal::new("BTCUSDT".to_string(), Direction::Long, dec!(0.1), "test1".to_string());

    let strategy = Arc::new(TestStrategy::new("test1", vec!["BTCUSDT".to_string()]));
    strategy.set_signals(vec![signal]);
    strategy.set_enabled(false);

    executor.register(strategy);

    let bar = StrategyKLine {
        symbol: "BTCUSDT".to_string(),
        period: "1m".to_string(),
        open: dec!(50000),
        high: dec!(51000),
        low: dec!(49000),
        close: dec!(50500),
        volume: dec!(100),
        timestamp: Utc::now(),
    };

    let signals = executor.dispatch(&bar);

    assert!(signals.is_empty(), "禁用策略不应返回信号");
}

#[test]
fn test_executor_get_signal() {
    let executor = StrategyExecutor::new();

    let signal1 = TradingSignal::new("BTCUSDT".to_string(), Direction::Long, dec!(0.1), "test1".to_string());
    let signal2 = TradingSignal::new("BTCUSDT".to_string(), Direction::Long, dec!(0.2), "test2".to_string());

    let strategy1 = Arc::new(TestStrategy::new("test1", vec!["BTCUSDT".to_string()]));
    strategy1.set_signals(vec![signal1]);

    let strategy2 = Arc::new(TestStrategy::new("test2", vec!["BTCUSDT".to_string()]));
    let mut s2 = signal2.clone();
    s2.priority = 80; // 高优先级
    strategy2.set_signals(vec![s2]);

    executor.register(strategy1);
    executor.register(strategy2);

    let bar = StrategyKLine {
        symbol: "BTCUSDT".to_string(),
        period: "1m".to_string(),
        open: dec!(50000),
        high: dec!(51000),
        low: dec!(49000),
        close: dec!(50500),
        volume: dec!(100),
        timestamp: Utc::now(),
    };

    executor.dispatch(&bar);

    // 获取最高优先级信号
    let signal = executor.get_signal("BTCUSDT");
    assert!(signal.is_some());
    assert_eq!(signal.unwrap().priority, 80);
}

#[test]
fn test_executor_get_signal_for_strategy() {
    let executor = StrategyExecutor::new();

    let signal = TradingSignal::new("BTCUSDT".to_string(), Direction::Long, dec!(0.1), "test1".to_string());

    let strategy = Arc::new(TestStrategy::new("test1", vec!["BTCUSDT".to_string()]));
    strategy.set_signals(vec![signal]);
    executor.register(strategy);

    let bar = StrategyKLine {
        symbol: "BTCUSDT".to_string(),
        period: "1m".to_string(),
        open: dec!(50000),
        high: dec!(51000),
        low: dec!(49000),
        close: dec!(50500),
        volume: dec!(100),
        timestamp: Utc::now(),
    };

    executor.dispatch(&bar);

    let signal = executor.get_signal_for_strategy("BTCUSDT", "test1");
    assert!(signal.is_some());
    assert_eq!(signal.unwrap().strategy_id, "test1");
}

#[test]
fn test_executor_clear() {
    let executor = StrategyExecutor::new();

    let strategy = Arc::new(TestStrategy::new("test1", vec!["BTCUSDT".to_string()]));
    executor.register(strategy);

    assert_eq!(executor.count(), 1);

    executor.clear();

    assert_eq!(executor.count(), 0);
    assert_eq!(executor.symbol_count(), 0);
}

// ============================================================================
// SignalAggregator 测试
// ============================================================================

#[test]
fn test_signal_aggregator_empty() {
    let aggregator = SignalAggregator::new(10);
    let result = aggregator.aggregate(Vec::new());
    assert!(result.is_empty());
}

#[test]
fn test_signal_aggregator_single_signal() {
    let aggregator = SignalAggregator::new(10);

    let signals = vec![TradingSignal::new(
        "BTCUSDT".to_string(),
        Direction::Long,
        dec!(0.1),
        "test1".to_string(),
    )];

    let result = aggregator.aggregate(signals);

    assert_eq!(result.len(), 1);
}

#[test]
fn test_signal_aggregator_same_direction_max_qty() {
    let aggregator = SignalAggregator::new(10);

    let mut signal1 = TradingSignal::new("BTCUSDT".to_string(), Direction::Long, dec!(0.1), "test1".to_string());
    let mut signal2 = TradingSignal::new("BTCUSDT".to_string(), Direction::Long, dec!(0.2), "test2".to_string());

    signal1.priority = 50;
    signal2.priority = 50;

    let signals = vec![signal1, signal2];
    let result = aggregator.aggregate(signals);

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].quantity, dec!(0.2), "应保留最大数量");
}

#[test]
fn test_signal_aggregator_priority_order() {
    let aggregator = SignalAggregator::new(10);

    // 使用不同品种来测试优先级排序
    let mut signal1 = TradingSignal::new("BTCUSDT".to_string(), Direction::Long, dec!(0.1), "test1".to_string());
    signal1.priority = 30;

    let mut signal2 = TradingSignal::new("ETHUSDT".to_string(), Direction::Long, dec!(0.1), "test2".to_string());
    signal2.priority = 80;

    let signals = vec![signal1, signal2];
    let result = aggregator.aggregate(signals);

    // 两个不同品种的信号都会被保留
    assert_eq!(result.len(), 2);
    // 由于排序是按优先级降序，ETHUSDT (test2, priority=80) 应该在 BTCUSDT (test1, priority=30) 之前
    assert_eq!(result[0].strategy_id, "test2", "高优先级信号在前");
    assert_eq!(result[1].strategy_id, "test1", "低优先级信号在后");
}

#[test]
fn test_signal_aggregator_max_signals_limit() {
    let aggregator = SignalAggregator::new(2);

    let mut signals = Vec::new();
    for i in 0..5 {
        let mut signal = TradingSignal::new(
            format!("SYMBOL{}", i),
            Direction::Long,
            dec!(0.1),
            format!("test{}", i),
        );
        signal.priority = 100 - i;
        signals.push(signal);
    }

    let result = aggregator.aggregate(signals);

    assert_eq!(result.len(), 2, "应限制为最大信号数");
}

#[test]
fn test_signal_aggregator_opposite_direction_both_kept() {
    let aggregator = SignalAggregator::new(10);

    let signal_long = TradingSignal::new("BTCUSDT".to_string(), Direction::Long, dec!(0.1), "test1".to_string());
    let signal_short = TradingSignal::new("BTCUSDT".to_string(), Direction::Short, dec!(0.1), "test2".to_string());

    let signals = vec![signal_long, signal_short];
    let result = aggregator.aggregate(signals);

    assert_eq!(result.len(), 2, "相反方向的信号都应保留");
}

// ============================================================================
// TradingSignal 辅助函数测试
// ============================================================================

#[test]
fn test_trading_signal_builder_pattern() {
    let signal = TradingSignal::new(
        "BTCUSDT".to_string(),
        Direction::Long,
        dec!(0.1),
        "test".to_string(),
    )
    .with_price(dec!(50000))
    .with_stop_loss(dec!(49000))
    .with_take_profit(dec!(52000))
    .with_signal_type(SignalType::Open)
    .with_priority(80);

    assert_eq!(signal.price, Some(dec!(50000)));
    assert_eq!(signal.stop_loss, Some(dec!(49000)));
    assert_eq!(signal.take_profit, Some(dec!(52000)));
    assert_eq!(signal.signal_type, SignalType::Open);
    assert_eq!(signal.priority, 80);
}

#[test]
fn test_trading_signal_is_valid() {
    let valid_signal = TradingSignal::new(
        "BTCUSDT".to_string(),
        Direction::Long,
        dec!(0.1),
        "test".to_string(),
    );
    assert!(valid_signal.is_valid());

    let invalid_zero_qty = TradingSignal::new(
        "BTCUSDT".to_string(),
        Direction::Long,
        dec!(0),
        "test".to_string(),
    );
    assert!(!invalid_zero_qty.is_valid(), "零数量应无效");

    let invalid_flat = TradingSignal::new(
        "BTCUSDT".to_string(),
        Direction::Flat,
        dec!(0.1),
        "test".to_string(),
    );
    assert!(!invalid_flat.is_valid(), "Flat 方向应无效");
}

#[test]
fn test_trading_signal_is_open_close() {
    let mut open_signal = TradingSignal::new(
        "BTCUSDT".to_string(),
        Direction::Long,
        dec!(0.1),
        "test".to_string(),
    );
    assert!(open_signal.is_open());
    assert!(!open_signal.is_close());

    open_signal.signal_type = SignalType::Close;
    assert!(!open_signal.is_open());
    assert!(open_signal.is_close());

    open_signal.signal_type = SignalType::Add;
    assert!(!open_signal.is_open());
    assert!(!open_signal.is_close());
}

// ============================================================================
// Direction 和 SignalType 枚举测试
// ============================================================================

#[test]
fn test_direction_default() {
    let direction = Direction::default();
    assert_eq!(direction, Direction::Flat);
}

#[test]
fn test_signal_type_default() {
    let signal_type = SignalType::default();
    assert_eq!(signal_type, SignalType::Close);
}

// ============================================================================
// StrategyState 测试
// ============================================================================

#[test]
fn test_strategy_state_new() {
    let state = StrategyState::new("test_strategy".to_string());

    assert_eq!(state.id, "test_strategy");
    assert!(state.enabled);
    assert_eq!(state.position_direction, Direction::Flat);
    assert_eq!(state.position_qty, Decimal::ZERO);
}
