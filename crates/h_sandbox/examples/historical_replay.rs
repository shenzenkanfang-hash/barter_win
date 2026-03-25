//! Historical Replay Example - 历史数据回放示例
//!
//! 演示如何使用流式回放系统：
//! 1. 加载 Parquet 历史 K线
//! 2. 流式生成仿真 Tick
//! 3. 注入内存驱动引擎
//!
//! ## 使用方法
//!
//! ```bash
//! cargo run --example historical_replay -- \
//!     --path data/BTCUSDT_1m.parquet \
//!     --symbol BTCUSDT \
//!     --speed 10.0
//! ```

use clap::Parser;
use std::sync::Arc;
use parking_lot::RwLock;
use tracing::{info, error, Level};
use tracing_subscriber::FmtSubscriber;

use h_sandbox::historical_replay::{
    KlineLoader, StreamTickGenerator, MemoryInjector, ReplayController, ReplayConfig,
    SharedMarketData,
};

/// 命令行参数
#[derive(Parser, Debug)]
#[command(name = "historical_replay")]
#[command(about = "历史数据回放示例", long_about = None)]
struct Args {
    /// Parquet 文件路径
    #[arg(long, default_value = "data/BTCUSDT_1m.parquet")]
    path: String,

    /// 交易对
    #[arg(long, default_value = "BTCUSDT")]
    symbol: String,

    /// 回放速度（1.0=实时，10.0=10倍速）
    #[arg(long, default_value = "1.0")]
    speed: f64,

    /// Tick 间隔（毫秒）
    #[arg(long, default_value = "16")]
    tick_interval_ms: u64,

    /// 详细日志
    #[arg(long, short)]
    verbose: bool,
}

#[tokio::main]
async fn main() {
    // 解析参数
    let args = Args::parse();

    // 初始化日志
    let level = if args.verbose { Level::DEBUG } else { Level::INFO };
    let subscriber = FmtSubscriber::builder()
        .with_max_level(level)
        .with_target(true)
        .init();

    info!("=== Historical Replay 启动 ===");
    info!("文件: {}", args.path);
    info!("交易对: {}", args.symbol);
    info!("速度: {}x", args.speed);

    // 检查文件是否存在
    if !std::path::Path::new(&args.path).exists() {
        error!("文件不存在: {}", args.path);
        info!("请先生成 Parquet 文件或指定正确的路径");
        info!("格式: timestamp, open, high, low, close, volume");
        std::process::exit(1);
    }

    // 加载 Parquet 获取信息
    match KlineLoader::new(&args.path) {
        Ok(loader) => {
            let info = loader.info();
            info!("Parquet 信息: {} rows, {} groups", info.num_rows, info.num_row_groups);
        }
        Err(e) => {
            error!("加载 Parquet 失败: {}", e);
            std::process::exit(1);
        }
    }

    // 创建共享内存
    let shared_data = Arc::new(RwLock::new(SharedMarketData::new()));

    // 创建配置
    let config = ReplayConfig {
        playback_speed: args.speed,
        tick_interval_ms: args.tick_interval_ms,
        verbose: args.verbose,
        warmup_seconds: 0,
    };

    // 创建控制器
    let mut controller = ReplayController::with_shared_data(config, shared_data.clone());

    info!("开始回放...");

    // 运行回放
    match controller.run(&args.path, &args.symbol) {
        Ok(()) => {
            let stats = controller.stats();
            info!("=== 回放完成 ===");
            info!("发送 Tick: {}", stats.ticks_sent);
            info!("完成 K线: {}", stats.klines_completed);
            info!("耗时: {:?}", stats.elapsed());
        }
        Err(e) => {
            error!("回放失败: {}", e);
            std::process::exit(1);
        }
    }

    // 输出最后几根 K线
    {
        let shared = shared_data.read();
        if let Some(ref kline) = shared.kline {
            info!("最后 K线: O={}, H={}, L={}, C={}, V={}",
                kline.open, kline.high, kline.low, kline.close, kline.volume);
        }
    }
}

/// 简单回放测试（不使用 tokio）
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn test_args_parsing() {
        let args = Args::parse_from(["test", "--path", "test.parquet", "--speed", "5.0"]);
        assert_eq!(args.path, "test.parquet");
        assert_eq!(args.speed, 5.0);
    }
}
