#![forbid(unsafe_code)]

//! SymbolRules 交易规则获取器
//!
//! 从币安 API 拉取交易对规则，包括价格/数量精度、手续费、下单限制等。
//!
//! # 使用方式
//!
//! ```rust,ignore
//! let fetcher = SymbolRulesFetcher::new();
//! let rules = fetcher.fetch_symbol_rules("BTCUSDT").await?;
//! println!("BTCUSDT price precision: {}", rules.price_precision);
//! ```

// Re-export from a_common
pub use a_common::api::{BinanceApiGateway, SymbolRulesFetcher, SymbolRulesData, BinanceExchangeInfo, BinanceSymbol, PositionRisk, LeverageBracket, BinanceAccountInfo, BinancePositionRisk, BinanceLeverageBracket};
