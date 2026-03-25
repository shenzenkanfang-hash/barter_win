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
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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

        let api = BinanceApiGateway::new_futures();
        let mut config = KlineFetcherConfig::new(api, &args.symbol, KlineInterval::Minute1);
        config.start_time = Some(start_ms);
        config.end_time = Some(end_ms);
        config.limit = args.limit;

        let fetcher = ApiKlineFetcher::new(config);
        info!("正在拉取 K线...");

        let klines = fetcher.fetch_all().await
            .map_err(|e| format!("拉取 K线失败: {}", e))?;

        if klines.is_empty() {
            error!("未获取到 K线数据");
            return Ok(());
        }

        info!("获取 K线: {} 条", klines.len());

        internal_klines = klines
            .into_iter()
            .map(|k| {
                b_data_source::KLine {
                    symbol: args.symbol.clone(),
                    period: b_data_source::Period::Minute(1),
                    open: k.open,
                    high: k.high,
                    low: k.low,
                    close: k.close,
                    volume: k.volume,
                    timestamp: k.open_time,
                }
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

            let api = BinanceApiGateway::new_futures();
            let mut config = KlineFetcherConfig::new(api, &args.symbol, KlineInterval::Minute1);
            config.start_time = Some(start_ms);
            config.end_time = Some(end_ms);
            config.limit = args.limit;

            let fetcher = ApiKlineFetcher::new(config);

            let klines = fetcher.fetch_all().await
                .map_err(|e| format!("拉取 K线失败: {}", e))?;

            if klines.is_empty() {
                error!("未获取到 K线数据");
                return Ok(());
            }

            info!("获取 K线: {} 条", klines.len());

            // 转换为内部 KLine
            internal_klines = klines
                .into_iter()
                .map(|k| {
                    b_data_source::KLine {
                        symbol: args.symbol.clone(),
                        period: b_data_source::Period::Minute(1),
                        open: k.open,
                        high: k.high,
                        low: k.low,
                        close: k.close,
                        volume: k.volume,
                        timestamp: k.open_time,
                    }
                })
                .collect();

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
