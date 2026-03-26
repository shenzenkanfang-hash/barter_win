//! History API Client - 历史数据 API 调用
//!
//! 实现从交易所API拉取K线数据，支持：
//! - 指数退避重试策略 + Jitter
//! - 多品种并发限制（Semaphore）
//! - 直接使用 HTTP Client（避免 parking_lot::Mutex 的 async 兼容性问题）
//!
//! # 核心设计
//! - 最大重试次数: 3次
//! - 初始退避: 100ms
//! - 最大退避: 5秒
//! - Jitter: 0.5 + random(0, 0.5)
//! - 并发限制: 5个并发请求

use std::sync::Arc;
use std::time::Duration;

use rand::Rng;
use reqwest::Client;
use rust_decimal::Decimal;
use tokio::sync::Semaphore;
use tracing::{info, warn};

use super::types::{HistoryError, KLine};

/// API调用配置
#[derive(Debug, Clone)]
pub struct ApiClientConfig {
    /// 最大重试次数
    pub max_retries: u8,
    /// 初始退避时间（毫秒）
    pub initial_backoff_ms: u64,
    /// 最大退避时间（毫秒）
    pub max_backoff_ms: u64,
    /// 最大并发请求数
    pub max_concurrent: usize,
    /// API Base URL
    pub api_base: String,
}

impl Default for ApiClientConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff_ms: 100,
            max_backoff_ms: 5000,
            max_concurrent: 5,
            api_base: "https://fapi.binance.com".to_string(),
        }
    }
}

/// 创建 HTTP 客户端（带代理支持）
fn create_http_client() -> Result<Client, HistoryError> {
    let proxy = std::env::var("HTTP_PROXY")
        .or_else(|_| std::env::var("http_proxy"))
        .ok();

    let mut builder = Client::builder()
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(15))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36");

    if let Some(proxy_url) = proxy {
        builder = builder.proxy(reqwest::Proxy::https(&proxy_url).unwrap_or_else(|_| {
            reqwest::Proxy::http(&proxy_url).expect("Invalid proxy")
        }));
    }

    builder.build()
        .map_err(|e| HistoryError::ApiRequestFailed(format!("HTTP client creation failed: {}", e)))
}

/// 历史数据 API 客户端
#[derive(Clone)]
pub struct HistoryApiClient {
    /// HTTP 客户端
    client: Client,
    /// 并发限制信号量
    semaphore: Arc<Semaphore>,
    /// 配置
    config: ApiClientConfig,
}

impl HistoryApiClient {
    /// 创建用于现货的客户端
    pub fn new_spot() -> Self {
        Self::with_config(ApiClientConfig {
            api_base: "https://api.binance.com".to_string(),
            ..Default::default()
        })
    }

    /// 创建用于合约的客户端
    pub fn new_futures() -> Self {
        Self::with_config(ApiClientConfig {
            api_base: "https://fapi.binance.com".to_string(),
            ..Default::default()
        })
    }

    /// 使用自定义配置创建
    pub fn with_config(config: ApiClientConfig) -> Self {
        let client = create_http_client()
            .expect("Failed to create HTTP client");
        Self {
            client,
            semaphore: Arc::new(Semaphore::new(config.max_concurrent)),
            config,
        }
    }

    /// 拉取历史K线（带重试+jitter）
    ///
    /// # 参数
    /// * `symbol` - 交易对，如 "BTCUSDT"
    /// * `period` - 周期，如 "1m", "1d"
    /// * `start_time` - 起始时间（毫秒）
    /// * `end_time` - 结束时间（毫秒）
    /// * `limit` - 数量（最大1000）
    pub async fn fetch_klines(
        &self,
        symbol: &str,
        period: &str,
        start_time: Option<i64>,
        end_time: Option<i64>,
        limit: u32,
    ) -> Result<Vec<KLine>, HistoryError> {
        // 并发控制
        let _permit = self.semaphore.acquire().await
            .map_err(|_| HistoryError::ApiRequestFailed("semaphore acquire failed".to_string()))?;

        let mut attempt = 0u8;
        let mut last_error = None;

        while attempt <= self.config.max_retries {
            match self.do_fetch(symbol, period, start_time, end_time, limit).await {
                Ok(klines) => return Ok(klines),
                Err(e) => {
                    // 不可重试错误直接返回
                    if !is_retryable_error(&e) {
                        return Err(e);
                    }
                    let error_msg = e.to_string();
                    last_error = Some(e);
                    attempt += 1;

                    if attempt <= self.config.max_retries {
                        // 计算退避时间（指数退避 + jitter）
                        let backoff = self.calculate_backoff(attempt);
                        warn!(
                            "API fetch failed (attempt {}/{}), retrying in {:?}: {}",
                            attempt, self.config.max_retries, backoff, error_msg
                        );
                        tokio::time::sleep(backoff).await;
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| HistoryError::ApiRequestFailed("unknown error".to_string())))
    }

    /// 执行单次API调用
    async fn do_fetch(
        &self,
        symbol: &str,
        period: &str,
        start_time: Option<i64>,
        end_time: Option<i64>,
        limit: u32,
    ) -> Result<Vec<KLine>, HistoryError> {
        let url = format!("{}/api/v3/klines", self.config.api_base);
        let mut req = self.client.get(&url);

        req = req.query(&[
            ("symbol", symbol),
            ("interval", period),
            ("limit", &limit.min(1000).to_string()),
        ]);

        if let Some(start) = start_time {
            req = req.query(&[("startTime", &start.to_string())]);
        }
        if let Some(end) = end_time {
            req = req.query(&[("endTime", &end.to_string())]);
        }

        let resp = req.send().await
            .map_err(|e| HistoryError::ApiRequestFailed(format!("HTTP request failed: {}", e)))?;

        if !resp.status().is_success() {
            return Err(HistoryError::ApiRequestFailed(format!(
                "API returned error status: {}", resp.status()
            )));
        }

        let raw_klines: Vec<Vec<serde_json::Value>> = resp.json().await
            .map_err(|e| HistoryError::ApiRequestFailed(format!("JSON parse failed: {}", e)))?;

        let klines: Vec<KLine> = raw_klines
            .into_iter()
            .filter_map(|arr| self.parse_kline(symbol, period, &arr))
            .collect();

        info!("Fetched {} klines for {} {}", klines.len(), symbol, period);
        Ok(klines)
    }

    /// 解析单条K线
    fn parse_kline(&self, symbol: &str, period: &str, arr: &[serde_json::Value]) -> Option<KLine> {
        let timestamp_str = arr.get(0)?.as_str()?;
        let timestamp_ms: i64 = timestamp_str.parse().ok()?;

        let parse_decimal = |idx: usize| -> Option<Decimal> {
            let s = arr.get(idx)?.as_str()?;
            let f: f64 = s.parse().ok()?;
            Decimal::from_f64_retain(f)
        };

        Some(KLine {
            symbol: symbol.to_string(),
            period: period.to_string(),
            open: parse_decimal(1)?,
            high: parse_decimal(2)?,
            low: parse_decimal(3)?,
            close: parse_decimal(4)?,
            volume: parse_decimal(5)?,
            timestamp_ms,
        })
    }

    /// 计算退避时间（指数退避 + jitter）
    ///
    /// 公式: min(initial * 2^attempt, max) * jitter
    /// Jitter: 0.5 + random(0, 0.5) = [0.5, 1.0)
    fn calculate_backoff(&self, attempt: u8) -> Duration {
        let base = self.config.initial_backoff_ms
            .saturating_mul(2u64.pow(attempt as u32))
            .min(self.config.max_backoff_ms);

        // 添加 jitter
        let mut rng = rand::thread_rng();
        let jitter_factor = 0.5 + rng.gen_range(0.0..0.5);
        let actual_ms = (base as f64 * jitter_factor) as u64;

        Duration::from_millis(actual_ms)
    }
}

/// 判断是否为可重试错误
fn is_retryable_error(err: &HistoryError) -> bool {
    match err {
        // 可重试：网络超时、连接失败、Rate Limit、服务器错误
        HistoryError::ApiRequestFailed(msg) => {
            let msg_lower = msg.to_lowercase();
            msg_lower.contains("timeout")
                || msg_lower.contains("connection")
                || msg_lower.contains("429")
                || msg_lower.contains("500")
                || msg_lower.contains("502")
                || msg_lower.contains("503")
                || msg_lower.contains("504")
        }
        // 不可重试：参数错误、未授权、品种不存在
        HistoryError::SymbolNotFound(_)
        | HistoryError::InvalidTimestamp(_)
        | HistoryError::NotQualified { .. } => false,
        // 其他错误默认不重试
        _ => false,
    }
}

/// 并发受限的批量获取
pub struct BatchFetcher {
    client: Arc<HistoryApiClient>,
}

impl BatchFetcher {
    pub fn new(client: HistoryApiClient) -> Self {
        Self {
            client: Arc::new(client),
        }
    }

    /// 批量获取多个品种的历史K线
    ///
    /// # 参数
    /// * `requests` - 请求列表 (symbol, period, start_time, end_time, limit)
    /// * `on_progress` - 进度回调（已完成数, 总数）
    pub async fn fetch_batch<F>(
        &self,
        requests: Vec<(String, String, Option<i64>, Option<i64>, u32)>,
        on_progress: Option<F>,
    ) -> Vec<Result<Vec<KLine>, HistoryError>>
    where
        F: Fn(usize, usize) + Send + 'static,
    {
        let total = requests.len();
        let mut results = Vec::with_capacity(total);

        for (i, (symbol, period, start, end, limit)) in requests.into_iter().enumerate() {
            let result = self.client.fetch_klines(&symbol, &period, start, end, limit).await;
            results.push(result);

            if let Some(callback) = &on_progress {
                callback(i + 1, total);
            }
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backoff_calculation() {
        let config = ApiClientConfig::default();
        let client = HistoryApiClient::with_config(config);

        // 测试退避时间范围
        for attempt in 0..5u8 {
            let backoff = client.calculate_backoff(attempt);
            assert!(backoff.as_millis() > 0);
        }
    }

    #[test]
    fn test_is_retryable_error() {
        assert!(is_retryable_error(&HistoryError::ApiRequestFailed("timeout".to_string())));
        assert!(is_retryable_error(&HistoryError::ApiRequestFailed("connection refused".to_string())));
        assert!(is_retryable_error(&HistoryError::ApiRequestFailed("429 rate limit".to_string())));
        assert!(is_retryable_error(&HistoryError::ApiRequestFailed("500 internal error".to_string())));

        assert!(!is_retryable_error(&HistoryError::SymbolNotFound("BTC".to_string())));
        assert!(!is_retryable_error(&HistoryError::InvalidTimestamp(0)));
    }
}
