//! 测试交易设置 API
//!
//! 运行: cargo run --example test_trade_settings --package b_data_source

use a_common::BinanceApiGateway;
use b_data_source::TradeSettings;

#[tokio::main]
async fn main() {
    println!("=== 测试 Binance API ===\n");

    // 使用测试网账户模式
    let gateway = BinanceApiGateway::new_futures_with_testnet();

    println!("API 配置:");
    println!("  市场API (行情): {}", gateway.market_api_base());
    println!("  账户API (交易): {}", gateway.account_api_base());
    println!();

    // 测试 1: 获取账户信息
    println!("=== 1. 获取账户信息 ===");
    match gateway.fetch_futures_account().await {
        Ok(account) => {
            println!("  总保证金: {}", account.total_margin_balance);
            println!("  可用余额: {}", account.available_balance);
            println!("  未实现盈亏: {}", account.total_unrealized_profit);
            println!("  更新时间: {}", account.update_time);
        }
        Err(e) => println!("  获取失败: {:?}", e),
    }
    println!();

    // 测试 2: 获取持仓信息
    println!("=== 2. 获取持仓信息 ===");
    match gateway.fetch_futures_positions().await {
        Ok(positions) => {
            if positions.is_empty() {
                println!("  当前无持仓");
            } else {
                for pos in &positions {
                    println!("  {} {} 数量:{} 杠杆:{}x",
                        pos.symbol, pos.position_side, pos.position_amt, pos.leverage);
                }
            }
        }
        Err(e) => println!("  获取失败: {:?}", e),
    }
    println!();

    // 测试 3: 获取手续费率
    println!("=== 3. 获取 BTCUSDT 手续费率 ===");
    match gateway.get_commission_rate("BTCUSDT").await {
        Ok((maker, taker)) => {
            println!("  Maker 费率: {}", maker);
            println!("  Taker 费率: {}", taker);
        }
        Err(e) => println!("  获取失败: {:?}", e),
    }
    println!();

    // 测试 4: 获取杠杆档位
    println!("=== 4. 获取 BTCUSDT 杠杆档位 ===");
    match gateway.fetch_leverage_brackets(Some("BTCUSDT")).await {
        Ok(brackets) => {
            if let Some(first) = brackets.first() {
                println!("  最大杠杆: {}x", first.max_leverage);
                println!("  档位: {}", first.bracket);
            }
        }
        Err(e) => println!("  获取失败: {:?}", e),
    }
    println!();

    // 测试 5: TradeSettings 封装
    println!("=== 5. TradeSettings 测试 ===");
    let settings = TradeSettings::with_testnet();

    // 获取手续费率
    match settings.get_commission_rate("ETHUSDT").await {
        Ok((maker, taker)) => {
            println!("  ETHUSDT Maker: {} Taker: {}", maker, taker);
        }
        Err(e) => println!("  获取手续费率失败: {:?}", e),
    }

    println!("\n=== 测试完成 ===");
}
