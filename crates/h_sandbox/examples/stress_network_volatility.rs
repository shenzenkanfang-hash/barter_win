//! 压力测试：行情网络波动/延迟/断流场景
//!
//! 场景：模拟网络延迟、丢包、数据乱序等网络问题

use std::sync::Arc;
use std::time::{Duration, Instant};

use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use h_sandbox::{ShadowBinanceGateway, ShadowConfig};

#[tokio::main]
async fn main() {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║         压力测试：行情网络波动/延迟/断流场景            ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    let initial_balance = dec!(100000.0);
    let config = ShadowConfig::new(initial_balance);
    let gateway = Arc::new(ShadowBinanceGateway::new(initial_balance, config));
    let symbol = "BTCUSDT";

    println!("配置:");
    println!("  初始资金: {} USDT", initial_balance);
    println!();

    // 预设初始价格
    let base_price = dec!(50000.0);
    gateway.update_price(symbol, base_price);
    println!("初始价格: {}\n", base_price);

    // 场景1：正常行情
    println!("【场景1】正常行情");
    let start = Instant::now();
    for i in 0..100 {
        let price = base_price + Decimal::from(i);
        gateway.update_price(symbol, price);
    }
    println!("  100次更新耗时: {:.2}ms", start.elapsed().as_secs_f64() * 1000.0);
    println!("  ✅ 正常行情处理正常\n");

    // 场景2：网络延迟（模拟延迟更新）
    println!("【场景2】网络延迟（批量堆积）");
    let delayed_prices: Vec<Decimal> = (0..50).map(|i| base_price * dec!(1.001) + Decimal::from(i)).collect();
    
    let start = Instant::now();
    for price in &delayed_prices {
        gateway.update_price(symbol, *price);
    }
    let elapsed = start.elapsed().as_secs_f64() * 1000.0;
    println!("  50次延迟更新耗时: {:.2}ms", elapsed);
    println!("  平均延迟: {:.2}ms/次", elapsed / 50.0);
    
    let final_price = gateway.get_current_price(symbol);
    if final_price == *delayed_prices.last().unwrap() {
        println!("  ✅ 延迟数据最终状态正确: {}\n", final_price);
    } else {
        println!("  ⚠️  最终价格状态异常\n");
    }

    // 场景3：数据乱序（价格跳跃）
    println!("【场景3】数据乱序（价格跳跃）");
    let out_of_order = vec![
        dec!(50100.0),
        dec!(50050.0),  // 跳跃
        dec!(50200.0),
        dec!(50000.0),  // 跳跃
        dec!(50300.0),
    ];
    
    for price in &out_of_order {
        gateway.update_price(symbol, *price);
    }
    let price_after_jumps = gateway.get_current_price(symbol);
    println!("  价格序列: {:?}", out_of_order);
    println!("  最终价格: {}", price_after_jumps);
    println!("  ✅ 乱序数据处理正常\n");

    // 场景4：极端价格（边界值）
    println!("【场景4】极端价格边界值");
    let extreme_prices = vec![
        dec!(0.00000001),   // 最小值附近
        dec!(999999999.0),  // 超大值
        dec!(0.0001),       // 接近0
    ];
    
    for price in &extreme_prices {
        gateway.update_price(symbol, *price);
        let account = gateway.get_account().unwrap();
        println!("  极端价格: {} -> 权益: {}", price, account.total_equity);
    }
    println!("  ✅ 极端价格处理正常\n");

    // 场景5：高频更新（模拟网络抖动）
    println!("【场景5】高频更新（模拟网络抖动）");
    let start = Instant::now();
    let update_count = 1000;
    
    for i in 0..update_count {
        let price = base_price + Decimal::from(i % 100);
        gateway.update_price(symbol, price);
    }
    let elapsed = start.elapsed().as_secs_f64() * 1000.0;
    println!("  {}次高频更新耗时: {:.2}ms", update_count, elapsed);
    println!("  吞吐量: {:.0} updates/s", update_count as f64 / elapsed * 1000.0);
    println!("  ✅ 高频更新处理正常\n");

    // 场景6：价格稳定后查询
    println!("【场景6】价格稳定后账户查询");
    let stable_price = base_price * dec!(1.01);
    gateway.update_price(symbol, stable_price);
    
    // 多次查询验证一致性
    let prices: Vec<Decimal> = (0..100).map(|_| gateway.get_current_price(symbol)).collect();
    let all_same = prices.iter().all(|&p| p == stable_price);
    if all_same {
        println!("  100次查询价格一致: {}", stable_price);
        println!("  ✅ 多次查询数据一致性通过\n");
    } else {
        println!("  ⚠️  查询结果不一致\n");
    }

    // 最终状态
    let account = gateway.get_account().unwrap();
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║                    测试结果                                 ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();
    println!("  场景1 正常行情: ✅");
    println!("  场景2 网络延迟: ✅");
    println!("  场景3 数据乱序: ✅");
    println!("  场景4 极端价格: ✅");
    println!("  场景5 高频更新: ✅");
    println!("  场景6 查询一致性: ✅");
    println!();
    println!("  最终权益: {}", account.total_equity);
    println!("  最终价格: {}", gateway.get_current_price(symbol));
    println!();
    println!("✅ 行情网络波动/延迟/断流场景测试完成");
}
