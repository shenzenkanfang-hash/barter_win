//! VolatilityManager - 波动率计算
//!
//! 每次写入K线时触发计算，不管K线是否闭合。

use std::collections::HashMap;
use parking_lot::RwLock;

use crate::ws::kline_1m::ws::KlineData;
use super::store_trait::VolatilityData;

/// 波动率计算器
pub struct VolatilityManager {
    /// symbol -> (prices, current_volatility)
    data: RwLock<HashMap<String, VolatilityState>>,
    /// 历史K线条数阈值
    history_size: usize,
}

/// 波动率状态
struct VolatilityState {
    /// 价格历史
    prices: Vec<f64>,
    /// 当前波动率
    volatility: f64,
    /// 更新时间戳
    update_time_ms: i64,
}

impl VolatilityManager {
    pub fn new() -> Self {
        Self {
            data: RwLock::new(HashMap::new()),
            history_size: 20, // 使用最近20根K线计算波动率
        }
    }

    /// 更新波动率（每次K线写入时调用，不管是否闭合）
    pub fn update(&self, symbol: &str, kline: &KlineData) {
        let symbol_lower = symbol.to_lowercase();
        let price = kline.close.parse::<f64>().unwrap_or(0.0);
        let timestamp_ms = kline.kline_close_time;

        let mut data = self.data.write();
        let state = data.entry(symbol_lower.clone())
            .or_insert_with(|| VolatilityState {
                prices: Vec::with_capacity(100),
                volatility: 0.0,
                update_time_ms: 0,
            });

        // 添加价格到历史
        state.prices.push(price);
        if state.prices.len() > self.history_size {
            state.prices.remove(0);
        }

        // 计算波动率（标准差）
        state.volatility = Self::calc_volatility(&state.prices);
        state.update_time_ms = timestamp_ms;
    }

    /// 计算波动率（简单标准差）
    fn calc_volatility(prices: &[f64]) -> f64 {
        if prices.len() < 2 {
            return 0.0;
        }

        let mean = prices.iter().sum::<f64>() / prices.len() as f64;
        let variance = prices.iter()
            .map(|p| (p - mean).powi(2))
            .sum::<f64>() / prices.len() as f64;
        
        variance.sqrt()
    }

    /// 获取波动率
    pub fn get_volatility(&self, symbol: &str) -> Option<VolatilityData> {
        let symbol_lower = symbol.to_lowercase();
        let data = self.data.read();
        
        data.get(&symbol_lower).map(|state| VolatilityData {
            symbol: symbol.to_string(),
            volatility: state.volatility,
            update_time_ms: state.update_time_ms,
        })
    }

    /// 获取所有波动率
    pub fn get_all(&self) -> HashMap<String, VolatilityData> {
        let data = self.data.read();
        data.iter()
            .map(|(k, v)| (k.clone(), VolatilityData {
                symbol: k.clone(),
                volatility: v.volatility,
                update_time_ms: v.update_time_ms,
            }))
            .collect()
    }

    /// 清空
    pub fn clear(&self) {
        self.data.write().clear();
    }
}

impl Default for VolatilityManager {
    fn default() -> Self {
        Self::new()
    }
}
