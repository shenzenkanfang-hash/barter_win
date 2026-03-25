//! 性能测试示例
//!
//! 运行: cargo run -p h_sandbox --example perf_test -- --fast
//!
//! 测试系统处理性能，不改动原有引擎代码
//! 数据源: CSV replay (b_data_source::replay_source::ReplaySource)

use std::sync::Arc;
use std::time::Duration;

use tokio::time::timeout;
use h_sandbox::perf_test::{PerfTestConfig, TickDriver, EngineDriver, PerformanceTracker, Reporter};

#[tokio::main]
async fn main() {
    println!("=== 性能测试 ===\n");

    // 解析命令行参数
    let args: Vec<String> = std::env::args().collect();
    let mut config = PerfTestConfig::default();

    for (i, arg) in args.iter().enumerate() {
        match arg.as_str() {
            "--path" => {
                if i + 1 < args.len() {
                    config.csv_path = args[i + 1].clone();
                }
            }
            "--symbol" => {
                if i + 1 < args.len() {
                    config.symbol = args[i + 1].clone();
                }
            }
            "--interval" => {
                if i + 1 < args.len() {
                    config.tick_interval_ms = args[i + 1].parse().unwrap_or(16);
                }
            }
            "--fast" => {
                config.fast_mode = true;
            }
            "--help" => {
                println!("用法: perf_test [选项]");
                println!("选项:");
                println!("  --path <路径>     CSV 数据文件路径");
                println!("  --symbol <品种>   测试品种 (默认: BTCUSDT)");
                println!("  --interval <ms>   tick 间隔 (默认: 16ms)");
                println!("  --fast            快速模式 (不等待间隔)");
                println!("  --help            显示帮助");
                return;
            }
            _ => {}
        }
    }

    // 检查必需参数（fast 模式可以使用模拟数据）
    if config.csv_path.is_empty() && !config.fast_mode {
        eprintln!("错误: 请指定 --path 参数");
        eprintln!("示例: cargo run --example perf_test -- --path \"data\\kline.csv\"");
        eprintln!("或者使用 --fast 模式（模拟数据）:");
        eprintln!("示例: cargo run --example perf_test -- --fast");
        return;
    }

    println!("配置:");
    println!("  数据源: {}", config.csv_path);
    println!("  品种: {}", config.symbol);
    println!("  tick 间隔: {}ms", config.tick_interval_ms);
    println!("  模式: {}", if config.fast_mode { "快速" } else { "实时" });
    println!();

    // 创建 channel
    let (tx, rx) = tokio::sync::mpsc::channel(1000);

    // 创建追踪器
    let tracker = Arc::new(PerformanceTracker::new());

    // 创建 TickDriver
    let tick_driver = if config.fast_mode {
        // 快速模式使用模拟数据
        match TickDriver::new(config.clone(), tx) {
            Ok(driver) => {
                println!("✅ 使用模拟数据，共 {} ticks", driver.total_ticks());
                driver
            }
            Err(e) => {
                eprintln!("❌ 创建 TickDriver 失败: {}", e);
                return;
            }
        }
    } else {
        // 实时模式从 CSV 加载
        match TickDriver::from_csv(config.clone(), tx) {
            Ok(driver) => {
                println!("✅ 成功加载数据，共 {} ticks", driver.total_ticks());
                driver
            }
            Err(e) => {
                eprintln!("❌ 创建 TickDriver 失败: {}", e);
                return;
            }
        }
    };

    // 创建 EngineDriver
    let engine_driver = EngineDriver::new(
        Default::default(),
        tracker.clone(),
        tick_driver.total_ticks(),
    );

    // 并行运行
    println!("\n开始测试...\n");

    let start = std::time::Instant::now();

    // 运行 tick driver
    let tick_handle = tokio::spawn(async move {
        if config.fast_mode {
            tick_driver.run_fast().await;
        } else {
            tick_driver.run_realtime().await;
        }
    });

    // 运行 engine driver
    let engine_handle = tokio::spawn(async move {
        engine_driver.run(rx).await;
    });

    // 等待完成（带超时）
    let timeout_secs = if config.fast_mode { 60 } else { 300 };
    let result = timeout(
        Duration::from_secs(timeout_secs),
        async {
            tokio::join!(tick_handle, engine_handle);
        }
    ).await;

    match result {
        Ok(_) => {
            let elapsed = start.elapsed();
            println!("\n测试完成，耗时: {:.2}s", elapsed.as_secs_f64());
        }
        Err(_) => {
            println!("\n⚠️  测试超时 ({}s)", timeout_secs);
        }
    }

    // 生成报告
    let stats = tracker.stats();
    let result = Reporter::generate(&config, stats);
    Reporter::print(&result);
}
