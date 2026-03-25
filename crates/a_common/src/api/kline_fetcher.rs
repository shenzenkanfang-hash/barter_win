//! Binance K线拉取器
//!
//! 币安公共 API 直连拉取历史 K线，流式分页，无本地存储。

use crate::api::BinanceApiGateway;
use crate::claint::error::EngineError;
use chrono::{DateTime, TimeZone, Utc};
use rust_decimal::Decimal;

/// K线数据（简化版）
#[derive(Debug, Clone)]
pub struct ApiKline {
    /// 开盘时间
    pub open_time: DateTime<Utc>,
    /// 开盘价
    pub open: Decimal,
    /// 最高价
    pub high: Decimal,
    /// 最低价
    pub low: Decimal,
    /// 收盘价
    pub close: Decimal,
    /// 成交量
    pub volume: Decimal,
    /// 收盘时间
    pub close_time: DateTime<Utc>,
    /// 成交额
    pub quote_volume: Decimal,
    /// 成交笔数
    pub num_trades: u64,
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
    /// 转换为币安 API 格式
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

/// K线拉取器配置
#[derive(Debug, Clone)]
pub struct KlineFetcherConfig {
    /// 币安 API 网关
    pub api: BinanceApiGateway,
    /// 交易对
    pub symbol: String,
    /// K线周期
    pub interval: KlineInterval,
    /// 起始时间（毫秒）
    pub start_time: Option<i64>,
    /// 结束时间（毫秒）
    pub end_time: Option<i64>,
    /// 每页数量 (默认 1000, 最大 1000)
    pub limit: u16,
}

impl KlineFetcherConfig {
    /// 创建配置
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

/// K线拉取器
pub struct ApiKlineFetcher {
    config: KlineFetcherConfig,
}

impl ApiKlineFetcher {
    /// 创建拉取器
    pub fn new(config: KlineFetcherConfig) -> Self {
        Self { config }
    }

    /// 拉取所有 K线（流式分页）
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
                || current_start >= self.config.end_time
            {
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
        // 使用 API 网关的 fetch_klines 方法
        let raw_klines = self
            .config
            .api
            .fetch_klines(
                &self.config.symbol,
                self.config.interval.as_str(),
                start_time,
                end_time,
                self.config.limit,
            )
            .await?;

        let klines = raw_klines
            .into_iter()
            .filter_map(|arr| self.parse_kline(&arr))
            .collect();

        Ok(klines)
    }

    /// 解析单条 K线
    fn parse_kline(&self, arr: &[serde_json::Value]) -> Option<ApiKline> {
        let open_time_str = arr.get(0)?.as_str()?;
        let close_time_str = arr.get(6)?.as_str()?;

        let open_time_ms: i64 = open_time_str.parse().ok()?;
        let close_time_ms: i64 = close_time_str.parse().ok()?;

        let parse_decimal = |idx: usize| -> Option<Decimal> {
            let s = arr.get(idx)?.as_str()?;
            let f: f64 = s.parse().ok()?;
            Decimal::from_f64_retain(f)
        };

        let parse_u64 = |idx: usize| -> u64 {
            arr.get(idx).and_then(|v| v.as_str()).unwrap_or("0").parse().unwrap_or(0)
        };

        Some(ApiKline {
            open_time: Utc.timestamp_millis_opt(open_time_ms).single()?,
            open: parse_decimal(1)?,
            high: parse_decimal(2)?,
            low: parse_decimal(3)?,
            close: parse_decimal(4)?,
            volume: parse_decimal(5)?,
            close_time: Utc.timestamp_millis_opt(close_time_ms).single()?,
            quote_volume: parse_decimal(7).unwrap_or(Decimal::ZERO),
            num_trades: parse_u64(8),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interval_as_str() {
        assert_eq!(KlineInterval::Minute1.as_str(), "1m");
        assert_eq!(KlineInterval::Minute15.as_str(), "15m");
        assert_eq!(KlineInterval::Day1.as_str(), "1d");
    }

    #[test]
    fn test_kline_fetcher_config() {
        let api = BinanceApiGateway::new();
        let config = KlineFetcherConfig::new(api, "BTCUSDT", KlineInterval::Minute1);

        assert_eq!(config.symbol, "BTCUSDT");
        assert_eq!(config.limit, 1000);
    }
}
