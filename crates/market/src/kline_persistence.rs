//! K线 Redis 持久化 - 1m/15m/1d 历史K线存储

use redis::AsyncCommands;

const KLINE_1M_MAX: isize = 320;
const KLINE_15M_MAX: isize = 100;
const KLINE_1D_MAX: isize = 100;

pub struct KlinePersistence {
    redis: redis::aio::ConnectionManager,
}

impl KlinePersistence {
    pub async fn new(redis_url: &str) -> Result<Self, crate::error::MarketError> {
        let client = redis::Client::open(redis_url)
            .map_err(|e| crate::error::MarketError::RedisError(e.to_string()))?;
        let conn = client
            .get_connection_manager()
            .map_err(|e| crate::error::MarketError::RedisError(e.to_string()))?;

        Ok(Self { redis: conn })
    }

    /// 存储 1m K线
    pub async fn push_1m(&mut self, symbol: &str, kline_json: &str) -> Result<(), crate::error::MarketError> {
        let key = format!("kline:1m:{}", symbol);
        let _: () = self
            .redis
            .lpush(&key, kline_json)
            .await
            .map_err(|e| crate::error::MarketError::RedisError(e.to_string()))?;
        let _: () = self
            .redis
            .ltrim(&key, 0, KLINE_1M_MAX - 1)
            .await
            .map_err(|e| crate::error::MarketError::RedisError(e.to_string()))?;
        Ok(())
    }

    /// 存储 15m K线
    pub async fn push_15m(&mut self, symbol: &str, kline_json: &str) -> Result<(), crate::error::MarketError> {
        let key = format!("kline:15m:{}", symbol);
        let _: () = self
            .redis
            .lpush(&key, kline_json)
            .await
            .map_err(|e| crate::error::MarketError::RedisError(e.to_string()))?;
        let _: () = self
            .redis
            .ltrim(&key, 0, KLINE_15M_MAX - 1)
            .await
            .map_err(|e| crate::error::MarketError::RedisError(e.to_string()))?;
        Ok(())
    }

    /// 存储 1d K线
    pub async fn push_1d(&mut self, symbol: &str, kline_json: &str) -> Result<(), crate::error::MarketError> {
        let key = format!("kline:1d:{}", symbol);
        let _: () = self
            .redis
            .lpush(&key, kline_json)
            .await
            .map_err(|e| crate::error::MarketError::RedisError(e.to_string()))?;
        let _: () = self
            .redis
            .ltrim(&key, 0, KLINE_1D_MAX - 1)
            .await
            .map_err(|e| crate::error::MarketError::RedisError(e.to_string()))?;
        Ok(())
    }

    /// 读取 1m K线
    pub async fn get_1m(&mut self, symbol: &str) -> Result<Vec<String>, crate::error::MarketError> {
        let key = format!("kline:1m:{}", symbol);
        let klines: Vec<String> = self
            .redis
            .lrange(&key, 0, -1)
            .await
            .map_err(|e| crate::error::MarketError::RedisError(e.to_string()))?;
        Ok(klines)
    }

    /// 读取 15m K线
    pub async fn get_15m(&mut self, symbol: &str) -> Result<Vec<String>, crate::error::MarketError> {
        let key = format!("kline:15m:{}", symbol);
        let klines: Vec<String> = self
            .redis
            .lrange(&key, 0, -1)
            .await
            .map_err(|e| crate::error::MarketError::RedisError(e.to_string()))?;
        Ok(klines)
    }

    /// 读取 1d K线
    pub async fn get_1d(&mut self, symbol: &str) -> Result<Vec<String>, crate::error::MarketError> {
        let key = format!("kline:1d:{}", symbol);
        let klines: Vec<String> = self
            .redis
            .lrange(&key, 0, -1)
            .await
            .map_err(|e| crate::error::MarketError::RedisError(e.to_string()))?;
        Ok(klines)
    }
}
