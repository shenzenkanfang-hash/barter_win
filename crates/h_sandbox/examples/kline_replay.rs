//! K线回放 Example
//!
//! 演示如何使用 API 直连回放系统：
//! 1. ApiKlineFetcher 直连拉取币安历史 K线
//! 2. 流式生成仿真 Tick
//! 3. 转换为 BinanceKlineMsg (WS JSON)
//!
//! ## 使用方法
//!
//! ```bash
//! cargo run --example kline_replay -- \
//!     --symbol BTCUSDT \
//!     --start "2024-01-01" \
//!     --end "2024-01-02" \
//!     --speed 10.0
//! ```
//!
//! ## 参数说明
//!
//! - --symbol: 交易对 (默认: BTCUSDT)
//! - --start: 起始日期 (格式: YYYY-MM-DD)
//! - --end: 结束日期 (格式: YYYY-MM-DD)
//! - --speed: 回放速度倍数 (默认: 10.0)
//! - --limit: 最大 K线数量，默认 1000
//! - --no_cache: 跳过缓存，直接从 API 拉取 (默认: false)
//! - --cache_dir: 缓存目录路径 (默认: D:/sandbox_cache)

use std::path::PathBuf;

use chrono::{NaiveDateTime, TimeZone, Utc};
use clap::Parser;
use tracing::{info, error, Level};
use tracing_subscriber::FmtSubscriber;

use a_common::api::{ApiKlineFetcher, KlineFetcherConfig, KlineInterval, BinanceApiGateway};
use h_sandbox::historical_replay::{StreamTickGenerator, TickToWsConverter, ShardCache, ShardReader, ShardReaderChain};

/// 命令行参数
#[derive(Parser, Debug)]
#[command(name = "kline_replay")]
#[command(about = "币安 API 直连 K线回放 + WS K线格式输出", long_about = None)]
struct Args {
    /// 交易对
    #[arg(long, default_value = "BTCUSDT")]
    symbol: String,

    /// 起始日期 (格式: YYYY-MM-DD)
    #[arg(long)]
    start: String,

    /// 结束日期 (格式: YYYY-MM-DD)
    #[arg(long)]
    end: String,

    /// 回放速度（1.0=实时，10.0=10倍速）
    #[arg(long, default_value = "10.0")]
    speed: f64,

    /// 最大 K线数量 (默认: 1000)
    #[arg(long, default_value = "1000")]
    limit: u16,

    /// 跳过缓存，直接从 API 拉取
    #[arg(long, default_value = "false")]
    no_cache: bool,

    /// 缓存目录路径 (默认: D:/sandbox_cache)
    #[arg(long)]
    cache_dir: Option<PathBuf>,
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    // 解析参数
    let args = Args::parse();

    // 初始化日志
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(true)
        .init();

    info!("=== K线回放启动 ===");
    info!("交易对: {}", args.symbol);
    info!("时间范围: {} → {}", args.start, args.end);
    info!("速度: {}x", args.speed);

    // 解析时间
    let start_dt = NaiveDateTime::parse_from_str(&format!("{} 00:00:00", args.start), "%Y-%m-%d %H:%M:%S")
        .map_err(|e| format!("日期解析失败: {}", e))?;
    let end_dt = NaiveDateTime::parse_from_str(&format!("{} 00:00:00", args.end), "%Y-%m-%d %H:%M:%S")
        .map_err(|e| format!("日期解析失败: {}", e))?;

    let start_ms = Utc.from_utc_datetime(&start_dt).timestamp_millis();
    let end_ms = Utc.from_utc_datetime(&end_dt).timestamp_millis();

    info!("时间戳: {} → {}", start_ms, end_ms);

    // 默认缓存目录
    let cache_root = args.cache_dir.unwrap_or_else(|| PathBuf::from("D:/sandbox_cache"));
    let cache = ShardCache::new(cache_root);

    // 内部 K线迭代器
    let internal_klines: Vec<b_data_source::KLine>;

    if args.no_cache {
        // 直接从 API 拉取（跳过缓存）
        info!("no_cache=true，跳过缓存，直接拉取 API");

        // 使用 curl 命令拉取（更稳定）
        // 币安合约K线API路径是 /fapi/v1/klines
        let url = format!(
            "https://fapi.binance.com/fapi/v1/klines?symbol={}&interval=1m&limit={}&startTime={}&endTime={}",
            args.symbol.to_uppercase(),
            args.limit,
            start_ms,
            end_ms
        );

        info!("请求 URL: {}", url);

        let output = std::process::Command::new("curl")
            .args(["-s", "-X", "GET", &url])
            .output()
            .map_err(|e| format!("curl 执行失败: {}", e))?;

        // Windows curl 可能输出到 stderr
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        
        info!("curl stdout: {} bytes", stdout.len());
        info!("curl stderr: {} bytes", stderr.len());

        let json_str = if !stdout.is_empty() { stdout.clone() } else { stderr.clone() };
        
        if json_str.is_empty() {
            return Err("API 返回空数据".into());
        }

        let raw_klines: Vec<Vec<serde_json::Value>> = serde_json::from_str(&json_str)
            .map_err(|e| format!("JSON 解析失败: {} | Body: {}", e, json_str))?;

        if raw_klines.is_empty() {
            error!("未获取到 K线数据");
            return Ok(());
        }

        info!("获取 K线: {} 条", raw_klines.len());

        internal_klines = raw_klines
            .into_iter()
            .filter_map(|arr| {
                let open_time_str = arr.get(0)?.as_str()?;
                let close_time_str = arr.get(6)?.as_str()?;
                let open_time_ms: i64 = open_time_str.parse().ok()?;
                let close_time_ms: i64 = close_time_str.parse().ok()?;

                let parse_decimal = |idx: usize| -> Option<rust_decimal::Decimal> {
                    let s = arr.get(idx)?.as_str()?;
                    let f: f64 = s.parse().ok()?;
                    rust_decimal::Decimal::from_f64_retain(f)
                };

                Some(b_data_source::KLine {
                    symbol: args.symbol.clone(),
                    period: b_data_source::Period::Minute(1),
                    open: parse_decimal(1)?,
                    high: parse_decimal(2)?,
                    low: parse_decimal(3)?,
                    close: parse_decimal(4)?,
                    volume: parse_decimal(5)?,
                    timestamp: chrono::Utc.timestamp_millis_opt(open_time_ms).single()?,
                })
            })
            .collect();
    } else {
        // 尝试从本地缓存分片读取
        let shards = cache.find_shards(&args.symbol, "1m", start_ms, end_ms);
        if !shards.is_empty() && ShardCache::shards_cover_range(&shards, start_ms, end_ms) {
            // 本地缓存完整，直接使用
            info!("使用本地缓存: {} 个分片", shards.len());

            let readers: Result<Vec<_>, _> = shards.iter()
                .map(|s| ShardReader::new(&s.path))
                .collect();
            let chain = ShardReaderChain::new(readers?);

            // 将 ShardReaderChain 转换为内部 KLine
            internal_klines = chain.filter_map(|r| r.ok()).collect();
            info!("从缓存读取 K线: {} 条", internal_klines.len());
        } else {
            // 本地缓存未命中或不全，拉取 API 并写入缓存
            info!("本地缓存未命中，拉取 API...");

            // 使用 curl 命令拉取（更稳定）
            let url = format!(
                "https://fapi.binance.com/fapi/v1/klines?symbol={}&interval=1m&limit={}&startTime={}&endTime={}",
                args.symbol.to_uppercase(),
                args.limit,
                start_ms,
                end_ms
            );

            info!("请求 URL: {}", url);

            let output = std::process::Command::new("curl")
                .args(["-s", "-X", "GET", &url])
                .output()
                .map_err(|e| format!("curl 执行失败: {}", e))?;

            // Windows curl 可能输出到 stderr
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            info!("curl stdout: {} bytes, stderr: {} bytes", stdout.len(), stderr.len());

            let json_str = if !stdout.is_empty() { stdout } else { stderr };

            let raw_klines: Vec<Vec<serde_json::Value>> = serde_json::from_str(&json_str)
                .map_err(|e| format!("JSON 解析失败: {} | Body: {}", e, json_str))?;

            // 调试：打印第一条数据
            if let Some(first) = raw_klines.first() {
                info!("第一条 K线原始数据: {:?}", first);
            }

            if raw_klines.is_empty() {
                error!("未获取到 K线数据");
                return Ok(());
            }

            info!("获取 K线: {} 条", raw_klines.len());

            // 转换为内部 KLine
            let mut parsed_count = 0;
            internal_klines = raw_klines
                .into_iter()
                .filter_map(|arr| {
                    // 索引 0 是 Number (open_time)
                    let open_time_ms = arr.get(0)?.as_i64()?;

                    // 索引 1-5 是 String (价格和成交量)
                    let parse_decimal = |idx: usize| -> Option<rust_decimal::Decimal> {
                        let s = arr.get(idx)?.as_str()?;
                        let f: f64 = s.parse().ok()?;
                        rust_decimal::Decimal::from_f64_retain(f)
                    };

                    let timestamp = chrono::Utc.timestamp_millis_opt(open_time_ms).single()?;

                    parsed_count += 1;
                    Some(b_data_source::KLine {
                        symbol: args.symbol.clone(),
                        period: b_data_source::Period::Minute(1),
                        open: parse_decimal(1)?,
                        high: parse_decimal(2)?,
                        low: parse_decimal(3)?,
                        close: parse_decimal(4)?,
                        volume: parse_decimal(5)?,
                        timestamp,
                    })
                })
                .collect();
            
            info!("解析 K线: 成功 {} 条", parsed_count);

            // 写入缓存
            if !internal_klines.is_empty() {
                let first_ts = internal_klines.first()
                    .map(|k| k.timestamp.timestamp_millis())
                    .unwrap_or(start_ms);
                let mut writer = cache.write_shard(&args.symbol, "1m", first_ts);
                for kline in &internal_klines {
                    if let Err(e) = writer.write(kline) {
                        info!("缓存写入失败（不影响回放）: {}", e);
                        break;
                    }
                }
                if let Ok(shard) = writer.finish() {
                    info!("缓存已写入: {:?}", shard.path);
                }
            }
        }
    }

    info!("准备回放的 K线数量: {}", internal_klines.len());

    // 创建生成器
    let generator = StreamTickGenerator::from_loader(args.symbol.clone(), internal_klines.into_iter());

    // 创建转换器
    let converter = TickToWsConverter::new(args.symbol.clone(), "1m".to_string());

    info!("开始回放...");

    // 流式输出 WS JSON
    let mut tick_count = 0u64;
    let tick_interval_ms = (1000.0 / args.speed) as u64;

    for (idx, tick) in generator.enumerate() {
        let tick_idx = (idx % 60) as u8;
        let is_last = tick.is_last_in_kline; // 由 StreamTickGenerator 自身判断

        // 转换为 WS 格式
        let ws_msg = converter.convert(&tick, tick_idx, is_last);
        let json = serde_json::to_string(&ws_msg)?;

        println!("{}", json);
        tick_count += 1;

        // 限速
        if args.speed > 0.0 {
            std::thread::sleep(std::time::Duration::from_millis(tick_interval_ms));
        }

        // 进度打印
        if tick_count % 600 == 0 {
            info!("已处理 {} ticks ({} klines)", tick_count, tick_count / 60);
        }
    }

    info!("=== 回放完成 ===");
    info!("总 ticks: {}", tick_count);
    info!("总 klines: {}", tick_count / 60);

    Ok(())
}
