//! 压力测试：批量并发下单
//!
//! 场景：同时发送 50 笔订单，测试线程安全和锁竞争

use std::sync::Arc;
use std::time::Instant;

use rust_decimal_macros::dec;

use h_sandbox::{ShadowBinanceGateway, ShadowConfig};
use a_common::models::types::Side;

#[tokio::main]
async fn main() {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║         压力测试：批量并发下单（50笔同时）                  ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    // 配置
    let initial_balance = dec!(100000.0);
    let config = ShadowConfig::new(initial_balance);
    let gateway = Arc::new(ShadowBinanceGateway::new(initial_balance, config));
    let order_count = 50;
    let symbol = "BTCUSDT".to_string();

    println!("配置:");
    println!("  初始资金: {} USDT", initial_balance);
    println!("  并发订单数: {}", order_count);
    println!();

    println!("发送 {} 笔订单...\n", order_count);

    let start = Instant::now();

    // 并发下单（用 Vec 收集 handle）
    let mut handles = Vec::new();
    for i in 0..order_count {
        let gw = gateway.clone();
        let symbol = symbol.clone();
        let side = if i % 2 == 0 { Side::Buy } else { Side::Sell };
        handles.push(tokio::spawn(async move {
            let req = f_engine::types::OrderRequest::new_market(
                symbol,
                side,
                dec!(0.001),
            );
            gw.place_order(req)
        }));
    }

    // 等待所有订单完成
    let mut success_count = 0u32;
    let mut fail_count = 0u32;

    for handle in handles {
        match handle.await {
            Ok(Ok(result)) => {
                if result.status == a_common::models::types::OrderStatus::Filled {
                    success_count += 1;
                } else {
                    fail_count += 1;
                }
            }
            _ => fail_count += 1,
        }
    }

    let elapsed = start.elapsed();

    // 获取最终账户状态
    let account = gateway.get_account().unwrap();

    // 输出结果
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║                    测试结果                                 ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();
    println!("  总耗时: {:.2}ms", elapsed.as_secs_f64() * 1000.0);
    println!("  并发数: {}", order_count);
    println!("  QPS: {:.0}", order_count as f64 / elapsed.as_secs_f64());
    println!();
    println!("  成功: {} 笔", success_count);
    println!("  失败: {} 笔", fail_count);
    println!();
    println!("  初始资金: {}", initial_balance);
    println!("  最终权益: {}", account.total_equity);
    println!("  可用余额: {}", account.available);
    println!("  冻结保证金: {}", account.frozen_margin);
    println!("  未实现盈亏: {}", account.unrealized_pnl);

    // 验证线程安全
    if fail_count == 0 {
        println!("\n✅ 批量并发下单测试通过 - 线程安全正常");
    } else {
        println!("\n⚠️  部分订单失败");
    }

    // 验证资金一致性
    let total = account.available + account.frozen_margin;
    if total == initial_balance + account.unrealized_pnl {
        println!("✅ 资金一致性检查通过");
    } else {
        println!("⚠️  资金不一致: {} != {}", total, initial_balance + account.unrealized_pnl);
    }
}
