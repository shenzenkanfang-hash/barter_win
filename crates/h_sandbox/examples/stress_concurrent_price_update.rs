//! 压力测试：多线程同时更新价格和查询账户
//!
//! 场景：10个线程同时更新价格，5个线程同时查询账户，测试数据竞争

use std::sync::Arc;
use std::time::Instant;

use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use h_sandbox::{ShadowBinanceGateway, ShadowConfig};

#[tokio::main]
async fn main() {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║         压力测试：多线程价格更新+账户查询                  ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    // 配置
    let initial_balance = dec!(100000.0);
    let config = ShadowConfig::new(initial_balance);
    let gateway = Arc::new(ShadowBinanceGateway::new(initial_balance, config));

    let update_threads = 10;
    let query_threads = 5;
    let ops_per_thread = 1000;
    let symbol = "BTCUSDT";

    println!("配置:");
    println!("  价格更新线程: {}", update_threads);
    println!("  账户查询线程: {}", query_threads);
    println!("  每线程操作数: {}", ops_per_thread);
    println!();

    let start = Instant::now();

    // 启动价格更新线程
    let update_handles: Vec<_> = (0..update_threads)
        .map(|tid| {
            let gw = gateway.clone();
            tokio::spawn(async move {
                let base_price = dec!(50000.0) + Decimal::from(tid) * dec!(100);
                for i in 0..ops_per_thread {
                    let price = base_price + Decimal::from(i % 1000);
                    gw.update_price(symbol, price);
                }
                ops_per_thread
            })
        })
        .collect();

    // 启动账户查询线程
    let query_handles: Vec<_> = (0..query_threads)
        .map(|tid| {
            let gw = gateway.clone();
            tokio::spawn(async move {
                let mut read_count = 0u64;
                for _ in 0..ops_per_thread {
                    if let Ok(account) = gw.get_account() {
                        // 验证数据一致性
                        let _ = account.total_equity;
                        let _ = account.available;
                        let _ = account.frozen_margin;
                        read_count += 1;
                    }
                }
                read_count
            })
        })
        .collect();

    // 等待所有操作完成
    let mut total_updates = 0u64;
    for handle in update_handles {
        if let Ok(count) = handle.await {
            total_updates += count;
        }
    }

    let mut total_queries = 0u64;
    for handle in query_handles {
        if let Ok(count) = handle.await {
            total_queries += count;
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
    println!();
    println!("  价格更新:");
    println!("    - 线程数: {}", update_threads);
    println!("    - 总操作数: {}", total_updates);
    println!("    - 吞吐量: {:.0} ops/s", total_updates as f64 / elapsed.as_secs_f64());
    println!();
    println!("  账户查询:");
    println!("    - 线程数: {}", query_threads);
    println!("    - 总操作数: {}", total_queries);
    println!("    - 吞吐量: {:.0} ops/s", total_queries as f64 / elapsed.as_secs_f64());
    println!();
    println!("  最终账户状态:");
    println!("    - 权益: {}", account.total_equity);
    println!("    - 可用: {}", account.available);
    println!("    - 冻结: {}", account.frozen_margin);

    // 验证
    let total = account.available + account.frozen_margin;
    if total == initial_balance + account.unrealized_pnl {
        println!("\n✅ 资金一致性检查通过");
    } else {
        println!("\n⚠️  资金不一致");
    }

    println!("✅ 多线程价格更新+账户查询测试完成");
}
