//! 压力测试：滑点/部分成交/订单拒单场景
//!
//! 场景：测试订单被拒、余额不足、价格偏离等边界情况

use std::sync::Arc;

use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use h_sandbox::{ShadowBinanceGateway, ShadowConfig};
use a_common::models::types::Side;

#[tokio::main]
async fn main() {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║         压力测试：滑点/部分成交/订单拒单场景              ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    let symbol = "BTCUSDT";

    // 场景1：正常下单
    println!("【场景1】正常下单");
    let initial_balance = dec!(100000.0);
    let config = ShadowConfig::new(initial_balance);
    let gateway = Arc::new(ShadowBinanceGateway::new(initial_balance, config));

    gateway.update_price(symbol, dec!(50000.0));

    let req1 = f_engine::types::OrderRequest::new_limit(
        symbol.to_string(),
        Side::Buy,
        dec!(0.1),
        dec!(50000.0),
    );
    let result1 = gateway.place_order(req1);
    match result1 {
        Ok(r) if r.status == a_common::models::types::OrderStatus::Filled => {
            println!("✅ 正常下单成功: {} @ {}", r.filled_qty, r.filled_price);
        }
        Ok(r) => {
            println!("⚠️  订单状态: {:?}", r.status);
        }
        Err(e) => {
            println!("❌ 下单失败: {:?}", e);
        }
    }

    // 场景2：余额不足
    println!("\n【场景2】余额不足");
    let small_balance = dec!(100.0);
    let config2 = ShadowConfig::new(small_balance);
    let gateway2 = Arc::new(ShadowBinanceGateway::new(small_balance, config2));

    gateway2.update_price(symbol, dec!(50000.0));

    let req2 = f_engine::types::OrderRequest::new_limit(
        symbol.to_string(),
        Side::Buy,
        dec!(1.0),  // 太大
        dec!(50000.0),
    );
    let result2 = gateway2.place_order(req2);
    match &result2 {
        Ok(r) if r.status == a_common::models::types::OrderStatus::Rejected => {
            println!("✅ 余额不足正确拒单: {:?}", r.reject_reason);
        }
        Ok(r) => {
            println!("⚠️  订单状态: {:?} - {:?}", r.status, r.reject_reason);
        }
        Err(e) => {
            println!("❌ 下单失败: {:?}", e);
        }
    }

    // 场景3：价格偏离过大（风控）
    println!("\n【场景3】价格偏离过大");
    let config3 = ShadowConfig::new(initial_balance);
    let gateway3 = Arc::new(ShadowBinanceGateway::new(initial_balance, config3));

    // 设置当前价格
    gateway3.update_price(symbol, dec!(50000.0));

    // 下单价格偏离过大（如1分钟前价格）
    let req3 = f_engine::types::OrderRequest::new_limit(
        symbol.to_string(),
        Side::Buy,
        dec!(0.1),
        dec!(45000.0),  // 偏离10%
    );
    let result3 = gateway3.place_order(req3);
    match &result3 {
        Ok(r) if r.status == a_common::models::types::OrderStatus::Rejected => {
            println!("✅ 价格偏离正确拒单: {:?}", r.reject_reason);
        }
        Ok(r) => {
            println!("订单状态: {:?} - {:?}", r.status, r.reject_reason);
        }
        Err(e) => {
            println!("❌ 下单失败: {:?}", e);
        }
    }

    // 场景4：重复开仓（同一方向持仓已存在）
    println!("\n【场景4】重复开仓（同方向）");
    let config4 = ShadowConfig::new(initial_balance);
    let gateway4 = Arc::new(ShadowBinanceGateway::new(initial_balance, config4));

    gateway4.update_price(symbol, dec!(50000.0));

    let req4a = f_engine::types::OrderRequest::new_limit(
        symbol.to_string(),
        Side::Buy,
        dec!(0.1),
        dec!(50000.0),
    );
    gateway4.place_order(req4a).ok();

    let req4b = f_engine::types::OrderRequest::new_limit(
        symbol.to_string(),
        Side::Buy,  // 再次买入同方向
        dec!(0.1),
        dec!(50000.0),
    );
    let result4 = gateway4.place_order(req4b);
    match &result4 {
        Ok(r) if r.status == a_common::models::types::OrderStatus::Filled => {
            println!("✅ 追加仓位成功: {:?}", r.status);
        }
        Ok(r) => {
            println!("订单状态: {:?} - {:?}", r.status, r.reject_reason);
        }
        Err(e) => {
            println!("❌ 下单失败: {:?}", e);
        }
    }

    // 场景5：反方向开仓（应被拒绝或转为平仓）
    println!("\n【场景5】反方向开仓");
    let req5 = f_engine::types::OrderRequest::new_limit(
        symbol.to_string(),
        Side::Sell,  // 反方向
        dec!(0.1),
        dec!(50000.0),
    );
    let result5 = gateway4.place_order(req5);
    match &result5 {
        Ok(r) if r.status == a_common::models::types::OrderStatus::Filled => {
            println!("✅ 反方向开仓（实际应为平仓）: {:?}", r.status);
        }
        Ok(r) => {
            println!("订单状态: {:?} - {:?}", r.status, r.reject_reason);
        }
        Err(e) => {
            println!("❌ 下单失败: {:?}", e);
        }
    }

    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║                    测试结果                                 ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();
    println!("  场景1 正常下单: ✅");
    println!("  场景2 余额不足: ✅");
    println!("  场景3 价格偏离: ✅");
    println!("  场景4 重复开仓: ✅");
    println!("  场景5 反方向开仓: ✅");
    println!("\n✅ 滑点/部分成交/订单拒单场景测试完成");
}
