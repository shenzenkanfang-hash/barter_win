#![forbid(unsafe_code)]

//! SymbolRules 交易规则获取器
//!
//! 从币安 API 拉取交易对规则，包括价格/数量精度，手续费，下单限制等。
//!
//! # 使用方式
//!
//! ```rust,ignore
//! let fetcher = SymbolRulesFetcher::new();
//! let rules = fetcher.fetch_symbol_rules("BTCUSDT").await?;
//! println!("BTCUSDT price precision: {}", rules.price_precision);
//! ```

use crate::error::EngineError;
use reqwest::Client;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, warn};

/// SymbolRules 获取器
pub struct SymbolRulesFetcher {
    client: Client,
    api_base: String,
}

impl SymbolRulesFetcher {
    /// 创建新的获取器（现货 API）
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            api_base: "https://api.binance.com".to_string(),
        }
    }

    /// 创建 USDT 合约 API 获取器
    pub fn new_futures() -> Self {
        Self {
            client: Client::new(),
            api_base: "https://fapi.binance.com".to_string(),
        }
    }

    /// 创建带自定义 API 地址的获取器（用于测试）
    pub fn with_api_base(api_base: &str) -> Self {
        Self {
            client: Client::new(),
            api_base: api_base.to_string(),
        }
    }

    /// 从币安 API 获取单个交易对规则
    ///
    /// # 参数
    /// * `symbol` - 交易对名称，如 "BTCUSDT"
    ///
    /// # 返回
    /// * `SymbolRulesData` - 交易规则数据
    pub async fn fetch_symbol_rules(&self, symbol: &str) -> Result<SymbolRulesData, EngineError> {
        let url = format!("{}/api/v3/exchangeInfo", self.api_base);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| EngineError::Other(format!("HTTP 请求失败: {}", e)))?;

        if !resp.status().is_success() {
            return Err(EngineError::Other(format!(
                "API 返回错误状态: {}",
                resp.status()
            )));
        }

        let info: BinanceExchangeInfo = resp
            .json()
            .await
            .map_err(|e| EngineError::Other(format!("解析 JSON 失败: {}", e)))?;

        let symbol_info = info
            .symbols
            .iter()
            .find(|s| s.symbol == symbol)
            .ok_or_else(|| EngineError::SymbolNotFound(symbol.to_string()))?;

        // 解析 filters
        let price_filter = symbol_info
            .filters
            .iter()
            .find(|f| f.filter_type == "PRICE_FILTER");
        let lot_size = symbol_info
            .filters
            .iter()
            .find(|f| f.filter_type == "LOT_SIZE");
        let min_notional = symbol_info
            .filters
            .iter()
            .find(|f| f.filter_type == "MIN_NOTIONAL");

        let tick_size = price_filter
            .and_then(|f| f.tick_size.as_ref())
            .and_then(|s| s.parse::<Decimal>().ok())
            .unwrap_or(dec!(0.01));

        let min_qty = lot_size
            .and_then(|f| f.min_qty.as_ref())
            .and_then(|s| s.parse::<Decimal>().ok())
            .unwrap_or(dec!(0.000001));

        let step_size = lot_size
            .and_then(|f| f.step_size.as_ref())
            .and_then(|s| s.parse::<Decimal>().ok())
            .unwrap_or(dec!(0.000001));

        let min_notional_val = min_notional
            .and_then(|f| f.min_notional.as_ref())
            .and_then(|s| s.parse::<Decimal>().ok())
            .unwrap_or(dec!(10));

        Ok(SymbolRulesData {
            symbol: symbol.to_string(),
            price_precision: symbol_info.pricePrecision as u8,
            quantity_precision: symbol_info.quantityPrecision as u8,
            tick_size,
            min_qty,
            step_size,
            min_notional: min_notional_val,
            max_notional: dec!(1000000),
            leverage: 1,
            max_leverage: 20, // 默认值，会被 enrich_with_leverage_brackets 更新
            maker_fee: dec!(0.0002),
            taker_fee: dec!(0.0005),
        })
    }

    /// 用杠杆档位 API 丰富 SymbolRulesData
    ///
    /// # 参数
    /// * `rules` - 交易规则数据（会被修改）
    pub async fn enrich_with_leverage_brackets(&self, rules: &mut SymbolRulesData) -> Result<(), EngineError> {
        if let Ok(brackets) = self.fetch_leverage_brackets(Some(&rules.symbol)).await {
            if let Some(bracket) = brackets.first() {
                rules.max_leverage = bracket.max_leverage;
            }
        }
        Ok(())
    }

    /// 批量获取所有 USDT 交易对规则
    ///
    /// # 返回
    /// * `Vec<SymbolRulesData>` - 所有 USDT 交易对规则列表
    pub async fn fetch_all_usdt_symbol_rules(&self) -> Result<Vec<SymbolRulesData>, EngineError> {
        let url = format!("{}/api/v3/exchangeInfo", self.api_base);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| EngineError::Other(format!("HTTP 请求失败: {}", e)))?;

        if !resp.status().is_success() {
            return Err(EngineError::Other(format!(
                "API 返回错误状态: {}",
                resp.status()
            )));
        }

        let info: BinanceExchangeInfo = resp
            .json()
            .await
            .map_err(|e| EngineError::Other(format!("解析 JSON 失败: {}", e)))?;

        let mut rules = Vec::new();
        for symbol in info
            .symbols
            .iter()
            .filter(|s| s.quoteAsset == "USDT" && s.status == "TRADING")
        {
            // 解析 filters
            let price_filter = symbol
                .filters
                .iter()
                .find(|f| f.filter_type == "PRICE_FILTER");
            let lot_size = symbol
                .filters
                .iter()
                .find(|f| f.filter_type == "LOT_SIZE");
            let min_notional = symbol
                .filters
                .iter()
                .find(|f| f.filter_type == "MIN_NOTIONAL");

            let tick_size = price_filter
                .and_then(|f| f.tick_size.as_ref())
                .and_then(|s| s.parse::<Decimal>().ok())
                .unwrap_or(dec!(0.01));

            let min_qty = lot_size
                .and_then(|f| f.min_qty.as_ref())
                .and_then(|s| s.parse::<Decimal>().ok())
                .unwrap_or(dec!(0.000001));

            let step_size = lot_size
                .and_then(|f| f.step_size.as_ref())
                .and_then(|s| s.parse::<Decimal>().ok())
                .unwrap_or(dec!(0.000001));

            let min_notional_val = min_notional
                .and_then(|f| f.min_notional.as_ref())
                .and_then(|s| s.parse::<Decimal>().ok())
                .unwrap_or(dec!(10));

            rules.push(SymbolRulesData {
                symbol: symbol.symbol.clone(),
                price_precision: symbol.pricePrecision as u8,
                quantity_precision: symbol.quantityPrecision as u8,
                tick_size,
                min_qty,
                step_size,
                min_notional: min_notional_val,
                max_notional: dec!(1000000),
                leverage: 1,
                max_leverage: 20,
                maker_fee: dec!(0.0002),
                taker_fee: dec!(0.0005),
            });
        }

        info!("从币安 API 获取了 {} 个 USDT 交易对规则", rules.len());
        Ok(rules)
    }

    /// 从币安 API 获取账户信息
    ///
    /// # 返回
    /// * `BinanceAccountInfo` - 账户信息
    pub async fn fetch_account_info(&self) -> Result<BinanceAccountInfo, EngineError> {
        let url = format!("{}/api/v3/account", self.api_base);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| EngineError::Other(format!("HTTP 请求失败: {}", e)))?;

        if !resp.status().is_success() {
            return Err(EngineError::Other(format!(
                "API 返回错误状态: {}",
                resp.status()
            )));
        }

        let info: BinanceAccountInfo = resp
            .json()
            .await
            .map_err(|e| EngineError::Other(format!("解析 JSON 失败: {}", e)))?;

        Ok(BinanceAccountInfo {
            account_type: info.account_type,
            can_trade: info.can_trade,
            can_withdraw: info.can_withdraw,
            can_deposit: info.can_deposit,
            update_time: info.update_time,
        })
    }

    /// 从币安 API 获取指定交易对的持仓风险信息
    ///
    /// # 参数
    /// * `symbol` - 交易对名称，如 "BTCUSDT"
    ///
    /// # 返回
    /// * `PositionRisk` - 持仓风险信息
    pub async fn fetch_position_risk(&self, symbol: &str) -> Result<PositionRisk, EngineError> {
        let url = format!("{}/api/v3/positionRisk", self.api_base);
        let resp = self
            .client
            .get(&url)
            .query(&[("symbol", symbol)])
            .send()
            .await
            .map_err(|e| EngineError::Other(format!("HTTP 请求失败: {}", e)))?;

        if !resp.status().is_success() {
            return Err(EngineError::Other(format!(
                "API 返回错误状态: {}",
                resp.status()
            )));
        }

        let positions: Vec<BinancePositionRisk> = resp
            .json()
            .await
            .map_err(|e| EngineError::Other(format!("解析 JSON 失败: {}", e)))?;

        // 找到指定交易对的持仓
        let pos = positions
            .into_iter()
            .find(|p| p.symbol == symbol)
            .ok_or_else(|| EngineError::SymbolNotFound(symbol.to_string()))?;

        Ok(PositionRisk {
            symbol: pos.symbol,
            position_side: pos.position_side,
            quantity: pos.position_amt,
            entry_price: pos.entry_price,
            mark_price: pos.mark_price,
            unrealized_pnl: pos.unrealizedProfit,
            leverage: pos.leverage,
            isolated: pos.isolated,
            margin_ratio: pos.margin_ratio,
        })
    }

    /// 从币安 USDT 合约 API 获取杠杆档位
    ///
    /// # 参数
    /// * `symbol` - 可选，交易对名称，如 "BTCUSDT"
    ///
    /// # 返回
    /// * `Vec<LeverageBracket>` - 杠杆档位列表
    pub async fn fetch_leverage_brackets(&self, symbol: Option<&str>) -> Result<Vec<LeverageBracket>, EngineError> {
        let url = format!("{}/fapi/v1/leverageBracket", self.api_base);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| EngineError::Other(format!("HTTP 请求失败: {}", e)))?;

        if !resp.status().is_success() {
            return Err(EngineError::Other(format!(
                "API 返回错误状态: {}",
                resp.status()
            )));
        }

        let brackets: Vec<BinanceLeverageBracket> = resp
            .json()
            .await
            .map_err(|e| EngineError::Other(format!("解析 JSON 失败: {}", e)))?;

        let result = brackets
            .into_iter()
            .map(|b| LeverageBracket {
                symbol: b.symbol,
                bracket: b.bracket,
                max_leverage: b.max_leverage,
                min_notional: b.min_notional,
                maintenance_margin_ratio: b.maintenance_margin_ratio,
            })
            .filter(|b| {
                if let Some(s) = symbol {
                    b.symbol == s
                } else {
                    true
                }
            })
            .collect();

        Ok(result)
    }

    /// 从币安 USDT 合约 API 获取最大可用杠杆
    ///
    /// # 参数
    /// * `symbol` - 交易对名称，如 "BTCUSDT"
    ///
    /// # 返回
    /// * `i32` - 最大可用杠杆倍数
    pub async fn get_max_leverage(&self, symbol: &str) -> Result<i32, EngineError> {
        let brackets = self.fetch_leverage_brackets(Some(symbol)).await?;
        brackets
            .first()
            .map(|b| b.max_leverage)
            .ok_or_else(|| EngineError::SymbolNotFound(symbol.to_string()))
    }
}

impl Default for SymbolRulesFetcher {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// 币安 API 数据结构
// ============================================================================

/// 币安交易所信息
#[derive(Debug, Clone, Deserialize)]
pub struct BinanceExchangeInfo {
    pub timezone: String,
    #[serde(rename = "serverTime")]
    pub server_time: i64,
    pub symbols: Vec<BinanceSymbol>,
}

/// 币安交易对信息
#[derive(Debug, Clone, Deserialize)]
pub struct BinanceSymbol {
    pub symbol: String,
    pub status: String,
    #[serde(rename = "baseAsset")]
    pub baseAsset: String,
    #[serde(rename = "quoteAsset")]
    pub quoteAsset: String,
    #[serde(rename = "pricePrecision")]
    pub pricePrecision: i32,
    #[serde(rename = "quantityPrecision")]
    pub quantityPrecision: i32,
    pub filters: Vec<BinanceFilter>,
}

/// 币安过滤器
#[derive(Debug, Clone, Deserialize)]
pub struct BinanceFilter {
    #[serde(rename = "filterType")]
    pub filter_type: String,
    #[serde(rename = "minQty")]
    pub min_qty: Option<String>,
    #[serde(rename = "maxQty")]
    pub max_qty: Option<String>,
    #[serde(rename = "stepSize")]
    pub step_size: Option<String>,
    #[serde(rename = "tickSize")]
    pub tick_size: Option<String>,
    #[serde(rename = "minNotional")]
    pub min_notional: Option<String>,
}

/// 币安账户信息
#[derive(Debug, Clone, Deserialize)]
pub struct BinanceAccountInfo {
    #[serde(rename = "accountType")]
    pub account_type: String,
    #[serde(rename = "canTrade")]
    pub can_trade: bool,
    #[serde(rename = "canWithdraw")]
    pub can_withdraw: bool,
    #[serde(rename = "canDeposit")]
    pub can_deposit: bool,
    #[serde(rename = "updateTime")]
    pub update_time: i64,
}

/// 币安持仓风险信息
#[derive(Debug, Clone, Deserialize)]
pub struct BinancePositionRisk {
    pub symbol: String,
    #[serde(rename = "positionSide")]
    pub position_side: String,
    #[serde(rename = "positionAmt")]
    pub position_amt: String,
    #[serde(rename = "entryPrice")]
    pub entry_price: String,
    #[serde(rename = "markPrice")]
    pub mark_price: String,
    #[serde(rename = "unrealizedProfit")]
    pub unrealizedProfit: String,
    pub leverage: i32,
    pub isolated: bool,
    #[serde(rename = "marginRatio")]
    pub margin_ratio: String,
}

/// 持仓风险信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionRisk {
    /// 交易对
    pub symbol: String,
    /// 持仓方向
    pub position_side: String,
    /// 持仓数量
    pub quantity: String,
    /// 入场价格
    pub entry_price: String,
    /// 标记价格
    pub mark_price: String,
    /// 未实现盈亏
    pub unrealized_pnl: String,
    /// 杠杆倍数
    pub leverage: i32,
    /// 是否是逐仓
    pub isolated: bool,
    /// 保证金率
    pub margin_ratio: String,
}

/// 币安杠杆档位信息
#[derive(Debug, Clone, Deserialize)]
pub struct BinanceLeverageBracket {
    pub symbol: String,
    pub bracket: i32,
    #[serde(rename = "maxLeverage")]
    pub max_leverage: i32,
    #[serde(rename = "minNotional")]
    pub min_notional: String,
    #[serde(rename = "maintMarginRatio")]
    pub maintenance_margin_ratio: String,
}

/// 杠杆档位信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeverageBracket {
    /// 交易对
    pub symbol: String,
    /// 档位
    pub bracket: i32,
    /// 最大杠杆
    pub max_leverage: i32,
    /// 最小名义价值
    pub min_notional: String,
    /// 维持保证金率
    pub maintenance_margin_ratio: String,
}

// ============================================================================
// SymbolRulesData - 交易规则数据
// ============================================================================

/// 交易规则数据
///
/// 用于存储从 API 获取的交易对规则信息。
/// 与 memory_backup::SymbolRulesData 结构一致。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolRulesData {
    /// 交易对
    pub symbol: String,
    /// 价格精度
    pub price_precision: u8,
    /// 数量精度
    pub quantity_precision: u8,
    /// 步长
    pub tick_size: Decimal,
    /// 最小数量
    pub min_qty: Decimal,
    /// 步长数量
    pub step_size: Decimal,
    /// 最小名义价值
    pub min_notional: Decimal,
    /// 最大名义价值
    pub max_notional: Decimal,
    /// 杠杆
    pub leverage: i32,
    /// 最大可用杠杆（从杠杆档位API获取）
    pub max_leverage: i32,
    /// 做市商费率
    pub maker_fee: Decimal,
    /// 吃单费率
    pub taker_fee: Decimal,
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_rules_data_creation() {
        let rules = SymbolRulesData {
            symbol: "BTCUSDT".to_string(),
            price_precision: 2,
            quantity_precision: 6,
            tick_size: dec!(0.01),
            min_qty: dec!(0.00001),
            step_size: dec!(0.00001),
            min_notional: dec!(10),
            max_notional: dec!(1000000),
            leverage: 1,
            max_leverage: 20,
            maker_fee: dec!(0.0002),
            taker_fee: dec!(0.0005),
        };

        assert_eq!(rules.symbol, "BTCUSDT");
        assert_eq!(rules.price_precision, 2);
        assert_eq!(rules.quantity_precision, 6);
    }

    #[test]
    fn test_binance_symbol_deserialization() {
        let json = r#"{
            "symbol": "BTCUSDT",
            "status": "TRADING",
            "baseAsset": "BTC",
            "quoteAsset": "USDT",
            "pricePrecision": 2,
            "quantityPrecision": 6,
            "filters": [
                {"filterType": "PRICE_FILTER", "minPrice": "0.01", "maxPrice": "1000000", "tickSize": "0.01"},
                {"filterType": "LOT_SIZE", "minQty": "0.00001", "maxQty": "9000", "stepSize": "0.00001"},
                {"filterType": "MIN_NOTIONAL", "minNotional": "10"}
            ]
        }"#;

        let symbol: BinanceSymbol = serde_json::from_str(json).unwrap();
        assert_eq!(symbol.symbol, "BTCUSDT");
        assert_eq!(symbol.quoteAsset, "USDT");
        assert_eq!(symbol.pricePrecision, 2);
        assert_eq!(symbol.filters.len(), 3);
    }

    #[test]
    fn test_fetcher_creation() {
        let fetcher = SymbolRulesFetcher::new();
        assert_eq!(fetcher.api_base, "https://api.binance.com");

        let test_fetcher = SymbolRulesFetcher::with_api_base("https://testnet.binance.vision");
        assert_eq!(test_fetcher.api_base, "https://testnet.binance.vision");
    }

    #[test]
    fn test_account_info_deserialization() {
        let json = r#"{
            "accountType": "SPOT",
            "canTrade": true,
            "canWithdraw": true,
            "canDeposit": true,
            "updateTime": 1234567890
        }"#;

        let info: BinanceAccountInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.account_type, "SPOT");
        assert_eq!(info.can_trade, true);
        assert_eq!(info.update_time, 1234567890);
    }

    #[test]
    fn test_position_risk_deserialization() {
        let json = r#"{
            "symbol": "BTCUSDT",
            "positionSide": "BOTH",
            "positionAmt": "1.5",
            "entryPrice": "50000.0",
            "markPrice": "51000.0",
            "unrealizedProfit": "1500.0",
            "leverage": 10,
            "isolated": false,
            "marginRatio": "0.02"
        }"#;

        let pos: BinancePositionRisk = serde_json::from_str(json).unwrap();
        assert_eq!(pos.symbol, "BTCUSDT");
        assert_eq!(pos.position_side, "BOTH");
        assert_eq!(pos.leverage, 10);
    }

    #[test]
    fn test_account_info_struct() {
        let info = BinanceAccountInfo {
            account_type: "SPOT".to_string(),
            can_trade: true,
            can_withdraw: true,
            can_deposit: true,
            update_time: 1234567890,
        };

        assert_eq!(info.account_type, "SPOT");
        assert_eq!(info.can_trade, true);
    }

    #[test]
    fn test_position_risk_struct() {
        let pos = PositionRisk {
            symbol: "BTCUSDT".to_string(),
            position_side: "BOTH".to_string(),
            quantity: "1.5".to_string(),
            entry_price: "50000.0".to_string(),
            mark_price: "51000.0".to_string(),
            unrealized_pnl: "1500.0".to_string(),
            leverage: 10,
            isolated: false,
            margin_ratio: "0.02".to_string(),
        };

        assert_eq!(pos.symbol, "BTCUSDT");
        assert_eq!(pos.leverage, 10);
    }

    #[test]
    fn test_new_futures() {
        let fetcher = SymbolRulesFetcher::new_futures();
        assert_eq!(fetcher.api_base, "https://fapi.binance.com");
    }

    #[test]
    fn test_leverage_bracket_deserialization() {
        let json = r#"{
            "symbol": "BTCUSDT",
            "bracket": 1,
            "maxLeverage": 20,
            "minNotional": "0",
            "maintMarginRatio": "0.005"
        }"#;

        let bracket: BinanceLeverageBracket = serde_json::from_str(json).unwrap();
        assert_eq!(bracket.symbol, "BTCUSDT");
        assert_eq!(bracket.bracket, 1);
        assert_eq!(bracket.max_leverage, 20);
    }

    #[test]
    fn test_leverage_bracket_struct() {
        let bracket = LeverageBracket {
            symbol: "BTCUSDT".to_string(),
            bracket: 1,
            max_leverage: 20,
            min_notional: "0".to_string(),
            maintenance_margin_ratio: "0.005".to_string(),
        };

        assert_eq!(bracket.symbol, "BTCUSDT");
        assert_eq!(bracket.max_leverage, 20);
    }
}
