//! SimpleExecutor - 简单止盈止损执行器
//!
//! 币安测试网简单交易策略：
//! - 入场后设置 0.4% 止盈
//! - 追踪止损：价格回撤 0.1% 则平仓
//! - 最大持仓时间：60分钟

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use chrono::{DateTime, Utc, Duration};
use thiserror::Error;

/// 交易方向
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TradeDirection {
    Long,
    Short,
}

/// 持仓状态
#[derive(Debug, Clone)]
pub struct Position {
    /// 品种
    pub symbol: String,
    /// 方向
    pub direction: TradeDirection,
    /// 入场价格
    pub entry_price: Decimal,
    /// 数量
    pub qty: Decimal,
    /// 入场时间
    pub entry_time: DateTime<Utc>,
    /// 追踪止损价格
    pub trailing_stop: Decimal,
    /// 止盈价格
    pub take_profit: Decimal,
    /// 初始止损价格
    pub initial_stop: Decimal,
}

/// 简单执行器配置
#[derive(Debug, Clone)]
pub struct SimpleExecutorConfig {
    /// 止盈百分比（默认 0.4%）
    pub take_profit_pct: Decimal,
    /// 追踪止损回撤百分比（默认 0.1%）
    pub trailing_stop_pct: Decimal,
    /// 最大持仓时间（分钟，默认 60）
    pub max_hold_minutes: i64,
    /// 是否启用追踪止损
    pub trailing_stop_enabled: bool,
}

impl Default for SimpleExecutorConfig {
    fn default() -> Self {
        Self {
            take_profit_pct: dec!(0.4),      // 0.4% 止盈
            trailing_stop_pct: dec!(0.1),    // 0.1% 回撤止损
            max_hold_minutes: 60,            // 60分钟最大持仓
            trailing_stop_enabled: true,     // 启用追踪止损
        }
    }
}

/// 交易结果
#[derive(Debug, Clone)]
pub enum TradeResult {
    /// 止盈平仓
    TakeProfit { symbol: String, pnl_pct: Decimal },
    /// 追踪止损平仓
    TrailingStop { symbol: String, pnl_pct: Decimal },
    /// 超时平仓
    Timeout { symbol: String, pnl_pct: Decimal },
    /// 手动平仓
    Manual { symbol: String, pnl_pct: Decimal },
}

/// 简单执行器错误
#[derive(Debug, Clone, Error)]
pub enum SimpleExecutorError {
    #[error("已有持仓：{0}")]
    AlreadyHasPosition(String),

    #[error("无持仓")]
    NoPosition,

    #[error("品种不匹配：预期 {expected}，实际 {actual}")]
    SymbolMismatch { expected: String, actual: String },
}

/// 简单止盈止损执行器
pub struct SimpleExecutor {
    /// 配置
    config: SimpleExecutorConfig,
    /// 当前持仓
    position: Option<Position>,
    /// 历史交易结果
    trade_history: Vec<TradeResult>,
}

impl SimpleExecutor {
    /// 创建新的执行器
    pub fn new(config: SimpleExecutorConfig) -> Self {
        Self {
            config,
            position: None,
            trade_history: Vec::new(),
        }
    }

    /// 创建默认配置的执行器
    pub fn with_default_config() -> Self {
        Self::new(SimpleExecutorConfig::default())
    }

    /// 开仓
    pub fn open_position(
        &mut self,
        symbol: &str,
        direction: TradeDirection,
        entry_price: Decimal,
        qty: Decimal,
        entry_time: DateTime<Utc>,
    ) -> Result<(), SimpleExecutorError> {
        if self.position.is_some() {
            return Err(SimpleExecutorError::AlreadyHasPosition(symbol.to_string()));
        }

        let (take_profit, initial_stop) = self.calculate_levels(direction, entry_price);

        let position = Position {
            symbol: symbol.to_string(),
            direction,
            entry_price,
            qty,
            entry_time,
            trailing_stop: initial_stop,
            take_profit,
            initial_stop,
        };

        self.position = Some(position);
        Ok(())
    }

    /// 更新持仓状态（每个Tick调用）
    pub fn update(&mut self, current_price: Decimal, current_time: DateTime<Utc>) -> Option<TradeResult> {
        // 先检查有无持仓
        if self.position.is_none() {
            return None;
        }

        // 克隆需要的数据，避免borrow冲突
        let direction = self.position.as_ref().unwrap().direction;
        let entry_price = self.position.as_ref().unwrap().entry_price;
        let entry_time = self.position.as_ref().unwrap().entry_time;
        let trailing_stop_pct = self.config.trailing_stop_pct;
        let max_hold = self.config.max_hold_minutes;
        let symbol = self.position.as_ref().unwrap().symbol.clone();
        let take_profit = self.position.as_ref().unwrap().take_profit;

        // 计算盈亏百分比
        let pnl_pct = Self::calc_pnl_pct_static(direction, entry_price, current_price);

        // 1. 检查止盈
        let tp_hit = match direction {
            TradeDirection::Long => current_price >= take_profit,
            TradeDirection::Short => current_price <= take_profit,
        };

        if tp_hit {
            let result = TradeResult::TakeProfit {
                symbol: symbol.clone(),
                pnl_pct,
            };
            self.trade_history.push(result.clone());
            self.position = None;
            return Some(result);
        }

        // 2. 检查追踪止损
        if self.config.trailing_stop_enabled {
            let new_trailing_stop = Self::calc_trailing_stop_static(direction, entry_price, trailing_stop_pct);

            // 更新追踪止损（只向有利方向移动）
            let current_trailing_stop = self.position.as_ref().unwrap().trailing_stop;
            let should_update = match direction {
                TradeDirection::Long => new_trailing_stop > current_trailing_stop,
                TradeDirection::Short => new_trailing_stop < current_trailing_stop,
            };

            if should_update {
                self.position.as_mut().unwrap().trailing_stop = new_trailing_stop;
            }

            // 检查追踪止损
            let stop_hit = match direction {
                TradeDirection::Long => current_price <= new_trailing_stop,
                TradeDirection::Short => current_price >= new_trailing_stop,
            };

            if stop_hit {
                let result = TradeResult::TrailingStop {
                    symbol: symbol.clone(),
                    pnl_pct,
                };
                self.trade_history.push(result.clone());
                self.position = None;
                return Some(result);
            }
        }

        // 3. 检查超时
        let hold_duration = current_time - entry_time;
        if hold_duration > Duration::minutes(max_hold) {
            let result = TradeResult::Timeout {
                symbol: symbol.clone(),
                pnl_pct,
            };
            self.trade_history.push(result.clone());
            self.position = None;
            return Some(result);
        }

        None
    }

    /// 平仓（手动）
    pub fn close_position(&mut self, current_price: Decimal) -> Result<TradeResult, SimpleExecutorError> {
        let position = self.position.take().ok_or(SimpleExecutorError::NoPosition)?;

        let pnl_pct = Self::calc_pnl_pct_static(position.direction, position.entry_price, current_price);

        let result = TradeResult::Manual {
            symbol: position.symbol,
            pnl_pct,
        };

        self.trade_history.push(result.clone());
        Ok(result)
    }

    /// 切换品种（强制平仓当前持仓）
    pub fn switch_symbol(&mut self, current_price: Decimal) -> Option<TradeResult> {
        if self.position.is_some() {
            self.close_position(current_price).ok()
        } else {
            None
        }
    }

    /// 获取当前持仓
    pub fn get_position(&self) -> Option<&Position> {
        self.position.as_ref()
    }

    /// 获取交易历史
    pub fn get_history(&self) -> &[TradeResult] {
        &self.trade_history
    }

    /// 清空历史
    pub fn clear_history(&mut self) {
        self.trade_history.clear();
    }

    /// 计算止盈止损价格
    fn calculate_levels(&self, direction: TradeDirection, entry_price: Decimal) -> (Decimal, Decimal) {
        let tp_pct = self.config.take_profit_pct / dec!(100);
        let tp_distance = entry_price * tp_pct;

        // 追踪止损初始值：止盈的一半
        let initial_stop_pct = tp_pct / dec!(2);
        let initial_stop_distance = entry_price * initial_stop_pct;

        let (take_profit, initial_stop) = match direction {
            TradeDirection::Long => {
                (entry_price + tp_distance, entry_price - initial_stop_distance)
            }
            TradeDirection::Short => {
                (entry_price - tp_distance, entry_price + initial_stop_distance)
            }
        };

        (take_profit, initial_stop)
    }

    /// 计算追踪止损价格（静态版本）
    fn calc_trailing_stop_static(direction: TradeDirection, entry_price: Decimal, trailing_pct: Decimal) -> Decimal {
        let ts_pct = trailing_pct / dec!(100);
        let ts_distance = entry_price * ts_pct;

        match direction {
            TradeDirection::Long => entry_price + ts_distance,
            TradeDirection::Short => entry_price - ts_distance,
        }
    }

    /// 计算盈亏百分比（静态版本）
    fn calc_pnl_pct_static(direction: TradeDirection, entry_price: Decimal, current_price: Decimal) -> Decimal {
        let price_diff = match direction {
            TradeDirection::Long => current_price - entry_price,
            TradeDirection::Short => entry_price - current_price,
        };

        if entry_price.is_zero() {
            return Decimal::ZERO;
        }
        (price_diff / entry_price * dec!(100)).round_dp(4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_and_take_profit() {
        let mut executor = SimpleExecutor::with_default_config();

        // 开多仓
        executor.open_position(
            "BTCUSDT",
            TradeDirection::Long,
            dec!(100),
            dec!(0.01),
            Utc::now(),
        ).unwrap();

        assert!(executor.get_position().is_some());

        // 价格上涨到止盈（0.4%）
        let result = executor.update(dec!(100.4), Utc::now());
        assert!(result.is_some());

        match result.unwrap() {
            TradeResult::TakeProfit { pnl_pct, .. } => {
                assert!(pnl_pct >= dec!(0.39));
            }
            _ => panic!("Expected TakeProfit"),
        }
    }

    #[test]
    fn test_trailing_stop() {
        let mut executor = SimpleExecutor::with_default_config();

        executor.open_position(
            "BTCUSDT",
            TradeDirection::Long,
            dec!(100),
            dec!(0.01),
            Utc::now(),
        ).unwrap();

        // 价格先上涨到 100.3，追踪止损变为 100.1
        executor.update(dec!(100.3), Utc::now());

        // 再回落到 100.09（跌破追踪止损 100.1）
        let result = executor.update(dec!(100.09), Utc::now());

        assert!(result.is_some());
        match result.unwrap() {
            TradeResult::TrailingStop { pnl_pct, .. } => {
                // 从 100 到 100.09，盈利约 0.09%
                assert!(pnl_pct > Decimal::ZERO);
            }
            _ => panic!("Expected TrailingStop"),
        }
    }

    #[test]
    fn test_no_double_position() {
        let mut executor = SimpleExecutor::with_default_config();

        executor.open_position(
            "BTCUSDT",
            TradeDirection::Long,
            dec!(100),
            dec!(0.01),
            Utc::now(),
        ).unwrap();

        // 尝试重复开仓
        let result = executor.open_position(
            "ETHUSDT",
            TradeDirection::Long,
            dec!(2000),
            dec!(0.01),
            Utc::now(),
        );

        assert!(result.is_err());
    }
}
