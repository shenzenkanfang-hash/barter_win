//! 品种注册中心 - Redis 存储 + 订阅管理

use fnv::FnvHashSet;
use redis::AsyncCommands;
use tokio::sync::RwLock;

pub struct SymbolRegistry {
    /// Redis 连接（mock 模式下为 None）
    redis: Option<redis::aio::ConnectionManager>,
    trading_symbols: RwLock<FnvHashSet<String>>,
    last_update: std::time::Instant,
    update_interval: std::time::Duration,
    /// 是否为 mock 模式
    is_mock: bool,
}

impl SymbolRegistry {
    /// 创建测试用的 Mock SymbolRegistry（不依赖 Redis）
    pub fn new_mock() -> Self {
        Self {
            redis: None,
            trading_symbols: RwLock::new(FnvHashSet::default()),
            last_update: std::time::Instant::now(),
            update_interval: std::time::Duration::from_secs(120),
            is_mock: true,
        }
    }

    pub async fn new(redis_url: &str) -> Result<Self, crate::claint::MarketError> {
        let client = redis::Client::open(redis_url)
            .map_err(|e| crate::claint::MarketError::RedisError(e.to_string()))?;
        let conn = client
            .get_connection_manager()
            .await
            .map_err(|e| crate::claint::MarketError::RedisError(e.to_string()))?;

        Ok(Self {
            redis: Some(conn),
            trading_symbols: RwLock::new(FnvHashSet::default()),
            last_update: std::time::Instant::now(),
            update_interval: std::time::Duration::from_secs(120), // 2分钟
            is_mock: false,
        })
    }

    /// 从 Binance 获取交易对信息并更新 Redis
    pub async fn update_symbols(&mut self) -> Result<(), crate::claint::MarketError> {
        // Mock 模式下跳过更新
        if self.is_mock {
            tracing::debug!("[SymbolRegistry] Mock 模式，跳过品种更新");
            return Ok(());
        }

        let client = reqwest::Client::new();
        let resp = client
            .get("https://fapi.binance.com/fapi/v1/exchangeInfo")
            .send()
            .await
            .map_err(|e| crate::claint::MarketError::NetworkError(e.to_string()))?;

        let info: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| crate::claint::MarketError::ParseError(e.to_string()))?;

        if let Some(symbols) = info.get("symbols").and_then(|s| s.as_array()) {
            let mut new_symbols = FnvHashSet::default();

            for symbol_info in symbols {
                if let (Some(symbol), Some(status)) = (
                    symbol_info.get("symbol").and_then(|s| s.as_str()),
                    symbol_info.get("status").and_then(|s| s.as_str()),
                ) {
                    if status == "TRADING" {
                        // 只在非 mock 模式下写入 Redis
                        if let Some(ref mut redis_conn) = self.redis {
                            let json = symbol_info.to_string();
                            let _: () = redis_conn
                                .hset("exchangeInfo", symbol, &json)
                                .await
                                .map_err(|e| crate::claint::MarketError::RedisError(e.to_string()))?;
                        }
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
        if let Some(ref mut redis_conn) = self.redis {
            let info: Option<String> = redis_conn.hget("exchangeInfo", symbol).await.ok()?;
            info
        } else {
            None
        }
    }
}
