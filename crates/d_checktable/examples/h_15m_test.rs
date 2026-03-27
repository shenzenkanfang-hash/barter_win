//! h_15m 模块独立测试示例
//! 运行: cargo run -p d_checktable --example h_15m_test

use std::sync::Arc;
use d_checktable::h_15m::{Trader, TraderConfig, QuantityCalculatorConfig};
use d_checktable::h_15m::executor::Executor;
use d_checktable::h_15m::repository::Repository;
use b_data_source::default_store;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    
    let config = TraderConfig {
        symbol: "BTCUSDT".to_string(),
        ..Default::default()
    };
    
    let executor = Arc::new(Executor::new(d_checktable::h_15m::executor::ExecutorConfig {
        symbol: config.symbol.clone(),
        order_interval_ms: config.order_interval_ms,
        initial_ratio: config.initial_ratio,
        lot_size: config.lot_size,
        max_position: config.max_position,
    }));
    
    let repository = Arc::new(Repository::new(&config.symbol, ":memory:").unwrap());
    let store = default_store().clone();
    
    // 创建 Trader
    let trader = Trader::new(config, executor, repository, store);
    
    // 配置数量计算器
    let qty_config = QuantityCalculatorConfig {
        base_open_qty: rust_decimal_macros::dec!(0.05),
        max_position_qty: rust_decimal_macros::dec!(0.15),
        add_multiplier: rust_decimal_macros::dec!(1.5),
        vol_adjustment: true,
    };
    
    let trader = trader.with_quantity_calculator(qty_config);
    
    println!("Trader created successfully!");
}
