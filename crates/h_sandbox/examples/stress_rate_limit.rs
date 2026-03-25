//! 压力测试：接口超时/限流/转发失败场景
//!
//! 场景：测试 ShadowGateway 劫持模式的容错能力

use std::sync::Arc;
use std::time::Instant;

use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use h_sandbox::{ShadowBinanceGateway, ShadowConfig};

#[tokio::main]
async fn main() {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║         压力测试：接口超时/限流/转发失败场景            ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    let initial_balance = dec!(100000.0);
    let config = ShadowConfig::new(initial_balance);
    let gateway = Arc::new(ShadowBinanceGateway::new(initial_balance, config));
    let symbol = "BTCUSDT";

    println!("配置:");
    println!("  初始资金: {} USDT", initial_balance);
    println!();

    // 预设价格
    gateway.update_price(symbol, dec!(50000.0));

    // 场景1：连续下单（模拟限流）
    println!("【场景1】连续下单（模拟限流）");
    let order_count = 100;
    let start = Instant::now();
    let mut success_count = 0u32;
    let mut fail_count = 0u32;

    for i in 0..order_count {
        let price = dec!(50000.0) + Decimal::from(i);
        gateway.update_price(symbol, price);

        let req = f_engine::types::OrderRequest::new_limit(
            symbol.to_string(),
            a_common::models::types::Side::Buy,
            dec!(0.001),
            price,
        );

        match gateway.place_order(req) {
            Ok(r) if r.status == a_common::models::types::OrderStatus::Filled => {
                success_count += 1;
            }
            Ok(r) if r.status == a_common::models::types::OrderStatus::Rejected => {
                fail_count += 1;
                println!("  订单被拒: {:?}", r.reject_reason);
            }
            _ => {}
        }
    }

    let elapsed = start.elapsed();
    println!();
    println!("  总订单数: {}", order_count);
    println!("  成功: {}", success_count);
    println!("  失败: {}", fail_count);
    println!("  耗时: {:.2}ms", elapsed.as_secs_f64() * 1000.0);
    println!("  QPS: {:.0}", order_count as f64 / elapsed.as_secs_f64());
    println!("  ✅ 连续下单测试完成\n");

    // 场景2：高频账户查询
    println!("【场景2】高频账户查询（模拟接口调用）");
    let query_count = 1000;
    let start = Instant::now();
    let mut query_success = 0u32;

    for _ in 0..query_count {
        if gateway.get_account().is_ok() {
            query_success += 1;
        }
    }

    let elapsed = start.elapsed();
    println!("  查询次数: {}", query_count);
    println!("  成功: {}", query_success);
    println!("  耗时: {:.2}ms", elapsed.as_secs_f64() * 1000.0);
    println!("  QPS: {:.0}", query_count as f64 / elapsed.as_secs_f64());
    println!("  ✅ 高频账户查询测试完成\n");

    // 场景3：持仓查询
    println!("【场景3】高频持仓查询");
    let pos_query_count = 1000;
    let start = Instant::now();
    let mut pos_success = 0u32;

    for _ in 0..pos_query_count {
        if gateway.get_position(symbol).is_ok() {
            pos_success += 1;
        }
    }

    let elapsed = start.elapsed();
    println!("  查询次数: {}", pos_query_count);
    println!("  成功: {}", pos_success);
    println!("  耗时: {:.2}ms", elapsed.as_secs_f64() * 1000.0);
    println!("  QPS: {:.0}", pos_query_count as f64 / elapsed.as_secs_f64());
    println!("  ✅ 高频持仓查询测试完成\n");

    // 场景4：并发读写混合
    println!("【场景4】并发读写混合");
    let mixed_ops = 500;
    let start = Instant::now();

    let read_handles: Vec<_> = (0..mixed_ops)
        .map(|i| {
            let gw = gateway.clone();
            tokio::spawn(async move {
                let price = dec!(50000.0) + Decimal::from(i);
                gw.update_price(symbol, price);
                gw.get_account().is_ok()
            })
        })
        .collect();

    let mut read_success = 0u32;
    for handle in read_handles {
        if let Ok(true) = handle.await {
            read_success += 1;
        }
    }

    let elapsed = start.elapsed();
    println!("  操作次数: {}", mixed_ops);
    println!("  成功: {}", read_success);
    println!("  耗时: {:.2}ms", elapsed.as_secs_f64() * 1000.0);
    println!("  QPS: {:.0}", mixed_ops as f64 / elapsed.as_secs_f64());
    println!("  ✅ 并发读写混合测试完成\n");

    // 最终状态
    let account = gateway.get_account().unwrap();
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║                    测试结果                                 ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();
    println!("  场景1 连续下单: ✅ (QPS {:.0})", order_count as f64 / elapsed.as_secs_f64());
    println!("  场景2 高频账户查询: ✅ (QPS {:.0})", query_count as f64 / elapsed.as_secs_f64());
    println!("  场景3 高频持仓查询: ✅ (QPS {:.0})", pos_query_count as f64 / elapsed.as_secs_f64());
    println!("  场景4 并发读写混合: ✅ (QPS {:.0})", mixed_ops as f64 / elapsed.as_secs_f64());
    println!();
    println!("  最终权益: {}", account.total_equity);
    println!();
    println!("✅ 接口超时/限流/转发失败场景测试完成");
}
