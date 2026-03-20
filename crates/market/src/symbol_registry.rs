//! 品种注册中心 - Redis 存储 + 订阅管理

use fnv::FnvHashSet;
use redis::AsyncCommands;
use tokio::sync::RwLock;

pub struct SymbolRegistry {
    redis: redis::aio::ConnectionManager,
    trading_symbols: RwLock<FnvHashSet<String>>,
    last_update: std::time::Instant,
    update_interval: std::time::Duration,
}

impl SymbolRegistry {
    pub async fn new(redis_url: &str) -> Result<Self, crate::error::MarketError> {
        let client = redis::Client::open(redis_url)
            .map_err(|e| crate::error::MarketError::RedisError(e.to_string()))?;
        let conn = client
            .get_connection_manager()
            .await
            .map_err(|e| crate::error::MarketError::RedisError(e.to_string()))?;

        Ok(Self {
            redis: conn,
            trading_symbols: RwLock::new(FnvHashSet::default()),
            last_update: std::time::Instant::now(),
            update_interval: std::time::Duration::from_secs(120), // 2分钟
        })
    }

    /// 从 Binance 获取交易对信息并更新 Redis
    pub async fn update_symbols(&mut self) -> Result<(), crate::error::MarketError> {
        let client = reqwest::Client::new();
        let resp = client
            .get("https://fapi.binance.com/fapi/v1/exchangeInfo")
            .send()
            .await
            .map_err(|e| crate::error::MarketError::NetworkError(e.to_string()))?;

        let info: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| crate::error::MarketError::ParseError(e.to_string()))?;

        if let Some(symbols) = info.get("symbols").and_then(|s| s.as_array()) {
            let mut new_symbols = FnvHashSet::default();

            for symbol_info in symbols {
                if let (Some(symbol), Some(status)) = (
                    symbol_info.get("symbol").and_then(|s| s.as_str()),
                    symbol_info.get("status").and_then(|s| s.as_str()),
                ) {
                    if status == "TRADING" {
                        let json = symbol_info.to_string();
                        let _: () = self
                            .redis
                            .hset("exchangeInfo", symbol, &json)
                            .await
                            .map_err(|e| crate::error::MarketError::RedisError(e.to_string()))?;

                        new_symbols.insert(symbol.to_string());
                    }
                }
            }

            let mut symbols_guard = self.trading_symbols.write().await;
            *symbols_guard = new_symbols;
        }

        self.last_update = std::time::Instant::now();
        Ok(())
    }

    pub fn needs_update(&self) -> bool {
        self.last_update.elapsed() >= self.update_interval
    }

    pub async fn get_trading_symbols(&self) -> FnvHashSet<String> {
        self.trading_symbols.read().await.clone()
    }

    pub async fn get_symbol_info(&mut self, symbol: &str) -> Option<String> {
        let info: Option<String> = self.redis.hget("exchangeInfo", symbol).await.ok()?;
        info
    }
}
