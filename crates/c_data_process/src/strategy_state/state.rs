//! 策略状态核心数据结构

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};


/// 持仓方向
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PositionSide {
    #[default]
    None,
    Long,
    Short,
}

/// 持仓状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionState {
    pub current: Decimal,
    pub side: PositionSide,
    pub avg_entry_price: Decimal,
    pub unrealized_pnl: Decimal,
    pub cumulative_closed_pnl: Decimal,
}

impl Default for PositionState {
    fn default() -> Self {
        Self {
            current: Decimal::ZERO,
            side: PositionSide::None,
            avg_entry_price: Decimal::ZERO,
            unrealized_pnl: Decimal::ZERO,
            cumulative_closed_pnl: Decimal::ZERO,
        }
    }
}

/// 每日盈亏快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyPnl {
    pub date: String,
    pub pnl: Decimal,
}

/// 平仓记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClosedTrade {
    pub trade_id: String,
    pub timestamp: DateTime<Utc>,
    pub symbol: String,
    pub side: PositionSide,
    pub entry_price: Decimal,
    pub exit_price: Decimal,
    pub qty: Decimal,
    pub fee: Decimal,
    pub pnl: Decimal,
    pub pnl_pct: Decimal,
    pub hold_duration_secs: i64,
    pub signal_type: String,
}

/// 盈亏统计状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PnlState {
    pub cumulative_closed: Decimal,
    pub daily: Vec<DailyPnl>,
    pub closed_trades: Vec<ClosedTrade>,
    pub max_drawdown: Decimal,
    pub max_drawdown_pct: Decimal,
    pub max_single_trade_profit: Decimal,
    pub max_single_trade_loss: Decimal,
}

impl Default for PnlState {
    fn default() -> Self {
        Self {
            cumulative_closed: Decimal::ZERO,
            daily: Vec::new(),
            closed_trades: Vec::new(),
            max_drawdown: Decimal::ZERO,
            max_drawdown_pct: Decimal::ZERO,
            max_single_trade_profit: Decimal::ZERO,
            max_single_trade_loss: Decimal::ZERO,
        }
    }
}

/// 交易统计
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TradingStats {
    pub total_trades: u64,
    pub winning_trades: u64,
    pub losing_trades: u64,
    pub win_rate: Decimal,
    pub profit_factor: Decimal,
    pub total_profit: Decimal,
    pub total_loss: Decimal,
}

/// 风控状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskState {
    pub stop_loss_price: Decimal,
    pub take_profit_price: Decimal,
    pub trailing_stop: Option<Decimal>,
    pub is_trading: bool,
    pub error_count: u32,
    pub circuit_breaker_triggered: bool,
}

impl Default for RiskState {
    fn default() -> Self {
        Self {
            stop_loss_price: Decimal::ZERO,
            take_profit_price: Decimal::ZERO,
            trailing_stop: None,
            is_trading: true,
            error_count: 0,
            circuit_breaker_triggered: false,
        }
    }
}

/// 策略参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyParams {
    pub leverage: u32,
    pub position_size_pct: Decimal,
    pub channel_type: String,
    pub channel_params: serde_json::Value,
}

impl Default for StrategyParams {
    fn default() -> Self {
        Self {
            leverage: 1,
            position_size_pct: Decimal::new(10, 2), // 10%
            channel_type: "trend".to_string(),
            channel_params: serde_json::json!({}),
        }
    }
}

/// 交易记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeRecord {
    pub trade_id: String,
    pub timestamp: DateTime<Utc>,
    pub symbol: String,
    pub side: PositionSide,
    pub price: Decimal,
    pub qty: Decimal,
    pub fee: Decimal,
    pub realized_pnl: Decimal,
    pub signal_type: String,
}

/// 核心策略状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyState {
    pub instrument_id: String,
    pub strategy_id: String,
    pub exchange: String,
    pub channel: String,
    pub init_time: DateTime<Utc>,
    pub last_update_time: DateTime<Utc>,
    pub position: PositionState,
    pub pnl: PnlState,
    pub trading_stats: TradingStats,
    pub risk: RiskState,
    pub params: StrategyParams,
}

impl StrategyState {
    pub fn new(instrument_id: String, strategy_id: String, exchange: String, channel: String) -> Self {
        let now = Utc::now();
        Self {
            instrument_id,
            strategy_id,
            exchange,
            channel,
            init_time: now,
            last_update_time: now,
            position: PositionState::default(),
            pnl: PnlState::default(),
            trading_stats: TradingStats::default(),
            risk: RiskState::default(),
            params: StrategyParams::default(),
        }
    }

    /// 更新持仓
    pub fn update_position(&mut self, side: PositionSide, qty: Decimal, price: Decimal) {
        self.last_update_time = Utc::now();
        
        match side {
            PositionSide::Long => {
                if self.position.side == PositionSide::Short && self.position.current > Decimal::ZERO {
                    // 平空开多
                    let close_qty = self.position.current.min(qty);
                    self.position.current = qty - close_qty;
                    if self.position.current > Decimal::ZERO {
                        self.position.side = PositionSide::Long;
                        self.position.avg_entry_price = price;
                    } else {
                        self.position.side = PositionSide::None;
                    }
                } else {
                    // 纯开多
                    let total_cost = self.position.avg_entry_price * self.position.current + price * qty;
                    self.position.current += qty;
                    if self.position.current > Decimal::ZERO {
                        self.position.avg_entry_price = total_cost / self.position.current;
                    }
                    self.position.side = PositionSide::Long;
                }
            }
            PositionSide::Short => {
                if self.position.side == PositionSide::Long && self.position.current > Decimal::ZERO {
                    // 平多开空
                    self.position.current = qty;
                    self.position.side = PositionSide::Short;
                    self.position.avg_entry_price = price;
                } else {
                    // 纯开空
                    let total_cost = self.position.avg_entry_price * self.position.current + price * qty;
                    self.position.current += qty;
                    if self.position.current > Decimal::ZERO {
                        self.position.avg_entry_price = total_cost / self.position.current;
                    }
                    self.position.side = PositionSide::Short;
                }
            }
            PositionSide::None => {
                // 清仓
                self.position.current = Decimal::ZERO;
                self.position.side = PositionSide::None;
                self.position.avg_entry_price = Decimal::ZERO;
                self.position.unrealized_pnl = Decimal::ZERO;
            }
        }
    }

    /// 更新浮动盈亏
    pub fn update_unrealized_pnl(&mut self, current_price: Decimal) {
        self.last_update_time = Utc::now();
        
        if self.position.current <= Decimal::ZERO {
            self.position.unrealized_pnl = Decimal::ZERO;
            return;
        }

        let price_diff = match self.position.side {
            PositionSide::Long => current_price - self.position.avg_entry_price,
            PositionSide::Short => self.position.avg_entry_price - current_price,
            PositionSide::None => Decimal::ZERO,
        };
        
        self.position.unrealized_pnl = price_diff * self.position.current;
    }

    /// 记录已平仓盈亏
    pub fn record_realized_pnl(&mut self, pnl: Decimal) {
        self.last_update_time = Utc::now();
        
        // 更新持仓累计平仓盈亏
        self.position.cumulative_closed_pnl += pnl;
        
        // 更新全局累计盈亏
        self.pnl.cumulative_closed += pnl;
        
        // 更新最大回撤
        if self.pnl.cumulative_closed < self.pnl.max_drawdown {
            self.pnl.max_drawdown = self.pnl.cumulative_closed;
        }
        
        // 更新交易统计
        self.trading_stats.total_trades += 1;
        if pnl > Decimal::ZERO {
            self.trading_stats.winning_trades += 1;
            self.trading_stats.total_profit += pnl;
        } else {
            self.trading_stats.losing_trades += 1;
            self.trading_stats.total_loss += pnl.abs();
        }
        
        // 计算胜率
        if self.trading_stats.total_trades > 0 {
            self.trading_stats.win_rate = Decimal::from(self.trading_stats.winning_trades)
                / Decimal::from(self.trading_stats.total_trades);
        }
        
        // 计算利润因子
        if self.trading_stats.total_loss > Decimal::ZERO {
            self.trading_stats.profit_factor = self.trading_stats.total_profit / self.trading_stats.total_loss;
        }
    }

    /// 记录完整平仓交易
    pub fn record_closed_trade(&mut self, trade: ClosedTrade) {
        self.last_update_time = Utc::now();
        
        let pnl = trade.pnl;
        
        // 更新持仓累计平仓盈亏
        self.position.cumulative_closed_pnl += pnl;
        
        // 更新全局累计盈亏
        self.pnl.cumulative_closed += pnl;
        
        // 添加到平仓记录列表
        self.pnl.closed_trades.push(trade);
        
        // 限制记录数量（保留最近 1000 条）
        if self.pnl.closed_trades.len() > 1000 {
            self.pnl.closed_trades.remove(0);
        }
        
        // 更新最大回撤
        if self.pnl.cumulative_closed < self.pnl.max_drawdown {
            self.pnl.max_drawdown = self.pnl.cumulative_closed;
        }
        
        // 更新最大单笔盈亏
        if pnl > Decimal::ZERO && pnl > self.pnl.max_single_trade_profit {
            self.pnl.max_single_trade_profit = pnl;
        } else if pnl < Decimal::ZERO && pnl.abs() > self.pnl.max_single_trade_loss {
            self.pnl.max_single_trade_loss = pnl.abs();
        }
        
        // 更新交易统计
        self.trading_stats.total_trades += 1;
        if pnl > Decimal::ZERO {
            self.trading_stats.winning_trades += 1;
            self.trading_stats.total_profit += pnl;
        } else {
            self.trading_stats.losing_trades += 1;
            self.trading_stats.total_loss += pnl.abs();
        }
        
        // 计算胜率
        if self.trading_stats.total_trades > 0 {
            self.trading_stats.win_rate = Decimal::from(self.trading_stats.winning_trades)
                / Decimal::from(self.trading_stats.total_trades);
        }
        
        // 计算利润因子
        if self.trading_stats.total_loss > Decimal::ZERO {
            self.trading_stats.profit_factor = self.trading_stats.total_profit / self.trading_stats.total_loss;
        }
    }

    /// 更新止损止盈
    pub fn update_risk_levels(&mut self, stop_loss: Decimal, take_profit: Decimal) {
        self.last_update_time = Utc::now();
        self.risk.stop_loss_price = stop_loss;
        self.risk.take_profit_price = take_profit;
    }

    /// 设置交易开关
    pub fn set_trading(&mut self, enabled: bool) {
        self.last_update_time = Utc::now();
        self.risk.is_trading = enabled;
    }

    /// 增加错误计数
    pub fn increment_error(&mut self) {
        self.risk.error_count += 1;
        if self.risk.error_count >= 5 {
            self.risk.circuit_breaker_triggered = true;
            self.risk.is_trading = false;
        }
    }

    /// 重置错误计数
    pub fn reset_error(&mut self) {
        self.risk.error_count = 0;
        self.risk.circuit_breaker_triggered = false;
    }

    /// 唯一标识符
    pub fn id(&self) -> String {
        format!("{}:{}", self.instrument_id, self.strategy_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_new_state() {
        let state = StrategyState::new(
            "BTC-USDT".to_string(),
            "trend_v1".to_string(),
            "binance".to_string(),
            "1h".to_string(),
        );
        
        assert_eq!(state.instrument_id, "BTC-USDT");
        assert_eq!(state.strategy_id, "trend_v1");
        assert_eq!(state.position.current, dec!(0));
    }

    #[test]
    fn test_update_position_long() {
        let mut state = StrategyState::new(
            "BTC-USDT".to_string(),
            "trend_v1".to_string(),
            "binance".to_string(),
            "1h".to_string(),
        );
        
        state.update_position(PositionSide::Long, dec!(0.1), dec!(50000));
        
        assert_eq!(state.position.current, dec!(0.1));
        assert_eq!(state.position.side, PositionSide::Long);
        assert_eq!(state.position.avg_entry_price, dec!(50000));
    }

    #[test]
    fn test_record_realized_pnl() {
        let mut state = StrategyState::new(
            "BTC-USDT".to_string(),
            "trend_v1".to_string(),
            "binance".to_string(),
            "1h".to_string(),
        );
        
        state.record_realized_pnl(dec!(100));
        state.record_realized_pnl(dec!(-50));
        state.record_realized_pnl(dec!(200));
        
        assert_eq!(state.pnl.cumulative_closed, dec!(250));
        assert_eq!(state.trading_stats.total_trades, 3);
        assert_eq!(state.trading_stats.winning_trades, 2);
        assert_eq!(state.trading_stats.losing_trades, 1);
        assert_eq!(state.trading_stats.win_rate, dec!(2) / dec!(3));
    }
}
