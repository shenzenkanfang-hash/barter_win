# Binance WS K线格式集成方案 (API 直连版)

## 1. 目标

- 删除废弃 CSV/KlineLoader 代码
- 新增 `ApiKlineFetcher` 币安 API 直连拉取历史 K线
- K线直接流式注入 `StreamTickGenerator`
- 输出 Binance 标准 WS K线 JSON
- 全系统保持：模拟价格流 + 仅拦截账户/持仓/下单 3 接口

---

## 2. 架构

```
币安公共 API (K线接口)
       ↓
ApiKlineFetcher (流式分页拉取，1000条/次)
       ↓
StreamTickGenerator (60 ticks/K线)
       ↓
TickToWsConverter (→ BinanceKlineMsg)
       ↓
┌──────────────────────────────────┐
│  1. JSON 输出 (控制台/文件)      │
│  2. DataFeeder.push_tick()       │
│  3. ShadowBinanceGateway (拦截)  │
└──────────────────────────────────┘
```

---

## 3. 文件变更

### 3.1 删除

| 文件 | 原因 |
|------|------|
| `crates/h_sandbox/src/historical_replay/kline_loader.rs` | 废弃，CSV方案弃用 |
| `examples/historical_replay.rs` | 废弃 |

### 3.2 新增

| 文件 | 说明 |
|------|------|
| `crates/a_common/src/api/kline_fetcher.rs` | K线拉取 API |
| `crates/h_sandbox/examples/kline_replay.rs` | 新的回放 example |

### 3.3 修改

| 文件 | 变更 |
|------|------|
| `crates/a_common/src/api/binance_api.rs` | 添加 K线拉取方法 |
| `crates/a_common/src/api/mod.rs` | 导出 ApiKlineFetcher |
| `crates/h_sandbox/src/historical_replay/mod.rs` | 删除 KlineLoader 导出 |
| `crates/h_sandbox/src/lib.rs` | 删除 KlineLoader 导出 |

---

## 4. ApiKlineFetcher 设计

### 4.1 币安 K线 API

```
GET /api/v3/klines
参数: symbol, interval, startTime, endTime, limit
返回: [["openTime","open","high","low","close","volume","closeTime","quoteVolume","numTrades","..."], ...]
```

### 4.2 Rust 实现

**位置**: `crates/a_common/src/api/kline_fetcher.rs`

```rust
//! Binance K线拉取器
//!
//! 币安公共 API 直连拉取历史 K线，流式分页，无本地存储。

use crate::api::BinanceApiGateway;
use crate::claint::error::EngineError;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal::prelude::FromPrimitive;
use serde::Deserialize;

/// K线数据（简化版）
#[derive(Debug, Clone)]
pub struct ApiKline {
    pub open_time: DateTime<Utc>,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
    pub close_time: DateTime<Utc>,
    pub quote_volume: Decimal,
    pub num_trades: u64,
}

/// K线拉取器配置
#[derive(Debug, Clone)]
pub struct KlineFetcherConfig {
    /// 币安 API 网关
    pub api: BinanceApiGateway,
    /// 交易对
    pub symbol: String,
    /// K线周期
    pub interval: KlineInterval,
    /// 起始时间
    pub start_time: Option<i64>,
    /// 结束时间
    pub end_time: Option<i64>,
    /// 每页数量 (默认 1000, 最大 1000)
    pub limit: u16,
}

impl KlineFetcherConfig {
    pub fn new(api: BinanceApiGateway, symbol: &str, interval: KlineInterval) -> Self {
        Self {
            api,
            symbol: symbol.to_uppercase(),
            interval,
            start_time: None,
            end_time: None,
            limit: 1000,
        }
    }
}

/// K线周期
#[derive(Debug, Clone, Copy)]
pub enum KlineInterval {
    Minute1,
    Minute5,
    Minute15,
    Hour1,
    Day1,
}

impl KlineInterval {
    pub fn as_str(&self) -> &'static str {
        match self {
            KlineInterval::Minute1 => "1m",
            KlineInterval::Minute5 => "5m",
            KlineInterval::Minute15 => "15m",
            KlineInterval::Hour1 => "1h",
            KlineInterval::Day1 => "1d",
        }
    }
}

/// K线拉取器
pub struct ApiKlineFetcher {
    config: KlineFetcherConfig,
}

impl ApiKlineFetcher {
    /// 创建拉取器
    pub fn new(config: KlineFetcherConfig) -> Self {
        Self { config }
    }

    /// 拉取所有 K线（流式）
    pub async fn fetch_all(&self) -> Result<Vec<ApiKline>, EngineError> {
        let mut all_klines = Vec::new();
        let mut current_start = self.config.start_time;

        loop {
            let batch = self.fetch_batch(current_start, self.config.end_time).await?;
            if batch.is_empty() {
                break;
            }

            all_klines.extend(batch);

            // 下一页起始时间 = 最后一条的 close_time + 1
            if let Some(last) = all_klines.last() {
                current_start = Some(last.close_time.timestamp_millis() + 1);
            }

            // 已达到限制或结束时间
            if all_klines.len() >= self.config.limit as usize
                || current_start >= self.config.end_time {
                break;
            }
        }

        Ok(all_klines)
    }

    /// 拉取单批 K线
    async fn fetch_batch(
        &self,
        start_time: Option<i64>,
        end_time: Option<i64>,
    ) -> Result<Vec<ApiKline>, EngineError> {
        let url = format!("{}/api/v3/klines", self.config.api.market_api_base());

        let mut req = self.config.api.client.get(&url);

        req = req.query(&[
            ("symbol", self.config.symbol.as_str()),
            ("interval", self.config.interval.as_str()),
            ("limit", &self.config.limit.to_string()),
        ]);

        if let Some(start) = start_time {
            req = req.query(&[("startTime", &start.to_string())]);
        }
        if let Some(end) = end_time {
            req = req.query(&[("endTime", &end.to_string())]);
        }

        // 限速
        self.config.api.rate_limiter().lock().acquire().await;

        let resp = req.send().await
            .map_err(|e| EngineError::Other(format!("HTTP 请求失败: {}", e)))?;

        if !resp.status().is_success() {
            return Err(EngineError::Other(format!("API 返回错误: {}", resp.status())));
        }

        let raw_klines: Vec<Vec<serde_json::Value>> = resp.json().await
            .map_err(|e| EngineError::Other(format!("解析失败: {}", e)))?;

        let klines = raw_klines.into_iter()
            .filter_map(|arr| self.parse_kline(&arr))
            .collect();

        Ok(klines)
    }

    /// 解析单条 K线
    fn parse_kline(&self, arr: &[serde_json::Value]) -> Option<ApiKline> {
        let get_str = |i: usize| arr.get(i)?.as_str().ok()?;
        let get_f64 = |i: usize| arr.get(i)?.as_str()?.parse::<f64>().ok()?;

        Some(ApiKline {
            open_time: DateTime::from_timestamp_millis(get_str(0).parse().ok()?)?,
            open: Decimal::from_f64_retain(get_f64(1))?,
            high: Decimal::from_f64_retain(get_f64(2))?,
            low: Decimal::from_f64_retain(get_f64(3))?,
            close: Decimal::from_f64_retain(get_f64(4))?,
            volume: Decimal::from_f64_retain(get_f64(5))?,
            close_time: DateTime::from_timestamp_millis(get_str(6).parse().ok()?)?,
            quote_volume: Decimal::from_f64_retain(get_f64(7)).unwrap_or(Decimal::ZERO),
            num_trades: arr.get(8)?.as_str()?.parse().unwrap_or(0),
        })
    }
}

/// 实现 IntoIterator 方便直接流入 StreamTickGenerator
impl IntoIterator for ApiKline {
    type Item = b_data_source::KLine;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        vec![self.into()].into_iter()
    }
}

impl From<ApiKline> for b_data_source::KLine {
    fn from(k: ApiKline) -> Self {
        b_data_source::KLine {
            symbol: "".to_string(), // 调用方设置
            period: b_data_source::Period::Minute(1),
            open: k.open,
            high: k.high,
            low: k.low,
            close: k.close,
            volume: k.volume,
            timestamp: k.open_time,
        }
    }
}
```

---

## 5. 更新现有文件

### 5.1 binance_api.rs 新增方法

```rust
/// 从 API 获取历史 K线
pub async fn fetch_klines(
    &self,
    symbol: &str,
    interval: &str,
    start_time: Option<i64>,
    end_time: Option<i64>,
    limit: u16,
) -> Result<Vec<Vec<serde_json::Value>>, EngineError> {
    self.rate_limiter.lock().acquire().await;

    let url = format!("{}/api/v3/klines", self.market_api_base);
    let mut req = self.client.get(&url);

    req = req.query(&[
        ("symbol", symbol),
        ("interval", interval),
        ("limit", &limit.to_string()),
    ]);

    if let Some(start) = start_time {
        req = req.query(&[("startTime", &start.to_string())]);
    }
    if let Some(end) = end_time {
        req = req.query(&[("endTime", &end.to_string())]);
    }

    let resp = req.send().await
        .map_err(|e| EngineError::Other(format!("HTTP 请求失败: {}", e)))?;

    if !resp.status().is_success() {
        return Err(EngineError::Other(format!("K线 API 错误: {}", resp.status())));
    }

    resp.json().await
        .map_err(|e| EngineError::Other(format!("解析 K线失败: {}", e)))
}
```

### 5.2 historical_replay/mod.rs

```rust
// 删除 kline_loader
pub mod kline_loader; // 保留但标注废弃
pub mod tick_generator;
pub mod noise;
pub mod memory_injector;
pub mod replay_controller;
pub mod tick_to_ws;

// 导出
pub use tick_generator::{StreamTickGenerator, SimulatedTick};
pub use memory_injector::{MemoryInjector, MemoryInjectorConfig, SharedMarketData};
pub use replay_controller::{ReplayController, ReplayConfig, ReplayState, ReplayStats, ReplayError};
pub use tick_to_ws::TickToWsConverter;

// 标记废弃但保留
#[deprecated(since = "1.2", note = "使用 ApiKlineFetcher 替代")]
pub use kline_loader::{KlineLoader, KlineLoadError, ParquetInfo};
```

### 5.3 kline_replay.rs (新 example)

```rust
//! K线回放 Example
//!
//! 1. ApiKlineFetcher 直连拉取币安历史 K线
//! 2. 流式生成 Tick
//! 3. 转换为 BinanceKlineMsg (WS JSON)
//!
//! cargo run --example kline_replay -- --symbol BTCUSDT --start "2024-01-01" --end "2024-01-02"

use chrono::{DateTime, Duration, NaiveDateTime, TimeZone, Utc};
use clap::Parser;
use h_sandbox::api::kline_fetcher::{ApiKlineFetcher, KlineFetcherConfig, KlineInterval};
use h_sandbox::historical_replay::{StreamTickGenerator, TickToWsConverter};
use h_sandbox::api::BinanceApiGateway;

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, default_value = "BTCUSDT")]
    symbol: String,
    #[arg(long)]
    start: String,
    #[arg(long)]
    end: String,
    #[arg(long, default_value = "10.0")]
    speed: f64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // 解析时间
    let start_dt = NaiveDateTime::parse_from_str(&args.start, "%Y-%m-%d")?;
    let end_dt = NaiveDateTime::parse_from_str(&args.end, "%Y-%m-%d")?;
    let start_ms = Utc.from_utc_datetime(&start_dt).timestamp_millis();
    let end_ms = Utc.from_utc_datetime(&end_dt).timestamp_millis();

    println!("拉取 K线: {} {} → {}", args.symbol, args.start, args.end);

    // 创建 API
    let api = BinanceApiGateway::new_futures();
    let config = KlineFetcherConfig {
        api,
        symbol: args.symbol.clone(),
        interval: KlineInterval::Minute1,
        start_time: Some(start_ms),
        end_time: Some(end_ms),
        limit: 1000,
    };

    // 拉取 K线
    let fetcher = ApiKlineFetcher::new(config);
    let klines = fetcher.fetch_all().await?;
    println!("获取 K线: {} 条", klines.len());

    // 转换为内部 KLine
    let internal_klines: Vec<_> = klines.into_iter()
        .map(|k| {
            let mut kline: b_data_source::KLine = k.into();
            kline.symbol = args.symbol.clone();
            kline
        })
        .collect();

    // 创建生成器
    let generator = StreamTickGenerator::from_klines(args.symbol.clone(), internal_klines);
    let converter = TickToWsConverter::new(args.symbol.clone(), "1m".to_string());

    // 流式输出 WS JSON
    for (idx, tick) in generator.enumerate() {
        let tick_idx = (idx % 60) as u8;
        let is_last = tick_idx == 59;

        let ws_msg = converter.convert(&tick, tick_idx, is_last);
        let json = serde_json::to_string(&ws_msg)?;
        println!("{}", json);

        // 限速
        let interval_ms = (1000.0 / args.speed) as u64;
        std::thread::sleep(std::time::Duration::from_millis(interval_ms));
    }

    Ok(())
}
```

---

## 6. 验证计划

| 验证项 | 命令 |
|--------|------|
| 编译通过 | `cargo check --all` |
| API 拉取 | 拉取 BTCUSDT 1m K线 |
| WS JSON 输出 | 对比 Binance WS 格式 |
| 全链路 | K线 → Tick → WS JSON → DataFeeder |
