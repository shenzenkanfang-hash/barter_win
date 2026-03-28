//! VolatilityRanker - 波动率排名器
//!
//! 功能：
//! - 接收全市场 K线数据
//! - 计算每个品种的滚动波动率（20根K线）
//! - 输出波动率排名

use fnv::FnvHashMap;
use parking_lot::RwLock;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::VecDeque;

/// 品种波动率信息
#[derive(Debug, Clone)]
pub struct SymbolVolatilityInfo {
    /// 交易品种
    pub symbol: String,
    /// 波动率（标准差百分比）
    pub volatility: Decimal,
    /// 波动率排名（1=最高）
    pub rank: u32,
    /// 价格均值
    pub mean_price: Decimal,
    /// 价格数量
    pub sample_count: usize,
}

/// 波动率排名器
pub struct VolatilityRanker {
    /// 品种 -> 价格历史（滚动20根）
    price_history: RwLock<FnvHashMap<String, VecDeque<Decimal>>>,
    /// 品种 -> 波动率（上次计算）
    volatility_cache: RwLock<FnvHashMap<String, Decimal>>,
    /// 滚动窗口大小
    window_size: usize,
}

impl VolatilityRanker {
    /// 创建新的排名器
    pub fn new() -> Self {
        Self {
            price_history: RwLock::new(FnvHashMap::default()),
            volatility_cache: RwLock::new(FnvHashMap::default()),
            window_size: 20, // 20根K线
        }
    }

    /// 创建指定窗口大小的排名器
    pub fn with_window(window_size: usize) -> Self {
        Self {
            price_history: RwLock::new(FnvHashMap::default()),
            volatility_cache: RwLock::new(FnvHashMap::default()),
            window_size,
        }
    }

    /// 更新品种价格
    pub fn update(&self, symbol: &str, price: Decimal) {
        let mut history = self.price_history.write();
        let prices = history.entry(symbol.to_string()).or_insert_with(VecDeque::new);

        prices.push_back(price);
        if prices.len() > self.window_size {
            prices.pop_front();
        }

        // 计算波动率
        if prices.len() >= 2 {
            let volatility = Self::calc_volatility(prices);
            self.volatility_cache.write().insert(symbol.to_string(), volatility);
        }
    }

    /// 获取品种波动率
    pub fn get_volatility(&self, symbol: &str) -> Option<Decimal> {
        self.volatility_cache.read().get(symbol).copied()
    }

    /// 获取波动率排名（返回排序后的列表）
    pub fn get_ranking(&self) -> Vec<SymbolVolatilityInfo> {
        let cache = self.volatility_cache.read();
        let history = self.price_history.read();

        let mut infos: Vec<SymbolVolatilityInfo> = cache
            .iter()
            .filter(|(symbol, _)| {
                // 只返回有足够数据的品种
                history.get(*symbol).map(|h| h.len() >= 2).unwrap_or(false)
            })
            .map(|(symbol, volatility)| {
                let mean_price = history.get(symbol).map(|prices| {
                    let sum: Decimal = prices.iter().sum();
                    sum / Decimal::from(prices.len())
                }).unwrap_or(Decimal::ZERO);

                SymbolVolatilityInfo {
                    symbol: symbol.clone(),
                    volatility: *volatility,
                    rank: 0, // 稍后填充
                    mean_price,
                    sample_count: history.get(symbol).map(|h| h.len()).unwrap_or(0),
                }
            })
            .collect();

        // 按波动率降序排序
        infos.sort_by(|a, b| b.volatility.cmp(&a.volatility));

        // 填充排名
        for (i, info) in infos.iter_mut().enumerate() {
            info.rank = (i + 1) as u32;
        }

        infos
    }

    /// 获取波动率最高的品种
    pub fn get_top_symbol(&self, exclude: Option<&str>) -> Option<SymbolVolatilityInfo> {
        let ranking = self.get_ranking();

        for info in ranking {
            if let Some(ex) = exclude {
                if info.symbol == ex {
                    continue;
                }
            }
            return Some(info);
        }

        None
    }

    /// 获取 Top N 品种
    pub fn get_top_n(&self, n: usize) -> Vec<SymbolVolatilityInfo> {
        let ranking = self.get_ranking();
        ranking.into_iter().take(n).collect()
    }

    /// 清空历史
    pub fn clear(&self) {
        self.price_history.write().clear();
        self.volatility_cache.write().clear();
    }

    /// 计算波动率（标准差百分比）
    fn calc_volatility(prices: &VecDeque<Decimal>) -> Decimal {
        if prices.len() < 2 {
            return Decimal::ZERO;
        }

        let n = Decimal::from(prices.len());
        let sum: Decimal = prices.iter().sum();
        let mean = sum / n;

        // 计算方差
        let mut squared_diff_sum = dec!(0);
        for price in prices {
            let diff = *price - mean;
            squared_diff_sum += diff * diff;
        }
        let variance = squared_diff_sum / n;

        // 标准差 / 均值 * 100 = 波动率百分比
        if mean.is_zero() {
            return Decimal::ZERO;
        }

        // 使用牛顿法近似计算平方根
        let stddev = Self::sqrt_decimal(variance);
        (stddev / mean * dec!(100)).round_dp(4)
    }

    /// 计算 Decimal 的平方根（牛顿法）
    fn sqrt_decimal(x: Decimal) -> Decimal {
        if x <= Decimal::ZERO {
            return Decimal::ZERO;
        }

        let mut guess = x / dec!(2);
        for _ in 0..20 {
            let next = (guess + x / guess) / dec!(2);
            if (guess - next).abs() < dec!(0.00000001) {
                break;
            }
            guess = next;
        }
        guess.round_dp(8)
    }
}

impl Default for VolatilityRanker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_volatility_calculation() {
        let ranker = VolatilityRanker::with_window(5);

        // 更新价格序列
        ranker.update("BTCUSDT", dec!(100));
        ranker.update("BTCUSDT", dec!(105));
        ranker.update("BTCUSDT", dec!(102));
        ranker.update("BTCUSDT", dec!(108));
        ranker.update("BTCUSDT", dec!(103));

        let volatility = ranker.get_volatility("BTCUSDT");
        assert!(volatility.is_some());
        assert!(volatility.unwrap() > Decimal::ZERO);
    }

    #[test]
    fn test_ranking() {
        let ranker = VolatilityRanker::new();

        // 添加两个品种
        for i in 1i32..=25 {
            ranker.update("BTCUSDT", dec!(100) + Decimal::from(i));
            ranker.update("ETHUSDT", dec!(2000) + Decimal::from(i) * dec!(10));
        }

        let ranking = ranker.get_ranking();
        assert_eq!(ranking.len(), 2);
        assert_eq!(ranking[0].rank, 1);
        assert_eq!(ranking[1].rank, 2);
    }

    #[test]
    fn test_top_symbol() {
        let ranker = VolatilityRanker::new();

        // BTC：稳定上涨
        for i in 1i32..=25 {
            ranker.update("BTCUSDT", dec!(100) + Decimal::from(i));
        }

        // ETH：大幅波动（波动更大）
        let eth_prices = [2000, 2050, 1950, 2100, 1900, 2150, 1850, 2200, 1800, 2250,
                          1750, 2300, 1700, 2350, 1650, 2400, 1600, 2450, 1550, 2500,
                          1500, 2550, 1450, 2600, 1400];
        for price in eth_prices {
            ranker.update("ETHUSDT", Decimal::from(price));
        }

        let top = ranker.get_top_symbol(None);
        assert!(top.is_some());
        assert_eq!(top.unwrap().symbol, "ETHUSDT");
    }
}
