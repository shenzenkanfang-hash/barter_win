//! 示例：使用 TickGenerator 生成 Tick 流
//!
//! 运行: cargo run -p h_sandbox --example demo_tick

use std::sync::Arc;
use chrono::Utc;
use rust_decimal_macros::dec;

use b_data_source::{DataFeeder, KLine, Period};
use h_sandbox::{TickGenerator, TickDriver};

fn main() {
    println!("=== TickGenerator 演示 ===\n");

    // 创建模拟 K线数据
    let klines = vec![
        KLine {
            symbol: "POWERUSDT".to_string(),
            period: Period::Minute(1),
            open: dec!(5.0),
            high: dec!(5.2),
            low: dec!(4.9),
            close: dec!(5.1),
            volume: dec!(1000),
            timestamp: Utc::now(),
        },
        KLine {
            symbol: "POWERUSDT".to_string(),
            period: Period::Minute(1),
            open: dec!(5.1),
            high: dec!(5.3),
            low: dec!(5.0),
            close: dec!(5.25),
            volume: dec!(1200),
            timestamp: Utc::now(),
        },
        KLine {
            symbol: "POWERUSDT".to_string(),
            period: Period::Minute(1),
            open: dec!(5.25),
            high: dec!(5.4),
            low: dec!(5.1),
            close: dec!(5.15),
            volume: dec!(800),
            timestamp: Utc::now(),
        },
    ];

    println!("K线数据:");
    for (i, k) in klines.iter().enumerate() {
        println!("  [{:02}] {} O:{:.4} H:{:.4} L:{:.4} C:{:.4} V:{:.0}",
            i, k.timestamp.format("%H:%M:%S"), k.open, k.high, k.low, k.close, k.volume);
    }

    // 创建 TickGenerator
    let symbol = "POWERUSDT".to_string();
    let generator = TickGenerator::from_klines(symbol, klines);
    
    println!("\n预计生成 {} ticks (3 K线 * 60 ticks)", generator.total_klines() * 60);

    // 创建 DataFeeder
    let data_feeder = Arc::new(DataFeeder::new());

    // 创建 TickDriver
    let driver = TickDriver::new(generator, data_feeder.clone(), 1);

    println!("\n开始生成 Tick...\n");

    // 生成一些 tick 用于演示
    let mut tick_count = 0u64;
    let max_ticks = 185; // 3 K线 + 5 ticks

    loop {
        if tick_count >= max_ticks {
            break;
        }

        let tick = {
            let generator = driver.generator();
            let mut tick_gen = generator.write();
            if tick_gen.is_exhausted() {
                break;
            }
            tick_gen.next_tick()
        };

        match tick {
            Some(t) => {
                tick_count += 1;
                
                // 每60个tick（K线切换）或每20个tick打印一次
                let kline_idx = (tick_count - 1) / 60;
                let tick_in_kline = (tick_count - 1) % 60;
                
                if tick_in_kline < 5 || tick_in_kline == 59 || tick_count <= 3 {
                    let trend = if t.price >= t.open { "↑" } else { "↓" };
                    println!("[Tick {:04}] K{:1}:{:02} {} @ {:.4} | H:{:.4} L:{:.4} {}",
                        tick_count, kline_idx, tick_in_kline, trend,
                        t.price, t.high, t.low,
                        t.timestamp.format("%H:%M:%S%.3f"));
                }
            }
            None => break,
        }
    }

    let (sent, total) = driver.progress();
    println!("\n完成: 生成了 {} / {} ticks", sent, total);
    println!("\n=== 演示结束 ===");
}
