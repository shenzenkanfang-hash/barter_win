//! 模拟交易系统 - 完整闭环
//!
//! 模拟真实生产环境的所有组件：
//! - DataFeeder (数据源)
//! - ShadowRiskChecker (风控)
//! - ShadowBinanceGateway (劫持网关)
//! - Account (账户)
//! - Position (持仓)
//!
//! 运行: cargo run -p h_sandbox --example sim_trading

use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use b_data_source::DataFeeder;
use h_sandbox::{
    ShadowBinanceGateway, ShadowConfig, ShadowRiskChecker,
    Account, Side,
};

/// 交易动作
#[derive(Debug, Clone, Copy)]
enum TradingAction {
    OpenLong,
    CloseLong,
    Hold,
}

/// 模拟交易系统
struct SimTradingSystem {
    data_feeder: Arc<DataFeeder>,
    gateway: Arc<ShadowBinanceGateway>,
    risk_checker: ShadowRiskChecker,
    account: Account,
    last_signal_price: Decimal,
    position_open: bool,
}

impl SimTradingSystem {
    fn new(initial_balance: Decimal) -> Self {
        let config = ShadowConfig::new(initial_balance);
        
        let data_feeder = Arc::new(DataFeeder::new());
        let gateway = ShadowBinanceGateway::new(initial_balance, config.clone());
        let risk_checker = ShadowRiskChecker::new();
        let account = Account::new(initial_balance, &config);

        Self {
            data_feeder,
            gateway: Arc::new(gateway),
            risk_checker,
            account,
            last_signal_price: Decimal::ZERO,
            position_open: false,
        }
    }

    /// 处理 Tick
    fn on_tick(&mut self, symbol: &str, price: Decimal, qty: Decimal) -> Option<TradingAction> {
        // 1. 更新 DataFeeder
        let tick = b_data_source::Tick {
            symbol: symbol.to_string(),
            price,
            qty,
            timestamp: Utc::now(),
            kline_1m: None,
            kline_15m: None,
            kline_1d: None,
        };
        self.data_feeder.push_tick(tick);

        // 2. 更新账户价格
        self.account.update_price(symbol, price);

        // 3. 简单策略：价格突破
        let action = self.simple_breakout_strategy(symbol, price);

        // 4. 执行交易
        if let Some(action) = &action {
            self.execute_action(symbol, action, price, qty);
        }

        action
    }

    /// 简单突破策略
    fn simple_breakout_strategy(&self, symbol: &str, price: Decimal) -> Option<TradingAction> {
        // 获取最近 10 个 tick 的均价
        let prices = self.data_feeder.ws_get_1m(symbol);
        
        if prices.is_none() {
            return None;
        }

        // 简单策略：价格变化 > 0.1% 时交易
        let price_change = if self.last_signal_price.is_zero() {
            Decimal::ZERO
        } else {
            ((price - self.last_signal_price) / self.last_signal_price).abs()
        };

        if price_change > dec!(0.001) {
            if !self.position_open {
                Some(TradingAction::OpenLong)
            } else if price_change > dec!(0.005) {
                Some(TradingAction::CloseLong)
            } else {
                None
            }
        } else {
            None
        }
    }

    /// 执行交易
    fn execute_action(&mut self, symbol: &str, action: &TradingAction, price: Decimal, qty: Decimal) {
        match action {
            TradingAction::OpenLong => {
                // 通过 ShadowGateway 下单
                let req = f_engine::types::OrderRequest::new_market(
                    symbol.to_string(),
                    a_common::models::types::Side::Buy,
                    qty,
                );

                let result = self.gateway.place_order(req);
                match result {
                    Ok(order) => {
                        self.position_open = true;
                        self.account.update_price(symbol, price);
                        println!("[{:?}] 📈 开多 @ {} | 订单: {}", Utc::now(), price, order.order_id);
                    }
                    Err(e) => {
                        println!("[{:?}] ❌ 开多失败: {:?}", Utc::now(), e);
                    }
                }
            }
            TradingAction::CloseLong => {
                let req = f_engine::types::OrderRequest::new_market(
                    symbol.to_string(),
                    a_common::models::types::Side::Sell,
                    qty,
                );

                let result = self.gateway.place_order(req);
                match result {
                    Ok(order) => {
                        self.position_open = false;
                        println!("[{:?}] 📉 平多 @ {} | 订单: {}", Utc::now(), price, order.order_id);
                    }
                    Err(e) => {
                        println!("[{:?}] ❌ 平多失败: {:?}", Utc::now(), e);
                    }
                }
            }
            TradingAction::Hold => {}
        }
    }

    /// 打印状态
    fn print_status(&self, tick_count: u64, symbol: &str, price: Decimal) {
        let equity = self.account.total_equity();
        let unrealized_pnl = self.account.total_unrealized_pnl();
        let position = self.account.get_position(symbol);

        println!(
            "[{:04}] {} @ {} | 权益: {} | 未实现盈亏: {} | 持仓: {}",
            tick_count,
            symbol,
            price,
            equity,
            unrealized_pnl,
            if self.position_open { "有" } else { "无" }
        );
    }
}

#[tokio::main]
async fn main() {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║            模拟交易系统 - 完整闭环测试                          ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    // 配置
    let symbol = "BTCUSDT";
    let initial_balance = dec!(10000.0);
    let tick_count = 1000;

    println!("配置:");
    println!("  品种: {}", symbol);
    println!("  初始资金: {} USDT", initial_balance);
    println!("  测试Tick数: {}", tick_count);
    println!();

    // 1. 创建系统
    println!("初始化组件...");
    let mut system = SimTradingSystem::new(initial_balance);
    println!("✅ 组件初始化完成\n");

    // 2. 生成模拟 Tick
    println!("开始模拟交易...\n");

    let start = Instant::now();
    let mut current_price = dec!(50000.0);
    let mut last_print = Instant::now();

    for i in 0..tick_count {
        // 生成 tick
        let price_change = if i % 20 < 10 { dec!(1.0005) } else { dec!(0.9995) };
        current_price = current_price * price_change;

        // 处理 tick
        let _action = system.on_tick(symbol, current_price, dec!(0.001));

        // 定期打印状态
        if last_print.elapsed() > Duration::from_secs(5) || i == tick_count - 1 {
            system.print_status(i, symbol, current_price);
            last_print = Instant::now();
        }

        // 模拟延迟
        tokio::time::sleep(Duration::from_micros(100)).await;
    }

    let elapsed = start.elapsed();

    // 3. 最终报告
    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║                    模拟交易报告                               ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();
    println!("  运行时长: {:.2}s", elapsed.as_secs_f64());
    println!("  处理Tick: {}", tick_count);
    println!("  处理速率: {:.0} ticks/s", tick_count as f64 / elapsed.as_secs_f64());
    println!();

    let equity = system.account.total_equity();
    let unrealized_pnl = system.account.total_unrealized_pnl();
    let available = system.account.available();

    println!("  初始资金: {}", initial_balance);
    println!("  最终权益: {}", equity);
    println!("  未实现盈亏: {}", unrealized_pnl);
    println!("  可用余额: {}", available);
    println!("  收益率: {:.2}%", ((equity - initial_balance) / initial_balance) * dec!(100));

    println!("\n✅ 模拟交易完成");
}
