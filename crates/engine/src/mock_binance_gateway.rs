use crate::EngineError;
use crate::account_pool::AccountPool;
use crate::margin_config::StrategyLevel;
use crate::minute_risk::calculate_minute_open_notional;
use crate::position_manager::{Direction, LocalPositionManager};
use crate::strategy_pool::StrategyPool;
use crate::sqlite_persistence::{AccountSnapshotRecord, EventRecorder, ExchangePositionRecord, RiskEventRecord, format_decimal};
use fnv::FnvHashMap;
use parking_lot::RwLock;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use strategy::types::{OrderRequest, OrderType, Side};
use tracing::{info, warn};

/// 模拟币安网关错误类型
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum RejectReason {
    InsufficientBalance,
    PositionLimitExceeded,
    MarginInsufficient,
    PriceDeviationExceeded,
    SymbolNotTradable,
    OrderFrequencyExceeded,
    SystemError,
}

impl std::fmt::Display for RejectReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RejectReason::InsufficientBalance => write!(f, "INSUFFICIENT_BALANCE"),
            RejectReason::PositionLimitExceeded => write!(f, "POSITION_LIMIT_EXCEEDED"),
            RejectReason::MarginInsufficient => write!(f, "MARGIN_INSUFFICIENT"),
            RejectReason::PriceDeviationExceeded => write!(f, "PRICE_DEVIATION_EXCEEDED"),
            RejectReason::SymbolNotTradable => write!(f, "SYMBOL_NOT_TRADABLE"),
            RejectReason::OrderFrequencyExceeded => write!(f, "ORDER_FREQUENCY_EXCEEDED"),
            RejectReason::SystemError => write!(f, "SYSTEM_ERROR"),
        }
    }
}

/// 模拟账户
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockAccount {
    pub account_id: String,
    pub total_equity: Decimal,
    pub available: Decimal,
    pub frozen_margin: Decimal,
    pub unrealized_pnl: Decimal,
    pub update_ts: i64,
}

impl MockAccount {
    pub fn new(account_id: String, initial_balance: Decimal) -> Self {
        Self {
            account_id,
            total_equity: initial_balance,
            available: initial_balance,
            frozen_margin: Decimal::ZERO,
            unrealized_pnl: Decimal::ZERO,
            update_ts: current_timestamp(),
        }
    }

    pub fn margin_ratio(&self) -> Decimal {
        if self.total_equity.is_zero() {
            return Decimal::ZERO;
        }
        // 保证金率 = 冻结保证金 / 总权益
        self.frozen_margin / self.total_equity
    }
}

/// 模拟持仓
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockPosition {
    pub symbol: String,
    pub long_qty: Decimal,
    pub long_avg_price: Decimal,
    pub short_qty: Decimal,
    pub short_avg_price: Decimal,
    pub unrealized_pnl: Decimal,
    pub margin_used: Decimal,
}

impl MockPosition {
    pub fn new(symbol: String) -> Self {
        Self {
            symbol,
            long_qty: Decimal::ZERO,
            long_avg_price: Decimal::ZERO,
            short_qty: Decimal::ZERO,
            short_avg_price: Decimal::ZERO,
            unrealized_pnl: Decimal::ZERO,
            margin_used: Decimal::ZERO,
        }
    }

    pub fn total_qty(&self) -> Decimal {
        self.long_qty + self.short_qty
    }

    pub fn net_direction(&self) -> Option<Direction> {
        if self.long_qty > Decimal::ZERO && self.short_qty == Decimal::ZERO {
            Some(Direction::Long)
        } else if self.short_qty > Decimal::ZERO && self.long_qty == Decimal::ZERO {
            Some(Direction::Short)
        } else if self.long_qty > Decimal::ZERO && self.short_qty > Decimal::ZERO {
            // 同时有多空，按净头寸判断
            if self.long_qty > self.short_qty {
                Some(Direction::Long)
            } else {
                Some(Direction::Short)
            }
        } else {
            None
        }
    }

    /// 计算未实现盈亏
    pub fn calc_unrealized_pnl(&mut self, current_price: Decimal) {
        // 多头未实现盈亏 = (当前价 - 多头均价) * 多头数量
        let long_pnl = if self.long_qty > Decimal::ZERO {
            (current_price - self.long_avg_price) * self.long_qty
        } else {
            Decimal::ZERO
        };

        // 空头未实现盈亏 = (空头均价 - 当前价) * 空头数量
        let short_pnl = if self.short_qty > Decimal::ZERO {
            (self.short_avg_price - current_price) * self.short_qty
        } else {
            Decimal::ZERO
        };

        self.unrealized_pnl = long_pnl + short_pnl;
    }
}

/// 模拟订单
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockOrder {
    pub order_id: String,
    pub symbol: String,
    pub side: Side,
    pub qty: Decimal,
    pub price: Decimal,
    pub order_type: OrderType,
    pub status: OrderStatus,
    pub filled_qty: Decimal,
    pub filled_price: Decimal,
    pub created_ts: i64,
    pub filled_ts: Option<i64>,
    pub reject_reason: Option<RejectReason>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderStatus {
    Pending,
    Filled,
    Cancelled,
    Rejected,
}

/// 成交记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockTrade {
    pub trade_id: String,
    pub order_id: String,
    pub symbol: String,
    pub side: Side,
    pub qty: Decimal,
    pub price: Decimal,
    pub commission: Decimal,
    pub realized_pnl: Decimal,
    pub ts: i64,
}

/// 风控配置
#[derive(Debug, Clone)]
pub struct RiskConfig {
    /// 最大持仓比例 (默认 95%)
    pub max_position_ratio: Decimal,
    /// 最低保留比例 (默认 5%)
    pub min_reserve_ratio: Decimal,
    /// 最大订单金额比例 (默认 10%)
    pub max_order_value_ratio: Decimal,
    /// 维持保证金率 (USDT永续合约 0.5%)
    pub maintenance_margin_rate: Decimal,
    /// 订单频率限制 (次/秒)
    pub order_frequency_limit: u32,
    /// 价格偏离限制 (默认 1%)
    pub price_deviation_limit: Decimal,
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            max_position_ratio: dec!(0.95),
            min_reserve_ratio: dec!(0.05),
            max_order_value_ratio: dec!(0.10),
            maintenance_margin_rate: dec!(0.005), // 0.5%
            order_frequency_limit: 10,
            price_deviation_limit: dec!(0.01), // 1%
        }
    }
}

impl RiskConfig {
    pub fn production() -> Self {
        Self::default()
    }
}

/// 订单结果
#[derive(Debug, Clone)]
pub struct OrderResult {
    pub order_id: String,
    pub status: OrderStatus,
    pub filled_qty: Decimal,
    pub filled_price: Decimal,
    pub commission: Decimal,
    pub reject_reason: Option<RejectReason>,
    pub message: String,
}

/// CSV写入器
#[derive(Debug, Clone)]
pub struct CsvWriter {
    pub trades_path: String,
    pub positions_path: String,
    pub risk_log_path: String,
    pub account_snapshot_path: String,
    pub indicator_comparison_path: String,
}

impl Default for CsvWriter {
    fn default() -> Self {
        Self {
            trades_path: "trades.csv".to_string(),
            positions_path: "positions.csv".to_string(),
            risk_log_path: "risk_log.csv".to_string(),
            account_snapshot_path: "account_snapshot.csv".to_string(),
            indicator_comparison_path: "indicator_comparison.csv".to_string(),
        }
    }
}

/// MockBinanceGateway - 模拟币安网关
pub struct MockBinanceGateway {
    account: RwLock<MockAccount>,
    positions: RwLock<FnvHashMap<String, MockPosition>>,
    orders: RwLock<FnvHashMap<String, MockOrder>>,
    trades: RwLock<Vec<MockTrade>>,
    risk_config: RiskConfig,
    csv_writer: CsvWriter,
    // 事件记录器 (可选)
    event_recorder: Option<Arc<dyn EventRecorder>>,
    // 内部持仓管理器 (用于实际持仓计算)
    position_manager: LocalPositionManager,
    // 账户池 (用于风控)
    account_pool: Arc<AccountPool>,
    // 策略池
    strategy_pool: Arc<StrategyPool>,
    // 订单计数器 (用于频率限制)
    order_count_last_second: RwLock<Vec<(u64, u32)>>,
    // 下一个订单ID
    next_order_id: RwLock<u64>,
    // 下一个成交ID
    next_trade_id: RwLock<u64>,
}

impl MockBinanceGateway {
    /// 创建新的 MockBinanceGateway
    pub fn new(
        initial_balance: Decimal,
        risk_config: RiskConfig,
        csv_writer: CsvWriter,
    ) -> Self {
        let account_pool = Arc::new(AccountPool::with_config(
            initial_balance,
            dec!(0.20),
            dec!(0.10),
        ));
        let strategy_pool = Arc::new(StrategyPool::new());

        Self {
            account: RwLock::new(MockAccount::new("mock_account_001".to_string(), initial_balance)),
            positions: RwLock::new(FnvHashMap::default()),
            orders: RwLock::new(FnvHashMap::default()),
            trades: RwLock::new(Vec::new()),
            risk_config,
            csv_writer,
            event_recorder: None,
            position_manager: LocalPositionManager::new(),
            account_pool,
            strategy_pool,
            order_count_last_second: RwLock::new(Vec::new()),
            next_order_id: RwLock::new(1),
            next_trade_id: RwLock::new(1),
        }
    }

    /// 设置事件记录器
    pub fn set_event_recorder(&mut self, recorder: Arc<dyn EventRecorder>) {
        self.event_recorder = Some(recorder);
    }

    /// 下单
    pub fn place_order(&self, req: OrderRequest) -> Result<OrderResult, EngineError> {
        let order_id = self.generate_order_id();
        let current_ts = current_timestamp();

        // 1. 订单频率检查
        if !self.check_order_frequency(current_ts as u64) {
            return Ok(self.reject_order(
                order_id.clone(),
                req.clone(),
                RejectReason::OrderFrequencyExceeded,
                "订单频率超过限制".to_string(),
            ));
        }

        // 2. 获取当前价格
        let current_price = req.price.unwrap_or(req.qty); // 如果没指定价格，用qty作为占位
        let order_value = req.qty * current_price;

        // 3. 风控检查
        if let Some(reason) = self.pre_risk_check(&req, order_value, current_price) {
            let reason_str = reason.to_string();
            self.log_risk_event(
                &order_id,
                &req.symbol,
                "REJECT",
                &reason_str,
                self.account.read().available,
                self.account.read().margin_ratio(),
            );
            // 记录风控事件到 SQLite
            self.record_risk_event_internal(
                "REJECT",
                &req.symbol,
                &order_id,
                &reason_str,
                "ORDER_REJECTED",
                &format!("订单被拒绝: {}", reason_str),
            );
            return Ok(self.reject_order(order_id, req, reason, format!("风控拒绝: {}", reason_str)));
        }

        // 4. 执行订单 (市价单立即成交)
        let (filled_price, filled_qty) = match req.order_type {
            OrderType::Market => (current_price, req.qty),
            OrderType::Limit => {
                // 限价单简化处理：如果指定价格，按指定价格成交
                (req.price.unwrap_or(current_price), req.qty)
            }
        };

        // 5. 成交处理
        let result = self.execute_fill(&order_id, &req, filled_price, filled_qty, current_ts);

        Ok(result)
    }

    /// 预风控检查
    fn pre_risk_check(
        &self,
        req: &OrderRequest,
        order_value: Decimal,
        current_price: Decimal,
    ) -> Option<RejectReason> {
        let account = self.account.read();

        // 1. 检查账户余额
        if account.available < order_value {
            warn!("余额不足: 可用 {} < 订单金额 {}", account.available, order_value);
            return Some(RejectReason::InsufficientBalance);
        }

        // 2. 检查持仓限制
        let current_position_value = self.get_position_value(&req.symbol, current_price);
        let max_position_value = account.total_equity * self.risk_config.max_position_ratio;

        if current_position_value + order_value > max_position_value {
            warn!(
                "持仓超限: 当前 {} + 订单 {} > 限制 {}",
                current_position_value, order_value, max_position_value
            );
            return Some(RejectReason::PositionLimitExceeded);
        }

        // 3. 检查保证金率
        let new_frozen_margin = account.frozen_margin + order_value;
        let new_total_equity = account.total_equity; // 假设总权益不变

        if !new_total_equity.is_zero() {
            let new_margin_ratio = new_frozen_margin / new_total_equity;
            if new_margin_ratio > dec!(0.95) {
                warn!(
                    "保证金率过高: {} > 95%",
                    new_margin_ratio
                );
                return Some(RejectReason::MarginInsufficient);
            }
        }

        // 4. 检查最大订单金额
        let max_order_value = account.total_equity * self.risk_config.max_order_value_ratio;
        if order_value > max_order_value {
            warn!(
                "订单金额超限: {} > 最大 {}",
                order_value, max_order_value
            );
            return Some(RejectReason::PositionLimitExceeded);
        }

        None
    }

    /// 执行成交
    fn execute_fill(
        &self,
        order_id: &str,
        req: &OrderRequest,
        filled_price: Decimal,
        filled_qty: Decimal,
        ts: i64,
    ) -> OrderResult {
        let mut account = self.account.write();
        let mut positions = self.positions.write();

        // 计算手续费 (Taker 0.04%)
        let commission = filled_qty * filled_price * dec!(0.0004);

        // 更新账户
        let order_value = filled_qty * filled_price;
        account.available -= order_value;
        account.frozen_margin += order_value;
        account.update_ts = ts;

        // 更新或创建持仓
        let position = positions.entry(req.symbol.clone()).or_insert_with(|| {
            MockPosition::new(req.symbol.clone())
        });

        let direction = match req.side {
            Side::Long => Direction::Long,
            Side::Short => Direction::Short,
        };

        // 更新持仓数量和均价
        match direction {
            Direction::Long => {
                let total_cost = position.long_qty * position.long_avg_price + filled_qty * filled_price;
                let total_qty = position.long_qty + filled_qty;
                position.long_avg_price = if total_qty > Decimal::ZERO {
                    total_cost / total_qty
                } else {
                    Decimal::ZERO
                };
                position.long_qty = total_qty;
            }
            Direction::Short => {
                let total_cost = position.short_qty * position.short_avg_price + filled_qty * filled_price;
                let total_qty = position.short_qty + filled_qty;
                position.short_avg_price = if total_qty > Decimal::ZERO {
                    total_cost / total_qty
                } else {
                    Decimal::ZERO
                };
                position.short_qty = total_qty;
            }
        }

        position.margin_used = order_value;
        position.calc_unrealized_pnl(filled_price);

        // 创建成交记录
        let trade_id = self.generate_trade_id();
        let trade = MockTrade {
            trade_id: trade_id.clone(),
            order_id: order_id.to_string(),
            symbol: req.symbol.clone(),
            side: req.side,
            qty: filled_qty,
            price: filled_price,
            commission,
            realized_pnl: Decimal::ZERO,
            ts,
        };

        self.trades.write().push(trade);

        // 创建订单记录
        let order = MockOrder {
            order_id: order_id.to_string(),
            symbol: req.symbol.clone(),
            side: req.side,
            qty: req.qty,
            price: filled_price,
            order_type: req.order_type,
            status: OrderStatus::Filled,
            filled_qty,
            filled_price,
            created_ts: ts,
            filled_ts: Some(ts),
            reject_reason: None,
        };

        self.orders.write().insert(order_id.to_string(), order);

        info!(
            "订单成交: {} {:?} {}@{} 手续费:{}",
            order_id, req.side, filled_qty, filled_price, commission
        );

        // 记录事件到 SQLite (仓位变动时)
        self.record_position_change(&account, &positions, &req.symbol, ts);

        OrderResult {
            order_id: order_id.to_string(),
            status: OrderStatus::Filled,
            filled_qty,
            filled_price,
            commission,
            reject_reason: None,
            message: "成交成功".to_string(),
        }
    }

    /// 拒绝订单
    fn reject_order(
        &self,
        order_id: String,
        req: OrderRequest,
        reason: RejectReason,
        message: String,
    ) -> OrderResult {
        let ts = current_timestamp();

        let order = MockOrder {
            order_id: order_id.clone(),
            symbol: req.symbol,
            side: req.side,
            qty: req.qty,
            price: req.price.unwrap_or(Decimal::ZERO),
            order_type: req.order_type,
            status: OrderStatus::Rejected,
            filled_qty: Decimal::ZERO,
            filled_price: Decimal::ZERO,
            created_ts: ts,
            filled_ts: None,
            reject_reason: Some(reason.clone()),
        };

        self.orders.write().insert(order_id.clone(), order);

        warn!("订单拒绝: {} 原因: {}", order_id, reason);

        OrderResult {
            order_id,
            status: OrderStatus::Rejected,
            filled_qty: Decimal::ZERO,
            filled_price: Decimal::ZERO,
            commission: Decimal::ZERO,
            reject_reason: Some(reason),
            message,
        }
    }

    /// 检查持仓价值
    fn get_position_value(&self, symbol: &str, current_price: Decimal) -> Decimal {
        let positions = self.positions.read();
        if let Some(position) = positions.get(symbol) {
            (position.long_qty + position.short_qty) * current_price
        } else {
            Decimal::ZERO
        }
    }

    /// 检查订单频率
    fn check_order_frequency(&self, current_ts: u64) -> bool {
        let mut counters = self.order_count_last_second.write();

        // 清理超过1秒的记录
        counters.retain(|(ts, _)| *ts > current_ts.saturating_sub(1));

        // 计算当前1秒内的订单数
        let current_count: u32 = counters.iter().map(|(_, c)| c).sum();

        if current_count >= self.risk_config.order_frequency_limit as u32 {
            return false;
        }

        // 增加当前计数
        counters.push((current_ts, 1));
        true
    }

    /// 记录风控日志
    fn log_risk_event(
        &self,
        order_id: &str,
        symbol: &str,
        action: &str,
        reason: &str,
        available: Decimal,
        margin_ratio: Decimal,
    ) {
        // 这里可以扩展为写入 CSV 文件
        info!(
            "风控日志: {} {} {} 原因:{} 可用:{} 保证金率:{}",
            order_id, symbol, action, reason, available, margin_ratio
        );
    }

    /// 生成订单ID
    fn generate_order_id(&self) -> String {
        let mut counter = self.next_order_id.write();
        let id = format!("O{:06}", *counter);
        *counter += 1;
        id
    }

    /// 生成成交ID
    fn generate_trade_id(&self) -> String {
        let mut counter = self.next_trade_id.write();
        let id = format!("T{:06}", *counter);
        *counter += 1;
        id
    }

    /// 获取账户信息
    pub fn get_account(&self) -> MockAccount {
        self.account.read().clone()
    }

    /// 获取持仓
    pub fn get_position(&self, symbol: &str) -> Option<MockPosition> {
        self.positions.read().get(symbol).cloned()
    }

    /// 获取所有持仓
    pub fn get_all_positions(&self) -> FnvHashMap<String, MockPosition> {
        self.positions.read().clone()
    }

    /// 获取订单
    pub fn get_order(&self, order_id: &str) -> Option<MockOrder> {
        self.orders.read().get(order_id).cloned()
    }

    /// 获取所有成交
    pub fn get_trades(&self) -> Vec<MockTrade> {
        self.trades.read().clone()
    }

    /// 更新持仓的未实现盈亏
    pub fn update_position_pnl(&self, symbol: &str, current_price: Decimal) {
        let mut positions = self.positions.write();
        if let Some(position) = positions.get_mut(symbol) {
            position.calc_unrealized_pnl(current_price);

            // 更新账户的未实现盈亏
            let mut account = self.account.write();
            let total_pnl: Decimal = positions.values().map(|p| p.unrealized_pnl).sum();
            account.unrealized_pnl = total_pnl;
            account.total_equity = account.total_equity; // 基础余额 + 未实现盈亏
        }
    }

    /// 强制平仓检查
    pub fn check_liquidation(&self, symbol: &str, current_price: Decimal) -> Option<OrderResult> {
        let positions = self.positions.read();
        let account = self.account.read();

        if let Some(position) = positions.get(symbol) {
            // 计算保证金率
            // 保证金率 = 维持保证金 / 持仓价值
            let position_value = (position.long_qty + position.short_qty) * current_price;

            if !position_value.is_zero() {
                let maintenance_margin = position_value * self.risk_config.maintenance_margin_rate;

                // 如果未实现亏损使得账户权益 < 维持保证金，触发强平
                let equity = account.total_equity + position.unrealized_pnl;

                if equity < maintenance_margin {
                    warn!(
                        "触发强平: 品种 {} 权益 {} < 维持保证金 {}",
                        symbol, equity, maintenance_margin
                    );
                    drop(positions);
                    drop(account);

                    // 执行强平
                    return Some(self.force_liquidation(symbol, current_price));
                }
            }
        }

        None
    }

    /// 执行强制平仓
    fn force_liquidation(&self, symbol: &str, current_price: Decimal) -> OrderResult {
        let mut positions = self.positions.write();
        let mut account = self.account.write();

        if let Some(position) = positions.get_mut(symbol) {
            // 平掉所有持仓
            let total_qty = position.long_qty + position.short_qty;

            if total_qty > Decimal::ZERO {
                // 计算平仓盈亏
                let long_pnl = if position.long_qty > Decimal::ZERO {
                    (current_price - position.long_avg_price) * position.long_qty
                } else {
                    Decimal::ZERO
                };

                let short_pnl = if position.short_qty > Decimal::ZERO {
                    (position.short_avg_price - current_price) * position.short_qty
                } else {
                    Decimal::ZERO
                };

                let realized_pnl = long_pnl + short_pnl;

                // 更新账户
                account.available += position.margin_used; // 释放保证金
                account.frozen_margin -= position.margin_used;
                account.unrealized_pnl += realized_pnl;

                // 记录成交
                let trade_id = self.generate_trade_id();
                let order_id = self.generate_order_id();
                let commission = total_qty * current_price * dec!(0.0004);

                let trade = MockTrade {
                    trade_id,
                    order_id: order_id.clone(),
                    symbol: symbol.to_string(),
                    side: if position.long_qty > Decimal::ZERO { Side::Short } else { Side::Long },
                    qty: total_qty,
                    price: current_price,
                    commission,
                    realized_pnl,
                    ts: current_timestamp(),
                };

                self.trades.write().push(trade);

                // 清空持仓
                position.long_qty = Decimal::ZERO;
                position.short_qty = Decimal::ZERO;
                position.long_avg_price = Decimal::ZERO;
                position.short_avg_price = Decimal::ZERO;
                position.unrealized_pnl = Decimal::ZERO;
                position.margin_used = Decimal::ZERO;

                info!(
                    "强制平仓完成: {} 盈亏:{} 手续费:{}",
                    symbol, realized_pnl, commission
                );

                // 记录强制平仓事件到 SQLite
                self.record_risk_event_internal(
                    "LIQUIDATION",
                    symbol,
                    &order_id,
                    "MARGIN_BELOW_MAINTENANCE",
                    "FORCE_LIQUIDATION",
                    &format!("强制平仓完成 盈亏:{}", realized_pnl),
                );

                return OrderResult {
                    order_id,
                    status: OrderStatus::Filled,
                    filled_qty: total_qty,
                    filled_price: current_price,
                    commission,
                    reject_reason: None,
                    message: format!("强制平仓完成 盈亏:{}", realized_pnl),
                };
            }
        }

        OrderResult {
            order_id: self.generate_order_id(),
            status: OrderStatus::Cancelled,
            filled_qty: Decimal::ZERO,
            filled_price: Decimal::ZERO,
            commission: Decimal::ZERO,
            reject_reason: None,
            message: "无持仓需要平".to_string(),
        }
    }

    /// 获取账户快照
    pub fn get_account_snapshot(&self) -> AccountSnapshot {
        let account = self.account.read();
        AccountSnapshot {
            ts: account.update_ts,
            total_equity: account.total_equity,
            available: account.available,
            frozen_margin: account.frozen_margin,
            unrealized_pnl: account.unrealized_pnl,
            margin_ratio: account.margin_ratio(),
        }
    }

    /// 记录仓位变动事件
    fn record_position_change(
        &self,
        account: &MockAccount,
        positions: &FnvHashMap<String, MockPosition>,
        symbol: &str,
        ts: i64,
    ) {
        if let Some(ref recorder) = self.event_recorder {
            // 记录账户快照
            recorder.record_account_snapshot(AccountSnapshotRecord {
                id: None,
                ts,
                account_id: account.account_id.clone(),
                total_equity: format_decimal(&account.total_equity),
                available: format_decimal(&account.available),
                frozen_margin: format_decimal(&account.frozen_margin),
                unrealized_pnl: format_decimal(&account.unrealized_pnl),
                margin_ratio: format_decimal(&account.margin_ratio()),
            });

            // 记录该品种的交易所持仓
            if let Some(pos) = positions.get(symbol) {
                if pos.long_qty > Decimal::ZERO {
                    recorder.record_exchange_position(ExchangePositionRecord {
                        id: None,
                        ts,
                        symbol: symbol.to_string(),
                        side: "long".to_string(),
                        qty: format_decimal(&pos.long_qty),
                        avg_price: format_decimal(&pos.long_avg_price),
                        unrealized_pnl: format_decimal(&pos.unrealized_pnl),
                        margin_used: format_decimal(&pos.margin_used),
                    });
                }
                if pos.short_qty > Decimal::ZERO {
                    recorder.record_exchange_position(ExchangePositionRecord {
                        id: None,
                        ts,
                        symbol: symbol.to_string(),
                        side: "short".to_string(),
                        qty: format_decimal(&pos.short_qty),
                        avg_price: format_decimal(&pos.short_avg_price),
                        unrealized_pnl: format_decimal(&pos.unrealized_pnl),
                        margin_used: format_decimal(&pos.margin_used),
                    });
                }
            }
        }
    }

    /// 记录风控事件
    pub fn record_risk_event_internal(
        &self,
        event_type: &str,
        symbol: &str,
        order_id: &str,
        reason: &str,
        action_taken: &str,
        details: &str,
    ) {
        if let Some(ref recorder) = self.event_recorder {
            let account = self.account.read();
            recorder.record_risk_event(RiskEventRecord {
                id: None,
                ts: current_timestamp(),
                event_type: event_type.to_string(),
                symbol: symbol.to_string(),
                order_id: order_id.to_string(),
                reason: reason.to_string(),
                available_before: format_decimal(&account.available),
                margin_ratio_before: format_decimal(&account.margin_ratio()),
                action_taken: action_taken.to_string(),
                details: details.to_string(),
            });
        }
    }
}

/// 账户快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountSnapshot {
    pub ts: i64,
    pub total_equity: Decimal,
    pub available: Decimal,
    pub frozen_margin: Decimal,
    pub unrealized_pnl: Decimal,
    pub margin_ratio: Decimal,
}

/// 获取当前时间戳
fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

// ============================================================================
// Signal Synthesis Layer - 通道退出逻辑
// ============================================================================

/// 通道类型 (用于 MockBinanceGateway)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GatewayChannelType {
    /// 慢速通道 (日线级别)
    Slow,
    /// 快速通道 (分钟级/高频)
    Fast,
}

/// 通道状态
#[derive(Debug, Clone)]
pub struct ChannelState {
    /// 当前通道类型
    pub channel_type: GatewayChannelType,
    /// 进入高速的时间戳
    pub enter_fast_ts: Option<i64>,
    /// tr_ratio 值
    pub tr_ratio: Decimal,
    /// ma5 在 20 日均线的位置
    pub ma5_in_20d_pos: Decimal,
    /// Pine 颜色
    pub pine_color: PineColorState,
}

/// Pine 颜色状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PineColorState {
    Red,    // 绿色 (下跌)
    Green,  // 红色 (上涨)
    Yellow, // 黄色 (中性/震荡)
}

/// 信号综合层
pub struct SignalSynthesisLayer {
    /// 通道状态
    channel_state: RwLock<ChannelState>,
    /// 高速进入阈值 (15min >= 13% 或 1min >= 3%)
    pub high_vol_threshold_15m: Decimal,
    pub high_vol_threshold_1m: Decimal,
    /// 低速退出阈值 (tr_ratio < 1)
    pub low_vol_exit_threshold: Decimal,
}

impl SignalSynthesisLayer {
    /// 创建新的 SignalSynthesisLayer
    pub fn new() -> Self {
        Self {
            channel_state: RwLock::new(ChannelState {
                channel_type: GatewayChannelType::Slow,
                enter_fast_ts: None,
                tr_ratio: Decimal::ZERO,
                ma5_in_20d_pos: Decimal::ZERO,
                pine_color: PineColorState::Yellow,
            }),
            high_vol_threshold_15m: dec!(0.13),  // 13%
            high_vol_threshold_1m: dec!(0.03),   // 3%
            low_vol_exit_threshold: dec!(1.0),    // tr_ratio < 1
        }
    }

    /// 检查是否应该进入高速通道
    pub fn check_enter_high_volatility(
        &self,
        volatility_15m: Decimal,
        volatility_1m: Decimal,
    ) -> bool {
        volatility_15m >= self.high_vol_threshold_15m || volatility_1m >= self.high_vol_threshold_1m
    }

    /// 检查是否应该退出高速通道 (tr_ratio < 1)
    pub fn check_exit_high_volatility(&self, tr_ratio: Decimal) -> bool {
        tr_ratio < self.low_vol_exit_threshold && self.channel_state.read().channel_type == GatewayChannelType::Fast
    }

    /// 更新通道状态
    pub fn update_channel(
        &self,
        channel_type: GatewayChannelType,
        tr_ratio: Decimal,
        ma5_in_20d_pos: Decimal,
        pine_color: PineColorState,
        current_ts: i64,
    ) {
        let mut state = self.channel_state.write();

        let old_type = state.channel_type;
        state.channel_type = channel_type;
        state.tr_ratio = tr_ratio;
        state.ma5_in_20d_pos = ma5_in_20d_pos;
        state.pine_color = pine_color;

        if channel_type == GatewayChannelType::Fast && old_type == GatewayChannelType::Slow {
            state.enter_fast_ts = Some(current_ts);
            info!("通道切换: Slow -> Fast at {}", current_ts);
        } else if channel_type == GatewayChannelType::Slow && old_type == GatewayChannelType::Fast {
            state.enter_fast_ts = None;
            info!("通道切换: Fast -> Slow at {} (tr_ratio={})", current_ts, tr_ratio);
        }
    }

    /// 获取当前通道类型
    pub fn get_channel_type(&self) -> GatewayChannelType {
        self.channel_state.read().channel_type
    }

    /// 获取当前通道状态
    pub fn get_channel_state(&self) -> ChannelState {
        self.channel_state.read().clone()
    }

    /// 检查日线趋势平仓条件
    /// 条件: ma5_close 位置 + Pine 颜色
    pub fn check_daily_trend_exit(&self) -> Option<ExitSignal> {
        let state = self.channel_state.read();

        // 日线趋势平仓条件:
        // 1. ma5_close 在 20 日均线下方 (ma5_in_20d_pos < 0.5)
        // 2. Pine 颜色为红色 (下跌趋势)
        let ma5_below_ma20 = state.ma5_in_20d_pos < dec!(0.5);
        let pine_is_red = state.pine_color == PineColorState::Red;

        if ma5_below_ma20 && pine_is_red {
            Some(ExitSignal {
                reason: "日线趋势反转".to_string(),
                ts: current_timestamp(),
                details: format!(
                    "ma5_in_20d_pos={:.2} PineColor={:?}",
                    state.ma5_in_20d_pos, state.pine_color
                ),
            })
        } else {
            None
        }
    }

    /// 检查通道退出条件
    pub fn check_channel_exit(&self, tr_ratio: Decimal) -> Option<ExitSignal> {
        if self.check_exit_high_volatility(tr_ratio) {
            Some(ExitSignal {
                reason: "tr_ratio < 1 退出高速通道".to_string(),
                ts: current_timestamp(),
                details: format!("tr_ratio={:.4}", tr_ratio),
            })
        } else {
            None
        }
    }

    /// 合成最终交易决策
    ///
    /// 根据信号和持仓状态，合成最终的交易决策
    ///
    /// # 参数
    /// - `signal`: 交易信号
    /// - `position_side`: 当前持仓方向
    /// - `current_price`: 当前价格
    /// - `symbol`: 交易品种
    /// - `account_pool`: 账户保证金池 (用于计算风控数量)
    /// - `current_symbol_count`: 当前交易品种数量
    /// - `leverage`: 杠杆倍数 (默认 10)
    pub fn synthesize(
        &self,
        signal: strategy::types::Signal,
        position_side: Option<strategy::types::Side>,
        current_price: Decimal,
        symbol: &str,
        account_pool: &AccountPool,
        current_symbol_count: Decimal,
        leverage: Decimal,
    ) -> strategy::types::TradingDecision {
        use strategy::types::{Side, TradingDecision, TradingAction};

        let state = self.channel_state.read();
        let channel_type = state.channel_type;

        // 计算风控开仓数量
        let open_qty = self.calculate_open_qty_for_channel(
            account_pool,
            current_symbol_count,
            current_price,
            leverage,
        );

        match signal {
            strategy::types::Signal::LongEntry => {
                // 做多入场信号
                if position_side == Some(Side::Short) {
                    // 当前持有空头，先平空再开多
                    TradingDecision::close_short(
                        symbol.to_string(),
                        current_price,
                        format!("通道 {:?} 平空入场做多", channel_type),
                    )
                } else {
                    TradingDecision::open_long(
                        symbol.to_string(),
                        current_price,
                        open_qty,
                        format!("通道 {:?} 做多入场", channel_type),
                    )
                }
            }
            strategy::types::Signal::ShortEntry => {
                // 做空入场信号
                if position_side == Some(Side::Long) {
                    // 当前持有多头，先平多再开空
                    TradingDecision::close_long(
                        symbol.to_string(),
                        current_price,
                        format!("通道 {:?} 平多入场做空", channel_type),
                    )
                } else {
                    TradingDecision::open_short(
                        symbol.to_string(),
                        current_price,
                        open_qty,
                        format!("通道 {:?} 做空入场", channel_type),
                    )
                }
            }
            strategy::types::Signal::LongExit => {
                // 平多信号
                TradingDecision::close_long(
                    symbol.to_string(),
                    current_price,
                    format!("通道 {:?} 平多", channel_type),
                )
            }
            strategy::types::Signal::ShortExit => {
                // 平空信号
                TradingDecision::close_short(
                    symbol.to_string(),
                    current_price,
                    format!("通道 {:?} 平空", channel_type),
                )
            }
            strategy::types::Signal::ExitHighVol => {
                // 高波动退出信号 - 全部平仓
                match position_side {
                    Some(Side::Long) => {
                        TradingDecision::close_long(
                            symbol.to_string(),
                            current_price,
                            format!("高波动退出 通道 {:?} 平多", channel_type),
                        )
                    }
                    Some(Side::Short) => {
                        TradingDecision::close_short(
                            symbol.to_string(),
                            current_price,
                            format!("高波动退出 通道 {:?} 平空", channel_type),
                        )
                    }
                    None => TradingDecision::no_action(
                        symbol.to_string(),
                        "高波动退出 无持仓".to_string(),
                    ),
                }
            }
            strategy::types::Signal::LongHedge => {
                // 多头对冲信号 (保持多头，但可能需要减仓)
                TradingDecision::no_action(
                    symbol.to_string(),
                    format!("通道 {:?} 多头对冲观望", channel_type),
                )
            }
            strategy::types::Signal::ShortHedge => {
                // 空头对冲信号
                TradingDecision::no_action(
                    symbol.to_string(),
                    format!("通道 {:?} 空头对冲观望", channel_type),
                )
            }
        }
    }

    /// 根据账户保证金和通道类型计算开仓数量
    ///
    /// 使用分钟级风控计算实际可开仓名义价值，然后转换为数量
    ///
    /// # 参数
    /// - `account_pool`: 账户保证金池
    /// - `current_symbol_count`: 当前交易品种数量
    /// - `current_price`: 当前价格
    /// - `leverage`: 杠杆倍数 (默认 10)
    ///
    /// # 返回
    /// - 计算出的开仓数量
    pub fn calculate_open_qty_for_channel(
        &self,
        account_pool: &AccountPool,
        current_symbol_count: Decimal,
        current_price: Decimal,
        leverage: Decimal,
    ) -> Decimal {
        use rust_decimal::Decimal;

        // 使用分钟级风控计算
        let result = calculate_minute_open_notional(
            account_pool,
            current_symbol_count,
            leverage,
        );

        // 如果满足最小阈值，使用计算出的名义价值
        let notional = if result.meets_min_threshold {
            result.actual_notional_per_symbol
        } else {
            // 不满足阈值时返回0，让调用方处理
            return Decimal::ZERO;
        };

        // 将名义价值转换为数量
        if current_price <= Decimal::ZERO {
            return dec!(0.001); // 最小默认值
        }

        let qty = notional / current_price;

        // 确保不低于最小数量
        qty.max(dec!(0.001))
    }
}

impl Default for SignalSynthesisLayer {
    fn default() -> Self {
        Self::new()
    }
}

/// 退出信号
#[derive(Debug, Clone)]
pub struct ExitSignal {
    pub reason: String,
    pub ts: i64,
    pub details: String,
}

/// Trigger 日志条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerLogEntry {
    pub ts: i64,
    pub event: String,
    pub channel_type: String,
    pub tr_ratio: Decimal,
    pub ma5_in_20d_pos: Decimal,
    pub pine_color: String,
    pub details: String,
}

impl SignalSynthesisLayer {
    /// 创建触发日志条目
    pub fn create_trigger_log(&self, event: &str, details: &str) -> TriggerLogEntry {
        let state = self.channel_state.read();
        TriggerLogEntry {
            ts: current_timestamp(),
            event: event.to_string(),
            channel_type: format!("{:?}", state.channel_type),
            tr_ratio: state.tr_ratio,
            ma5_in_20d_pos: state.ma5_in_20d_pos,
            pine_color: format!("{:?}", state.pine_color),
            details: details.to_string(),
        }
    }
}

// ============================================================================
// ExchangeGateway trait 实现
// ============================================================================

impl crate::gateway::ExchangeGateway for MockBinanceGateway {
    /// 下单
    fn place_order(&self, req: OrderRequest) -> Result<OrderResult, EngineError> {
        MockBinanceGateway::place_order(self, req)
    }

    /// 获取账户信息
    fn get_account(&self) -> Result<MockAccount, EngineError> {
        Ok(MockBinanceGateway::get_account(self))
    }

    /// 获取持仓
    fn get_position(&self, symbol: &str) -> Result<Option<MockPosition>, EngineError> {
        Ok(MockBinanceGateway::get_position(self, symbol))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_account_creation() {
        let account = MockAccount::new("test".to_string(), dec!(100000.0));
        assert_eq!(account.total_equity, dec!(100000.0));
        assert_eq!(account.available, dec!(100000.0));
        assert_eq!(account.frozen_margin, Decimal::ZERO);
    }

    #[test]
    fn test_mock_position_pnl() {
        let mut position = MockPosition::new("BTCUSDT".to_string());
        position.long_qty = dec!(0.1);
        position.long_avg_price = dec!(70000);

        // 价格下跌到 69000
        position.calc_unrealized_pnl(dec!(69000));

        // 盈亏 = (69000 - 70000) * 0.1 = -100
        assert!(position.unrealized_pnl < Decimal::ZERO);
    }

    #[test]
    fn test_channel_switch() {
        let synthesis = SignalSynthesisLayer::new();

        // 初始状态是慢速通道
        assert_eq!(synthesis.get_channel_type(), GatewayChannelType::Slow);

        // 检查进入高速条件
        assert!(synthesis.check_enter_high_volatility(dec!(0.15), dec!(0.01)));
        assert!(synthesis.check_enter_high_volatility(dec!(0.01), dec!(0.05)));

        // 先进入高速通道
        synthesis.update_channel(
            GatewayChannelType::Fast,
            dec!(1.5),
            dec!(0.6),
            PineColorState::Green,
            1000,
        );

        // 检查退出高速条件
        assert!(!synthesis.check_exit_high_volatility(dec!(1.5))); // tr_ratio > 1, 不退出
        assert!(synthesis.check_exit_high_volatility(dec!(0.8)));  // tr_ratio < 1, 退出
    }

    #[test]
    fn test_order_frequency_limit() {
        let gateway = MockBinanceGateway::new(
            dec!(100000.0),
            RiskConfig::default(),
            CsvWriter::default(),
        );

        // 模拟多次下单 (默认频率限制是 10次/秒)
        for _ in 0..10 {
            let req = OrderRequest {
                symbol: "BTCUSDT".to_string(),
                side: Side::Long,
                order_type: OrderType::Market,
                qty: dec!(0.001),
                price: Some(dec!(70000)),
            };
            let _ = gateway.place_order(req);
        }

        // 继续下单应该被频率限制拒绝
        let req = OrderRequest {
            symbol: "BTCUSDT".to_string(),
            side: Side::Long,
            order_type: OrderType::Market,
            qty: dec!(0.001),
            price: Some(dec!(70000)),
        };
        let result = gateway.place_order(req).unwrap();
        assert_eq!(result.status, OrderStatus::Rejected);
        assert_eq!(result.reject_reason, Some(RejectReason::OrderFrequencyExceeded));
    }
}
