//! 压力测试：价格闪崩/插针场景
//!
//! 场景：模拟价格在短时间内剧烈波动，测试账户盈亏计算和风控

use std::sync::Arc;

use rust_decimal_macros::dec;

use h_sandbox::{ShadowBinanceGateway, ShadowConfig};
use a_common::models::types::Side;

#[tokio::main]
async fn main() {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║         压力测试：价格闪崩/插针场景                        ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    // 配置
    let initial_balance = dec!(100000.0);
    let config = ShadowConfig::new(initial_balance);
    let gateway = Arc::new(ShadowBinanceGateway::new(initial_balance, config));
    let symbol = "BTCUSDT";

    println!("配置:");
    println!("  初始资金: {} USDT", initial_balance);
    println!();

    // 预设价格（市价单需要当前市场价格）
    let current_price = dec!(50000.0);
    gateway.update_price(symbol, current_price);
    println!("预设市场价格: {}\n", current_price);

    // 开多单
    let open_req = f_engine::types::OrderRequest::new_market(
        symbol.to_string(),
        Side::Buy,
        dec!(1.0),  // 1 BTC
    );
    let result = gateway.place_order(open_req).unwrap();
    println!("📈 开多 @ {} (数量: {})", result.filled_price, result.filled_qty);

    // 更新价格到开仓价
    let open_price = result.filled_price;
    gateway.update_price(symbol, open_price);

    // 模拟价格闪崩场景
    println!("\n模拟价格闪崩...");
    let scenarios = vec![
        ("正常波动", dec!(0.995), dec!(1.005)),      // -0.5% ~ +0.5%
        ("小幅下跌", dec!(0.97), dec!(0.99)),        // -3% ~ -1%
        ("闪崩", dec!(0.85), dec!(0.95)),           // -15% ~ -5%
        ("极端闪崩", dec!(0.70), dec!(0.85)),        // -30% ~ -15%
        ("V形反转", dec!(0.80), dec!(1.10)),        // -20% ~ +10%
    ];

    for (name, min_ratio, max_ratio) in scenarios {
        let spike_price_low = open_price * min_ratio;
        let spike_price_high = open_price * max_ratio;
        let current_price = spike_price_low;  // 使用最低价作为当前价格

        gateway.update_price(symbol, current_price);

        let account = gateway.get_account().unwrap();
        let pnl_pct = (current_price - open_price) / open_price * dec!(100);

        println!("\n  {}: {} -> {} ({}%)", name, open_price, current_price, pnl_pct);
        println!("    权益: {}", account.total_equity);
        println!("    未实现盈亏: {}", account.unrealized_pnl);

        // 检查风控阈值
        if account.unrealized_pnl < -initial_balance * dec!(0.05) {
            println!("    ⚠️  触发50%亏损风控线！");
        }
        if account.unrealized_pnl < -initial_balance * dec!(0.10) {
            println!("    🔴 触发100%亏损（爆仓线）！");
        }
    }

    // 恢复价格并平仓
    gateway.update_price(symbol, open_price);
    let close_req = f_engine::types::OrderRequest::new_market(
        symbol.to_string(),
        Side::Sell,
        dec!(1.0),
    );
    let close_result = gateway.place_order(close_req).unwrap();
    println!("\n📉 平多 @ {} (数量: {})", close_result.filled_price, close_result.filled_qty);

    // 最终账户状态
    let account = gateway.get_account().unwrap();
    let total_pnl = account.total_equity - initial_balance;
    let pnl_pct = total_pnl / initial_balance * dec!(100);

    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║                    测试结果                                 ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();
    println!("  初始资金: {}", initial_balance);
    println!("  最终权益: {}", account.total_equity);
    println!("  总盈亏: {} ({}%)", total_pnl, pnl_pct);
    println!("  保证金: {}", account.frozen_margin);
    println!("  可用: {}", account.available);

    // 验证账户资金一致性
    let expected_total = account.available + account.frozen_margin;
    if expected_total == initial_balance + account.unrealized_pnl {
        println!("\n✅ 账户资金一致性检查通过");
    } else {
        println!("\n⚠️  账户资金不一致: {} != {}", expected_total, initial_balance + account.unrealized_pnl);
    }

    println!("\n✅ 价格闪崩/插针场景测试完成");
}
