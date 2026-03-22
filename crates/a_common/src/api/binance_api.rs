#![forbid(unsafe_code)]

//! 币安 API 网关
//!
//! 从币安 API 拉取交易对规则，包括价格/数量精度、手续费、下单限制等。
//! 包含限速规则和请求管理。
//!
//! # 使用方式
//!
//! ```rust,ignore
//! let api = BinanceApiGateway::new();
//! let rules = api.fetch_symbol_rules("BTCUSDT").await?;
//! println!("BTCUSDT price precision: {}", rules.price_precision);
//! ```

use crate::claint::error::EngineError;
use crate::config::Paths;
use chrono::Utc;
use reqwest::Client;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::sync::Arc;
use std::time::{Duration, Instant};
use parking_lot::Mutex;
use tracing::{info, warn, error};

/// API 限速器
#[derive(Debug)]
pub struct RateLimiter {
    /// 每分钟请求数限制
    requests_per_minute: u32,
    /// 当前窗口起始时间
    window_start: Mutex<Instant>,
    /// 当前窗口内请求数
    request_count: Mutex<u32>,
}

impl RateLimiter {
    /// 创建新的限速器
    pub fn new(requests_per_minute: u32) -> Self {
        Self {
            requests_per_minute,
            window_start: Mutex::new(Instant::now()),
            request_count: Mutex::new(0),
        }
    }

    /// 获取请求许可（如果超过限制则等待）
    pub async fn acquire(&self) {
        loop {
            let elapsed = {
                let mut window_start = self.window_start.lock();
                let mut request_count = self.request_count.lock();

                let elapsed = window_start.elapsed();
                if elapsed > Duration::from_secs(60) {
                    // 重置窗口
                    *window_start = Instant::now();
                    *request_count = 0;
                }

                if *request_count >= self.requests_per_minute {
                    // 等待直到窗口结束
                    let wait_time = Duration::from_secs(60) - elapsed;
                    drop(window_start);
                    drop(request_count);
                    tokio::time::sleep(wait_time).await;
                    continue;
                }

                *request_count += 1;
                elapsed
            };
            break;
        }
    }
}

/// 币安 API 网关
pub struct BinanceApiGateway {
    client: Client,
    /// 价格/市场数据 API（实盘）
    market_api_base: String,
    /// 账户 API（可配置为实盘或测试网）
    account_api_base: String,
    rate_limiter: Arc<RateLimiter>,
}

impl BinanceApiGateway {
    /// 创建新的网关（现货 API）
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            market_api_base: "https://api.binance.com".to_string(),
            account_api_base: "https://api.binance.com".to_string(),
            rate_limiter: Arc::new(RateLimiter::new(1200)), // 币安现货 API 限制
        }
    }

    /// 创建 USDT 合约 API 网关（实盘价格 + 实盘账户）
    pub fn new_futures() -> Self {
        Self {
            client: Client::new(),
            market_api_base: "https://fapi.binance.com".to_string(),
            account_api_base: "https://fapi.binance.com".to_string(),
            rate_limiter: Arc::new(RateLimiter::new(2400)), // 合约 API 限制更高
        }
    }

    /// 创建 USDT 合约 API 网关（实盘价格 + 测试网账户）
    ///
    /// 用于：实盘行情 + 模拟交易
    pub fn new_futures_with_testnet() -> Self {
        Self {
            client: Client::new(),
            market_api_base: "https://fapi.binance.com".to_string(),      // 实盘行情
            account_api_base: "https://testnet.binancefuture.com".to_string(), // 测试网账户
            rate_limiter: Arc::new(RateLimiter::new(2400)),
        }
    }

    /// 创建带自定义 API 地址的网关（用于测试）
    pub fn with_api_base(api_base: &str) -> Self {
        Self {
            client: Client::new(),
            market_api_base: api_base.to_string(),
            account_api_base: api_base.to_string(),
            rate_limiter: Arc::new(RateLimiter::new(1200)),
        }
    }

    /// 获取市场数据 API 地址
    pub fn market_api_base(&self) -> &str {
        &self.market_api_base
    }

    /// 获取账户 API 地址
    pub fn account_api_base(&self) -> &str {
        &self.account_api_base
    }

    /// 从币安 API 获取单个交易对规则
    ///
    /// # 参数
    /// * `symbol` - 交易对名称，如 "BTCUSDT"
    ///
    /// # 返回
    /// * `SymbolRulesData` - 交易规则数据
    pub async fn fetch_symbol_rules(&self, symbol: &str) -> Result<SymbolRulesData, EngineError> {
        self.rate_limiter.acquire().await;

        let url = format!("{}/api/v3/exchangeInfo", self.market_api_base);
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
            price_precision: symbol_info.pricePrecision.unwrap_or(8) as u8,
            quantity_precision: symbol_info.quantityPrecision.unwrap_or(8) as u8,
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
        self.rate_limiter.acquire().await;

        let url = format!("{}/api/v3/exchangeInfo", self.market_api_base);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| EngineError::Other(format!("HTTP 请求失败: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();
            return Err(EngineError::Other(format!(
                "API 返回错误状态: {} - Body: {}",
                status,
                &body_text[..body_text.len().min(500)]
            )));
        }

        let body_text = resp.text().await.map_err(|e| EngineError::Other(format!("读取响应体失败: {}", e)))?;
        let info: BinanceExchangeInfo = serde_json::from_str(&body_text)
            .map_err(|e| EngineError::Other(format!("解析 JSON 失败: {} - Response: {}", e, &body_text[..body_text.len().min(500)])))?;

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
                price_precision: symbol.pricePrecision.unwrap_or(8) as u8,
                quantity_precision: symbol.quantityPrecision.unwrap_or(8) as u8,
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

    /// 从 API 获取并直接保存每个交易对的原始规则 JSON（不解析）
    pub async fn fetch_and_save_all_usdt_symbol_rules(&self) -> Result<Vec<SymbolRulesData>, EngineError> {
        self.rate_limiter.acquire().await;

        let url = format!("{}/api/v3/exchangeInfo", self.market_api_base);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| EngineError::Other(format!("HTTP 请求失败: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();
            return Err(EngineError::Other(format!(
                "API 返回错误状态: {} - Body: {}",
                status,
                &body_text[..body_text.len().min(500)]
            )));
        }

        let body_text = resp.text().await.map_err(|e| EngineError::Other(format!("读取响应体失败: {}", e)))?;

        // 解析获取所有交易对列表
        let info: BinanceExchangeInfo = serde_json::from_str(&body_text)
            .map_err(|e| EngineError::Other(format!("解析 JSON 失败: {}", e)))?;

        // 保存原始 JSON
        let paths = Paths::new();
        let base_dir = &paths.symbols_rules_dir;
        info!("创建目录: {}", base_dir);
        if let Err(e) = std::fs::create_dir_all(base_dir) {
            error!("创建目录失败: {}", e);
            return Err(EngineError::Other(format!("创建目录失败: {}", e)));
        }
        info!("目录创建成功: {}", base_dir);

        let trading_symbols: Vec<_> = info
            .symbols
            .iter()
            .filter(|s| s.quoteAsset == "USDT" && s.status == "TRADING")
            .collect();

        for symbol in &trading_symbols {
            let file_path = format!("{}/{}.json", base_dir, symbol.symbol.to_lowercase());
            info!("写入文件: {}", file_path);
            if let Ok(json_str) = serde_json::to_string_pretty(symbol) {
                match std::fs::write(&file_path, json_str.as_bytes()) {
                    Ok(_) => info!("已保存: {}", symbol.symbol),
                    Err(e) => error!("保存失败 {}: {}", symbol.symbol, e),
                }
            } else {
                error!("JSON 序列化失败: {}", symbol.symbol);
            }
        }

        // 保存有效交易品种列表到 memory_backup_dir（作为一级资源）
        let paths = Paths::new();
        let symbols_list_path = format!("{}/symbols_list.json", paths.memory_backup_dir);
        let symbols_list = serde_json::json!({
            "有效交易品种": trading_symbols.iter().map(|s| s.symbol.clone()).collect::<Vec<_>>(),
            "更新时间戳": chrono::Utc::now().timestamp()
        });
        if let Ok(json_str) = serde_json::to_string_pretty(&symbols_list) {
            match std::fs::write(&symbols_list_path, json_str.as_bytes()) {
                Ok(_) => info!("已保存有效交易品种列表到 {}", symbols_list_path),
                Err(e) => error!("保存有效交易品种列表失败: {}", e),
            }
        } else {
            error!("序列化有效交易品种列表失败");
        }

        // 构建返回数据
        let mut rules = Vec::new();
        for symbol in trading_symbols {
            rules.push(SymbolRulesData {
                symbol: symbol.symbol.clone(),
                price_precision: symbol.pricePrecision.unwrap_or(8) as u8,
                quantity_precision: symbol.quantityPrecision.unwrap_or(8) as u8,
                tick_size: dec!(0.01),
                min_qty: dec!(0.000001),
                step_size: dec!(0.000001),
                min_notional: dec!(10),
                max_notional: dec!(1000000),
                leverage: 1,
                max_leverage: 20,
                maker_fee: dec!(0.0002),
                taker_fee: dec!(0.0005),
            });
        }

        Ok(rules)
    }

    /// 保存交易规则到 symbols_rules/{symbol}.json
    pub fn save_symbol_rules(&self, rules: &[SymbolRulesData]) -> Result<(), EngineError> {
        let paths = Paths::new();
        let base_dir = &paths.symbols_rules_dir;

        // 创建目录
        if let Err(e) = std::fs::create_dir_all(base_dir) {
            return Err(EngineError::Other(format!("创建目录失败: {}", e)));
        }

        let mut saved = 0;
        let mut failed = 0;

        for rule in rules {
            let file_path = format!("{}/{}.json", base_dir, rule.symbol.to_lowercase());
            let json_str = match serde_json::to_string_pretty(rule) {
                Ok(s) => s,
                Err(e) => {
                    failed += 1;
                    tracing::warn!("序列化规则失败 {}: {}", rule.symbol, e);
                    continue;
                }
            };

            if let Err(e) = std::fs::write(&file_path, json_str.as_bytes()) {
                failed += 1;
                tracing::warn!("保存规则失败 {}: {}", rule.symbol, e);
                continue;
            }
            saved += 1;
        }

        info!("已保存 {}/{} 个交易规则到 {} (失败: {})", saved, rules.len(), base_dir, failed);
        Ok(())
    }

    /// 从币安 API 获取账户信息
    ///
    /// # 返回
    /// * `BinanceAccountInfo` - 账户信息
    pub async fn fetch_account_info(&self) -> Result<BinanceAccountInfo, EngineError> {
        self.rate_limiter.acquire().await;

        let url = format!("{}/api/v3/account", self.market_api_base);
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
        self.rate_limiter.acquire().await;

        let url = format!("{}/api/v3/positionRisk", self.market_api_base);
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
        self.rate_limiter.acquire().await;

        let url = format!("{}/fapi/v1/leverageBracket", self.market_api_base);
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

    /// 从币安 USDT 合约 API 获取账户信息
    ///
    /// # 返回
    /// * `FuturesAccountResponse` - USDT 合约账户信息
    pub async fn fetch_futures_account(&self) -> Result<FuturesAccountResponse, EngineError> {
        self.rate_limiter.acquire().await;

        let url = format!("{}/fapi/v2/account", self.account_api_base);
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

        let account: FuturesAccountResponse = resp
            .json()
            .await
            .map_err(|e| EngineError::Other(format!("解析 JSON 失败: {}", e)))?;

        Ok(account)
    }

    /// 从币安 USDT 合约 API 获取持仓信息
    ///
    /// # 返回
    /// * `Vec<FuturesPositionResponse>` - USDT 合约持仓列表
    pub async fn fetch_futures_positions(&self) -> Result<Vec<FuturesPositionResponse>, EngineError> {
        self.rate_limiter.acquire().await;

        let url = format!("{}/fapi/v2/positionRisk", self.account_api_base);
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

        let positions: Vec<FuturesPositionResponse> = resp
            .json()
            .await
            .map_err(|e| EngineError::Other(format!("解析 JSON 失败: {}", e)))?;

        Ok(positions)
    }

    /// 设置持仓模式（双向持仓/单向持仓）
    ///
    /// # 参数
    /// * `dual_side_position` - true: 双向持仓模式, false: 单向持仓模式
    ///
    /// # 返回
    /// * `bool` - 设置是否成功
    pub async fn change_position_mode(&self, dual_side_position: bool) -> Result<bool, EngineError> {
        self.rate_limiter.acquire().await;

        let url = format!("{}/fapi/v1/positionMode", self.account_api_base);
        let params = serde_json::json!({
            "dualSidePosition": dual_side_position.to_string(),
            "recvWindow": 5000
        });

        let resp = self
            .client
            .post(&url)
            .json(&params)
            .send()
            .await
            .map_err(|e| EngineError::Other(format!("HTTP 请求失败: {}", e)))?;

        if !resp.status().is_success() {
            return Err(EngineError::Other(format!(
                "设置持仓模式失败: {}",
                resp.status()
            )));
        }

        tracing::info!(dual_side = dual_side_position, "持仓模式设置成功");
        Ok(true)
    }

    /// 设置交易对杠杆倍数
    ///
    /// # 参数
    /// * `symbol` - 交易对名称，如 "BTCUSDT"
    /// * `leverage` - 杠杆倍数 (1-125)
    ///
    /// # 返回
    /// * `bool` - 设置是否成功
    pub async fn change_leverage(&self, symbol: &str, leverage: i32) -> Result<bool, EngineError> {
        self.rate_limiter.acquire().await;

        let url = format!("{}/fapi/v1/leverage", self.account_api_base);
        let params = serde_json::json!({
            "symbol": symbol.to_uppercase(),
            "leverage": leverage,
            "recvWindow": 5000
        });

        let resp = self
            .client
            .post(&url)
            .json(&params)
            .send()
            .await
            .map_err(|e| EngineError::Other(format!("HTTP 请求失败: {}", e)))?;

        if !resp.status().is_success() {
            return Err(EngineError::Other(format!(
                "设置杠杆失败: {}",
                resp.status()
            )));
        }

        tracing::info!(symbol = symbol, leverage = leverage, "杠杆设置成功");
        Ok(true)
    }

    /// 获取交易手续费率
    ///
    /// # 参数
    /// * `symbol` - 交易对名称，如 "BTCUSDT"
    ///
    /// # 返回
    /// * `(maker_fee, taker_fee)` - (maker费率, taker费率)
    pub async fn get_commission_rate(&self, symbol: &str) -> Result<(Decimal, Decimal), EngineError> {
        self.rate_limiter.acquire().await;

        let url = format!("{}/fapi/v1/commissionRate", self.account_api_base);
        let params = serde_json::json!({
            "symbol": symbol.to_uppercase(),
            "recvWindow": 5000
        });

        let resp = self
            .client
            .get(&url)
            .query(&[
                ("symbol", symbol.to_uppercase()),
                ("recvWindow", "5000"),
            ])
            .send()
            .await
            .map_err(|e| EngineError::Other(format!("HTTP 请求失败: {}", e)))?;

        if !resp.status().is_success() {
            return Err(EngineError::Other(format!(
                "获取手续费率失败: {}",
                resp.status()
            )));
        }

        #[derive(Deserialize)]
        struct CommissionRateResponse {
            #[serde(rename = "makerCommissionRate")]
            maker_commission_rate: String,
            #[serde(rename = "takerCommissionRate")]
            taker_commission_rate: String,
        }

        let rate: CommissionRateResponse = resp
            .json()
            .await
            .map_err(|e| EngineError::Other(format!("解析 JSON 失败: {}", e)))?;

        let maker_fee = Decimal::from_str(&rate.maker_commission_rate).unwrap_or(dec!(0.0002));
        let taker_fee = Decimal::from_str(&rate.taker_commission_rate).unwrap_or(dec!(0.0004));

        Ok((maker_fee, taker_fee))
    }
}

impl Default for BinanceApiGateway {
    fn default() -> Self {
        Self::new()
    }
}

// 兼容性别名
pub type SymbolRulesFetcher = BinanceApiGateway;

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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinanceSymbol {
    pub symbol: String,
    pub status: String,
    #[serde(rename = "baseAsset")]
    pub baseAsset: String,
    #[serde(rename = "baseAssetPrecision")]
    pub baseAssetPrecision: Option<u32>,
    #[serde(rename = "quoteAsset")]
    pub quoteAsset: String,
    #[serde(rename = "quoteAssetPrecision")]
    pub quoteAssetPrecision: Option<u32>,
    #[serde(rename = "pricePrecision")]
    pub pricePrecision: Option<u32>,
    #[serde(rename = "quantityPrecision")]
    pub quantityPrecision: Option<u32>,
    pub filters: Vec<BinanceFilter>,
}

/// 币安过滤器
#[derive(Debug, Clone, Serialize, Deserialize)]
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
// USDT 合约账户 API 数据结构
// ============================================================================

/// USDT 合约账户信息响应
/// GET /fapi/v2/account
#[derive(Debug, Clone, Deserialize)]
pub struct FuturesAccountResponse {
    #[serde(rename = "totalMarginBalance")]
    pub total_margin_balance: String,
    #[serde(rename = "totalUnrealizedProfit")]
    pub total_unrealized_profit: String,
    #[serde(rename = "availableBalance")]
    pub available_balance: String,
    #[serde(rename = "totalMaintMargin")]
    pub total_maint_margin: String,
    #[serde(rename = "maxWithdrawAmount")]
    pub max_withdraw_amount: String,
    #[serde(rename = "updateTime")]
    pub update_time: i64,
    pub assets: Vec<FuturesAsset>,
}

/// USDT 合约资产信息
#[derive(Debug, Clone, Deserialize)]
pub struct FuturesAsset {
    pub asset: String,
    #[serde(rename = "marginBalance")]
    pub margin_balance: String,
    #[serde(rename = "unrealizedProfit")]
    pub unrealized_profit: String,
    #[serde(rename = "availableBalance")]
    pub available_balance: String,
}

// ============================================================================
// USDT 合约持仓 API 数据结构
// ============================================================================

/// USDT 合约持仓信息响应
/// GET /fapi/v2/positionRisk
#[derive(Debug, Clone, Deserialize)]
pub struct FuturesPositionResponse {
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
    pub unrealized_profit: String,
    pub leverage: String,
    #[serde(rename = "marginRatio")]
    pub margin_ratio: String,
}

// ============================================================================
// SymbolRulesData - 交易规则数据
// ============================================================================

/// 交易规则数据
///
/// 用于存储从 API 获取的交易对规则信息。
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
        assert_eq!(symbol.pricePrecision, Some(2));
        assert_eq!(symbol.filters.len(), 3);
    }

    #[test]
    fn test_fetcher_creation() {
        let fetcher = BinanceApiGateway::new();
        assert_eq!(fetcher.market_api_base, "https://api.binance.com");
        assert_eq!(fetcher.account_api_base, "https://api.binance.com");

        let test_fetcher = BinanceApiGateway::with_api_base("https://testnet.binance.vision");
        assert_eq!(test_fetcher.market_api_base, "https://testnet.binance.vision");
        assert_eq!(test_fetcher.account_api_base, "https://testnet.binance.vision");
    }

    #[test]
    fn test_new_futures() {
        let fetcher = BinanceApiGateway::new_futures();
        assert_eq!(fetcher.market_api_base, "https://fapi.binance.com");
        assert_eq!(fetcher.account_api_base, "https://fapi.binance.com");
    }

    #[test]
    fn test_new_futures_with_testnet() {
        let fetcher = BinanceApiGateway::new_futures_with_testnet();
        assert_eq!(fetcher.market_api_base, "https://fapi.binance.com"); // 实盘行情
        assert_eq!(fetcher.account_api_base, "https://testnet.binancefuture.com"); // 测试网账户
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

    #[test]
    fn test_rate_limiter() {
        let limiter = RateLimiter::new(10);
        assert_eq!(limiter.requests_per_minute, 10);
    }

    #[test]
    fn test_futures_account_response_deserialization() {
        let json = r#"{
            "totalMarginBalance": "10000.00",
            "totalUnrealizedProfit": "500.00",
            "availableBalance": "8000.00",
            "totalMaintMargin": "100.00",
            "maxWithdrawAmount": "8000.00",
            "updateTime": 1234567890,
            "assets": [
                {
                    "asset": "USDT",
                    "marginBalance": "10000.00",
                    "unrealizedProfit": "500.00",
                    "availableBalance": "8000.00"
                }
            ]
        }"#;

        let account: FuturesAccountResponse = serde_json::from_str(json).unwrap();
        assert_eq!(account.total_margin_balance, "10000.00");
        assert_eq!(account.total_unrealized_profit, "500.00");
        assert_eq!(account.available_balance, "8000.00");
        assert_eq!(account.assets.len(), 1);
        assert_eq!(account.assets[0].asset, "USDT");
    }

    #[test]
    fn test_futures_position_response_deserialization() {
        let json = r#"{
            "symbol": "BTCUSDT",
            "positionSide": "LONG",
            "positionAmt": "1.5",
            "entryPrice": "50000.00",
            "markPrice": "51000.00",
            "unrealizedProfit": "1500.00",
            "leverage": "10",
            "marginRatio": "0.02"
        }"#;

        let pos: FuturesPositionResponse = serde_json::from_str(json).unwrap();
        assert_eq!(pos.symbol, "BTCUSDT");
        assert_eq!(pos.position_side, "LONG");
        assert_eq!(pos.position_amt, "1.5");
        assert_eq!(pos.leverage, "10");
    }
}
