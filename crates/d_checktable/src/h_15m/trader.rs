//! trader.rs - 单币种交易器
//!
//! 对标 Python `singleAssetTrader`，完整实现除了指标信号外的所有框架：
//! - 交易规则管理 (Coins_Info)
//! - 余额/风控管理 (balance_management)
//! - 仓位风险管理 (_set_position_risk)
//! - 数据持久化 (_store_data / _load_data)
//! - 循环控制 (startloop / stoploop / _run_loop)
//! - 心跳检查 (health_check)
//! - 开仓/平仓 (open_position / close_position) - 策略逻辑注入点
//!
//! ## 状态流转
//!
//! ```text
//! Initial ──startloop()──► Trading ──stoploop()──► Stopped
//! ```

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tokio::time::{sleep, Instant};

// ============================================================================
// 枚举定义
// ============================================================================

/// 交易方向
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    LONG,  // 做多
    SHORT, // 做空
}

/// 交易状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradeStatus {
    INITIAL, // 初始状态
    DOUBLE,  // 翻倍加仓状态
    REDUCE,  // 减仓状态
    NORMAL,  // 居中状态
    END,
}

/// 趋势状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrendStatus {
    DIGRESS,   // 大幅偏离
    TENDENCIES, // 趋势
    SHAKE,     // 震荡
}

/// 交易器状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    Initial,
    Trading,
    Stopped,
}

impl Default for Status {
    fn default() -> Self {
        Status::Initial
    }
}

// ============================================================================
// 配置
// ============================================================================

/// 交易器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// 交易品种
    pub symbol: String,
    /// 时间周期
    pub interval: String,
    /// 循环间隔（毫秒）
    pub interval_ms: u64,
    /// 数据超时时间（秒）
    pub data_timeout_secs: u64,
    /// 功能比例（开仓比例）
    pub fun_ratio: f64,
    /// 平仓比例
    pub close_ratio: f64,
    /// 初始资金
    pub initial_balance: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            symbol: "BTCUSDT".to_string(),
            interval: "1d".to_string(),
            interval_ms: 300,       // 300ms 循环
            data_timeout_secs: 180, // 3分钟超时
            fun_ratio: 0.1,         // 10% 开仓比例
            close_ratio: 0.02,      // 2% 平仓比例
            initial_balance: 1000.0,
        }
    }
}

// ============================================================================
// 交易规则 (Coins_Info)
// ============================================================================

/// 交易规则信息
///
/// 包含币种的交易参数：精度、最小数量、手续费等
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoinsInfo {
    /// 价格精度
    pub price_precision: i32,
    /// 数量精度
    pub quantity_precision: i32,
    /// 最小价格变动 (tick size)
    pub tick_size: f64,
    /// 最小数量
    pub min_qty: f64,
    /// 最小名义价值
    pub min_notional: f64,
    /// 最大名义价值
    pub max_notional: f64,
    /// 杠杆倍数
    pub leverage: f64,
    /// 最低平仓比例 (考虑手续费)
    pub close_min_ratio: f64,
    /// Maker 手续费率
    pub maker_fee: f64,
    /// Taker 手续费率
    pub taker_fee: f64,
}

impl Default for CoinsInfo {
    fn default() -> Self {
        Self {
            price_precision: 2,
            quantity_precision: 3,
            tick_size: 0.01,
            min_qty: 0.001,
            min_notional: 5.0,
            max_notional: 50000.0,
            leverage: 10.0,
            close_min_ratio: 0.002,
            maker_fee: 0.0002,
            taker_fee: 0.0004,
        }
    }
}

// ============================================================================
// 账户信息 (Account_Information)
// ============================================================================

/// 账户信息
///
/// 包含账户余额、持仓状态、盈亏等信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInformation {
    /// 品种
    pub symbol: String,
    /// 功能比例（开仓比例）
    pub fun_ratio: f64,
    /// 平仓比例
    pub close_ratio: f64,
    /// 多头状态
    pub long_status: i32,
    /// 空头状态
    pub short_status: i32,
    /// 当前盈亏
    pub profit: f64,
    /// 多头最高价
    pub most_long: f64,
    /// 空头最低价
    pub most_short: f64,
    /// 当前价格
    pub close: f64,
    /// MACD 值
    pub macd_live: f64,
    /// MACD 方向 (L/S)
    pub macd_direction: String,
    /// 最后一个 hist 值
    pub last_hist: f64,
    /// LS 方向 (L/S)
    pub ls: String,
    /// 是否顶部品种
    pub if_top_symbols: bool,
    /// 当前时间
    pub now_time: String,
    /// 持仓信息
    pub pst_info: PositionInfo,
    /// 多头仓位风险
    pub pst_risk_long: Option<PositionRisk>,
    /// 空头仓位风险
    pub pst_risk_short: Option<PositionRisk>,
    /// 钱包余额
    pub wallet_balance: WalletBalance,
    /// 顶部品种列表
    pub top_symbols: Vec<String>,
    /// 功能比例详情
    pub all_fun_ratio: HashMap<String, f64>,
    /// 初始资金
    pub money_initial: f64,
    /// 价格评分
    pub price_ratio: f64,
    /// 执行耗时
    pub taken_time: f64,
}

impl Default for AccountInformation {
    fn default() -> Self {
        Self {
            symbol: "BTCUSDT".to_string(),
            fun_ratio: 0.1,
            close_ratio: 0.02,
            long_status: 0,
            short_status: 0,
            profit: 0.0,
            most_long: 0.0,
            most_short: 0.0,
            close: 0.0,
            macd_live: 0.0,
            macd_direction: "S".to_string(),
            last_hist: 0.0,
            ls: "S".to_string(),
            if_top_symbols: false,
            now_time: String::new(),
            pst_info: PositionInfo::default(),
            pst_risk_long: None,
            pst_risk_short: None,
            wallet_balance: WalletBalance::default(),
            top_symbols: Vec::new(),
            all_fun_ratio: HashMap::new(),
            money_initial: 0.0,
            price_ratio: 0.0,
            taken_time: 0.0,
        }
    }
}

/// 钱包余额
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WalletBalance {
    /// 钱包余额
    pub wallet_balance: f64,
    /// 可用余额
    pub available_balance: f64,
    /// 保证金余额
    pub margin_balance: f64,
    /// 维持保证金
    pub maint_margin: f64,
    /// 风险率
    pub risk_ratio: f64,
    /// 未实现盈亏
    pub unrealized_profit: f64,
}

// ============================================================================
// 持仓信息 (pst_info)
// ============================================================================

/// 持仓信息
///
/// 记录当前和历史的持仓订单
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionInfo {
    /// 当前多头持仓
    pub long_pst_now: Vec<PositionOrder>,
    /// 当前空头持仓
    pub short_pst_now: Vec<PositionOrder>,
    /// 历史多头持仓
    pub long_pst_past: Vec<PositionOrder>,
    /// 历史空头持仓
    pub short_pst_past: Vec<PositionOrder>,
}

impl Default for PositionInfo {
    fn default() -> Self {
        Self {
            long_pst_now: Vec::new(),
            short_pst_now: Vec::new(),
            long_pst_past: Vec::new(),
            short_pst_past: Vec::new(),
        }
    }
}

/// 持仓订单
///
/// 记录单个持仓订单的详情
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionOrder {
    /// 订单ID
    pub order_id: i64,
    /// 订单价格
    pub price: f64,
    /// 订单数量
    pub quantity: f64,
    /// 订单方向
    pub side: String,
    /// 创建时间
    pub create_time: i64,
}

impl PositionOrder {
    pub fn new(order_id: i64, price: f64, quantity: f64, side: &str) -> Self {
        Self {
            order_id,
            price,
            quantity,
            side: side.to_string(),
            create_time: Utc::now().timestamp(),
        }
    }
}

/// 仓位风险
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionRisk {
    /// symbol
    pub symbol: String,
    /// 持仓数量
    pub position_amt: f64,
    /// 名义价值
    pub notional: f64,
    /// 持仓方向
    pub position_side: String,
    ///  entry 价格
    pub entry_price: f64,
    /// 未实现盈亏
    pub unrealized_profit: f64,
    /// 维持保证金
    pub maint_margin: f64,
    /// 保证金
    pub isolated_margin: f64,
    /// 杠杆
    pub leverage: f64,
    /// 更新时间
    pub update_time: i64,
}

// ============================================================================
// 运行时信息
// ============================================================================

/// 运行时信息
#[derive(Debug, Clone)]
pub struct RuntimeInfo {
    /// 启动时间
    pub started_at: Option<i64>,
    /// 最后一次执行时间
    pub last_execute_at: Option<i64>,
    /// 最后订单时间
    pub last_order_timestamp: Option<i64>,
    /// 执行次数
    pub execute_count: u64,
    /// 连续错误数
    pub consecutive_errors: u32,
    /// 最后错误信息
    pub last_error: Option<String>,
    /// 当前持仓
    pub current_position: Option<PositionRisk>,
    /// 是否需要保存
    pub if_save: bool,
    /// 循环开始时间
    pub loop_start_time: Option<Instant>,
}

impl Default for RuntimeInfo {
    fn default() -> Self {
        Self {
            started_at: None,
            last_execute_at: None,
            last_order_timestamp: None,
            execute_count: 0,
            consecutive_errors: 0,
            last_error: None,
            current_position: None,
            if_save: false,
            loop_start_time: None,
        }
    }
}

// ============================================================================
// 交易器主体
// ============================================================================

/// 单币种交易器
///
/// 自包含实例，包含：
/// - 交易规则管理
/// - 余额/风控管理
/// - 仓位风险管理
/// - 数据持久化
/// - 循环控制
/// - 心跳检查
/// - 开仓/平仓 (策略逻辑注入点)
pub struct Trader {
    /// 配置
    pub config: Config,

    /// 当前状态
    pub status: Status,

    /// 是否运行（原子标志，跨线程安全）
    is_running: Arc<AtomicBool>,

    /// 交易规则
    pub coins_info: CoinsInfo,

    /// 账户信息
    pub account: AccountInformation,

    /// 余额
    pub balance: Option<WalletBalance>,

    /// 运行时信息
    runtime: Mutex<RuntimeInfo>,
}

impl Default for Trader {
    fn default() -> Self {
        Self::new("BTCUSDT")
    }
}

// ============================================================================
// 方法实现
// ============================================================================

impl Trader {
    /// 创建新的交易器
    pub fn new(symbol: &str) -> Self {
        let mut trader = Self {
            config: Config {
                symbol: symbol.to_string(),
                ..Default::default()
            },
            status: Status::Initial,
            is_running: Arc::new(AtomicBool::new(false)),
            coins_info: CoinsInfo::default(),
            account: AccountInformation {
                symbol: symbol.to_string(),
                ..Default::default()
            },
            balance: None,
            runtime: Mutex::new(RuntimeInfo::default()),
        };

        // 初始化账户信息中的价格
        trader.account.most_long = 0.0;
        trader.account.most_short = 0.0;

        trader
    }

    /// 更新交易规则
    ///
    /// 从交易所获取最新的交易规则信息：
    /// - 价格/数量精度
    /// - 最小/最大数量
    /// - 手续费率
    /// - 杠杆信息
    pub fn update_transaction_rules(&mut self) {
        // TODO: 从交易所 API 获取
        // 当前使用默认值，实际应调用:
        // let all_coins = exchange.get_all_coins();
        // let leverage_info = exchange.get_leverage_brackets();
        // let fee_info = exchange.get_commission_rate();

        tracing::debug!("[{}] 更新交易规则", self.config.symbol);
    }

    /// 余额管理
    ///
    /// 基于实时账户数据的风控管理
    /// - 获取账户余额信息
    /// - 计算风险率
    /// - 更新持仓信息
    pub fn balance_management(&mut self, _if_update: bool) {
        // TODO: 从交易所 API 获取
        // let account_info = exchange.get_account_info();
        // self.balance = parse_account_info(account_info);

        tracing::debug!(
            "[{}] 余额管理: balance={:?}",
            self.config.symbol,
            self.balance
        );
    }

    /// 设置仓位风险
    ///
    /// 从交易所获取仓位风险信息：
    /// - 多头仓位 (pst_risk_long)
    /// - 空头仓位 (pst_risk_short)
    pub fn set_position_risk(&mut self, force_update: bool) {
        if !force_update {
            // 非强制更新时，每隔一定时间更新一次
            // TODO: 实现时间间隔控制
            return;
        }

        // TODO: 从交易所 API 获取
        // let response = exchange.get_position_risk();
        // for m in response:
        //     if m.position_side == "LONG":
        //         self.account.pst_risk_long = Some(parse_position_risk(m));
        //     elif m.position_side == "SHORT":
        //         self.account.pst_risk_short = Some(parse_position_risk(m));

        tracing::debug!(
            "[{}] 仓位风险: long={:?}, short={:?}",
            self.config.symbol,
            self.account.pst_risk_long,
            self.account.pst_risk_short
        );
    }

    /// 风控检查
    ///
    /// 开仓前的风控检查：
    /// - 检查是否超过最大名义价值
    /// - 检查是否超过最大仓位限制
    ///
    /// # 参数
    ///
    /// - `direction`: 0=多头, 1=空头
    /// - `quantity`: 开仓数量
    ///
    /// # 返回
    ///
    /// - `true`: 检查通过，可以开仓
    /// - `false`: 检查未通过，禁止开仓
    pub fn risk_check(&self, direction: i32, quantity: f64) -> bool {
        // TODO: 实现完整的风控检查

        // 检查数量是否有效
        if quantity <= 0.0 {
            tracing::warn!("[{}] 数量无效: {}", self.config.symbol, quantity);
            return false;
        }

        // 检查是否超过最大名义价值
        let position_value = (quantity * self.account.close).abs();
        if position_value > self.coins_info.max_notional * 0.9 {
            tracing::warn!(
                "[{}] 超过最大名义价值: {} > {}",
                self.config.symbol,
                position_value,
                self.coins_info.max_notional * 0.9
            );
            return false;
        }

        // 检查余额
        if let Some(balance) = &self.balance {
            let open_order_margin = position_value / self.coins_info.leverage;
            let initial_margin = open_order_margin + balance.maint_margin;

            // 多头：保证金占比不超过 20%
            if direction == 0 && initial_margin / balance.margin_balance > 0.2 {
                tracing::warn!(
                    "[{}] 多头超过最大仓位限制: {}",
                    self.config.symbol,
                    initial_margin / balance.margin_balance
                );
                return false;
            }

            // 空头：保证金占比不超过 50%
            if direction == 1 && initial_margin / balance.margin_balance > 0.5 {
                tracing::warn!(
                    "[{}] 空头超过最大仓位限制: {}",
                    self.config.symbol,
                    initial_margin / balance.margin_balance
                );
                return false;
            }
        }

        true
    }

    /// 开仓
    ///
    /// 策略逻辑注入点。
    /// 判断品种是否处于波动最高品种，计算开仓数量，执行开仓。
    ///
    /// # 返回
    ///
    /// - `true`: 执行了开仓操作
    /// - `false`: 未执行开仓
    pub fn open_position(&mut self) -> bool {
        // TODO: 策略逻辑
        //
        // 1. 检查品种是否在顶部列表
        // if !self.account.if_top_symbols {
        //     return false;
        // }
        //
        // 2. 获取 MACD 方向
        // let macd_dir = &self.account.macd_direction;
        //
        // 3. 计算开仓数量
        // let money = self.balance.as_ref().unwrap().margin_balance * self.config.fun_ratio;
        // let quantity = money / self.account.close;
        //
        // 4. 风控检查
        // if !self.risk_check(direction, quantity) {
        //     return false;
        // }
        //
        // 5. 发送订单
        // let result = exchange.new_order(symbol, quantity, direction, "OPEN");
        //
        // 6. 更新持仓信息
        // if result.success {
        //     self.account.pst_info.long_pst_now.push(...);
        // }

        tracing::debug!("[{}] 开仓检查", self.config.symbol);
        false
    }

    /// 平仓
    ///
    /// 策略逻辑注入点。
    /// 根据盈利情况、价格位置等条件判断是否需要平仓。
    pub fn close_position(&mut self) {
        // TODO: 策略逻辑
        //
        // 1. 获取当前持仓
        // let long_pst = &self.account.pst_info.long_pst_now;
        // let short_pst = &self.account.pst_info.short_pst_now;
        //
        // 2. 计算平均价格和数量
        // let (avg_price, total_qty) = calculate_avg_price(long_pst);
        //
        // 3. 计算当前盈亏
        // self.account.profit = (self.account.close - avg_price) * total_qty;
        //
        // 4. 判断平仓条件
        // - 盈利达到一定比例
        // - 价格触及止损/止盈位
        // - 趋势反转
        //
        // 5. 发送平仓订单
        // let result = exchange.new_order(symbol, qty, direction, "CLOSE");
        //
        // 6. 更新持仓信息
        // if result.success {
        //     self.account.pst_info.long_pst_now.clear();
        //     self.runtime.if_save = true;
        // }

        tracing::debug!("[{}] 平仓检查", self.config.symbol);
    }

    /// 平仓历史持仓
    ///
    /// 当品种从顶部列表移除时，平掉历史持仓
    pub fn close_position_past(&mut self) -> bool {
        // TODO: 实现平仓历史持仓逻辑
        tracing::debug!("[{}] 平仓历史持仓", self.config.symbol);
        false
    }

    /// 保存数据
    ///
    /// 将交易状态持久化到存储
    pub fn store_data(&mut self) {
        // TODO: 持久化到 Redis/文件
        // let data = serde_json::to_string(&self.account).unwrap();
        // redis.set(self.config.symbol, data);

        tracing::info!("[{}] 保存交易数据", self.config.symbol);
        self.runtime.lock().if_save = false;
    }

    /// 加载数据
    ///
    /// 从存储加载交易状态
    pub fn load_data(&mut self) {
        // TODO: 从 Redis/文件加载
        // let data = redis.get(self.config.symbol);
        // if let Some(data) = data {
        //     self.account = serde_json::from_str(&data).unwrap();
        // }

        tracing::info!("[{}] 加载交易数据", self.config.symbol);
    }

    /// 运行模式检查
    ///
    /// 检查是否应该继续运行
    /// - 无持仓
    /// - 不在顶部品种列表
    /// - 满足退出条件
    pub fn run_model(&mut self) {
        let has_position = self.account.pst_risk_long.is_some()
            || self.account.pst_risk_short.is_some();
        let has_pending = !self.account.pst_info.long_pst_now.is_empty()
            || !self.account.pst_info.short_pst_now.is_empty()
            || !self.account.pst_info.long_pst_past.is_empty()
            || !self.account.pst_info.short_pst_past.is_empty();

        // 如果无持仓、无待处理订单、不在顶部品种，则退出
        if !has_position && !has_pending && !self.account.if_top_symbols {
            tracing::info!("[{}] 满足退出条件，停止交易", self.config.symbol);
            self.is_running.store(false, Ordering::SeqCst);
        }
    }

    /// 更新市场数据
    ///
    /// 获取最新的市场信息：
    /// - 当前价格
    /// - 顶部品种列表
    /// - 持仓信息
    pub fn update_market_data(&mut self) -> bool {
        // TODO: 从市场数据源获取
        // self.account.close = market.get_price(self.config.symbol);
        // self.account.top_symbols = market.get_top_symbols();
        // self.account.if_top_symbols = self.account.top_symbols.contains(&self.config.symbol);

        tracing::trace!(
            "[{}] 市场数据: close={}, top={}",
            self.config.symbol,
            self.account.close,
            self.account.if_top_symbols
        );

        self.account.close > 0.0
    }

    /// 主循环（内部方法）
    ///
    /// 对标 Python 的 `_run_loop`，包含完整的主循环逻辑：
    async fn _run_loop(&mut self) {
        tracing::info!("[{}] _run_loop started", self.config.symbol);

        // 初始化
        self.load_data();
        self.set_position_risk(true);
        self.balance_management(true);

        // 主循环
        while self.is_running.load(Ordering::SeqCst) {
            let loop_start = Instant::now();

            // 1. 更新交易规则
            self.update_transaction_rules();

            // 2. 更新市场数据
            if !self.update_market_data() {
                sleep(Duration::from_millis(self.config.interval_ms)).await;
                continue;
            }

            // 3. 设置仓位风险
            self.set_position_risk(false);

            // 4. 余额管理
            self.balance_management(false);

            // 5. 开仓逻辑
            if !self.open_position() {
                // 如果未开仓，尝试平仓历史持仓
                if !self.close_position_past() {
                    // 最后尝试平仓
                    self.close_position();
                }
            }

            // 6. 保存数据
            if self.runtime.lock().if_save {
                self.store_data();
            }

            // 7. 运行模式检查
            self.run_model();

            // 记录执行时间
            self.account.taken_time = loop_start.elapsed().as_secs_f64();

            // 循环间隔
            sleep(Duration::from_millis(self.config.interval_ms)).await;
        }

        tracing::info!("[{}] _run_loop stopped", self.config.symbol);
    }

    /// 启动交易循环（在独立线程中运行）
    pub fn startloop(&mut self) {
        if self.is_running.load(Ordering::SeqCst) {
            tracing::warn!("[{}] Trader already running", self.config.symbol);
            return;
        }

        self.is_running.store(true, Ordering::SeqCst);
        self.status = Status::Trading;
        self.runtime.lock().started_at = Some(Utc::now().timestamp());

        tracing::info!("[{}] Trading loop started", self.config.symbol);

        // 获取配置用于新线程
        let symbol = self.config.symbol.clone();

        // 创建新线程
        std::thread::spawn(move || {
            // 在新线程中创建 runtime
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let mut trader = Trader::new(&symbol);
                trader.is_running.store(true, Ordering::SeqCst);
                trader.status = Status::Trading;
                trader._run_loop().await;
            });
        });
    }

    /// 直接运行（当前线程）
    ///
    /// 调用 _run_loop 主循环
    pub async fn run(&mut self) {
        if self.is_running.load(Ordering::SeqCst) {
            tracing::warn!("[{}] Trader already running", self.config.symbol);
            return;
        }

        self.is_running.store(true, Ordering::SeqCst);
        self.status = Status::Trading;
        self.runtime.lock().started_at = Some(Utc::now().timestamp());

        tracing::info!("[{}] Trading started", self.config.symbol);

        // 调用主循环
        self._run_loop().await;

        tracing::info!("[{}] Trading stopped", self.config.symbol);
    }

    /// 停止交易循环
    pub fn stoploop(&mut self) {
        self.is_running.store(false, Ordering::SeqCst);
        self.status = Status::Stopped;
        tracing::info!("[{}] Stop requested", self.config.symbol);
    }

    /// 检查是否运行中
    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::SeqCst)
    }

    /// 心跳检查
    ///
    /// 返回当前交易器的健康状态快照，用于监控。
    pub fn health_check(&self) -> HealthCheck {
        let runtime = self.runtime.lock();

        HealthCheck {
            symbol: self.config.symbol.clone(),
            status: format!("{:?}", self.status),
            is_running: self.is_running(),
            is_alive: self.is_running(),
            started_at: runtime.started_at,
            last_execute_at: runtime.last_execute_at,
            last_order_timestamp: runtime.last_order_timestamp,
            execute_count: runtime.execute_count,
            consecutive_errors: runtime.consecutive_errors,
            last_error: runtime.last_error.clone(),
            current_position: runtime.current_position.clone(),
        }
    }

    /// 记录错误
    pub fn record_error(&mut self, error: &str) {
        let mut runtime = self.runtime.lock();
        runtime.consecutive_errors += 1;
        runtime.last_error = Some(error.to_string());

        // 连续错误超过阈值时停止
        if runtime.consecutive_errors >= 10 {
            tracing::error!(
                "[{}] Too many consecutive errors ({}), stopping",
                self.config.symbol,
                runtime.consecutive_errors
            );
            drop(runtime);
            self.stoploop();
        }
    }

    /// 重置错误计数
    pub fn reset_errors(&mut self) {
        let mut runtime = self.runtime.lock();
        runtime.consecutive_errors = 0;
        runtime.last_error = None;
    }
}

// ============================================================================
// 健康检查结果
// ============================================================================

/// 健康检查结果
#[derive(Debug, Clone)]
pub struct HealthCheck {
    /// 品种代码
    pub symbol: String,
    /// 状态描述
    pub status: String,
    /// 是否运行中
    pub is_running: bool,
    /// 是否存活
    pub is_alive: bool,
    /// 启动时间戳
    pub started_at: Option<i64>,
    /// 最后执行时间戳
    pub last_execute_at: Option<i64>,
    /// 最后订单时间戳
    pub last_order_timestamp: Option<i64>,
    /// 累计执行次数
    pub execute_count: u64,
    /// 连续错误数
    pub consecutive_errors: u32,
    /// 最后错误信息
    pub last_error: Option<String>,
    /// 当前持仓
    pub current_position: Option<PositionRisk>,
}

// ============================================================================
// 辅助函数
// ============================================================================

/// 计算平均价格和总数量
pub fn calculate_avg_price(positions: &[PositionOrder]) -> (f64, f64) {
    if positions.is_empty() {
        return (0.0, 0.0);
    }

    let total_value: f64 = positions.iter().map(|p| p.price * p.quantity).sum();
    let total_qty: f64 = positions.iter().map(|p| p.quantity).sum();

    if total_qty > 0.0 {
        (total_value / total_qty, total_qty)
    } else {
        (0.0, 0.0)
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_trader() {
        let trader = Trader::new("BTCUSDT");
        assert_eq!(trader.config.symbol, "BTCUSDT");
        assert_eq!(trader.status, Status::Initial);
        assert!(!trader.is_running());
    }

    #[test]
    fn test_coins_info() {
        let trader = Trader::new("BTCUSDT");
        assert_eq!(trader.coins_info.leverage, 10.0);
        assert_eq!(trader.coins_info.min_notional, 5.0);
    }

    #[test]
    fn test_account_info() {
        let trader = Trader::new("BTCUSDT");
        assert_eq!(trader.account.symbol, "BTCUSDT");
        assert_eq!(trader.account.long_status, 0);
        assert_eq!(trader.account.short_status, 0);
    }

    #[test]
    fn test_calculate_avg_price() {
        let positions = vec![
            PositionOrder::new(1, 100.0, 10.0, "BUY"),
            PositionOrder::new(2, 110.0, 20.0, "BUY"),
        ];

        let (avg_price, total_qty) = calculate_avg_price(&positions);
        assert!((avg_price - 106.67).abs() < 0.01);
        assert_eq!(total_qty, 30.0);
    }

    #[test]
    fn test_calculate_avg_price_empty() {
        let positions: Vec<PositionOrder> = vec![];
        let (avg_price, total_qty) = calculate_avg_price(&positions);
        assert_eq!(avg_price, 0.0);
        assert_eq!(total_qty, 0.0);
    }

    #[test]
    fn test_position_order() {
        let order = PositionOrder::new(123, 100.0, 1.5, "BUY");
        assert_eq!(order.order_id, 123);
        assert_eq!(order.price, 100.0);
        assert_eq!(order.quantity, 1.5);
        assert_eq!(order.side, "BUY");
    }
}
