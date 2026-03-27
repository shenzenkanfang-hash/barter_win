//! 事件驱动交易引擎
//!
//! 替代 TraderManager 的新架构：
//! - 单事件循环：所有 Tick 串行处理，无 spawn
//! - Channel 驱动：通过 mpsc::Receiver<Tick> 接收数据
//! - 无后台任务：调用者控制生命周期
//!
//! # 使用方式
//! ```ignore
//! use f_engine::core::EventDrivenEngine;
//!
//! let mut engine = EventDrivenEngine::new("BTCUSDT");
//!
//! // 启动事件循环
//! engine.run(tick_rx).await;
//! ```

use crate::types::{OrderRequest, OrderType, Side, TradingDecision, TradingAction};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::HashMap;
use tokio::sync::mpsc;

/// 引擎状态
#[derive(Debug, Clone)]
struct EngineState {
    /// 指标缓存（按品种）
    indicators: HashMap<String, IndicatorData>,
    /// 持仓状态
    positions: HashMap<String, PositionState>,
    /// 统计
    stats: EngineStats,
}

#[derive(Debug, Clone, Default)]
struct IndicatorData {
    ema5: Option<Decimal>,
    ema20: Option<Decimal>,
    rsi: Option<Decimal>,
    price_history: Vec<Decimal>,
}

#[derive(Debug, Clone)]
struct PositionState {
    has_position: bool,
    entry_price: Decimal,
    side: Option<Side>,
}

impl Default for PositionState {
    fn default() -> Self {
        Self {
            has_position: false,
            entry_price: Decimal::ZERO,
            side: None,
        }
    }
}

#[derive(Debug, Default, Clone)]
struct EngineStats {
    total_orders: u64,
    total_trades: u64,
    total_errors: u64,
}

/// 事件驱动交易引擎
///
/// 核心设计：
/// - 单事件循环：Tick 串行处理，无并发竞态
/// - 零 spawn：直接 await，不启动后台任务
/// - 零 sleep：事件驱动，无轮询
/// - 背压控制：channel send().await 自动处理
pub struct EventDrivenEngine {
    /// 交易品种
    symbol: String,
    /// 引擎状态
    state: EngineState,
}

impl EventDrivenEngine {
    /// 创建引擎
    pub fn new(symbol: String) -> Self {
        Self {
            symbol,
            state: EngineState {
                indicators: HashMap::new(),
                positions: HashMap::new(),
                stats: EngineStats::default(),
            },
        }
    }

    /// 单事件循环
    ///
    /// 接收 Tick，串行处理，无 spawn
    pub async fn run(&mut self, mut tick_rx: mpsc::Receiver<b_data_source::Tick>) {
        tracing::info!("[Engine] {} 事件循环启动", self.symbol);

        while let Some(tick) = tick_rx.recv().await {
            self.on_tick(tick).await;
        }

        tracing::info!("[Engine] {} 事件循环结束", self.symbol);
    }

    /// 处理单个 Tick
    async fn on_tick(&mut self, tick: b_data_source::Tick) {
        let symbol = tick.symbol.clone();

        // 1. 更新指标
        self.update_indicators(&tick);

        // 2. 策略决策
        if let Some(decision) = self.decide(&tick) {
            // 3. 风控检查
            if let Some(order) = self.check_risk(&decision) {
                // 4. 异步下单
                self.submit_order(order).await;
            }
        }

        // 5. 更新持仓
        self.update_position(&tick);
    }

    /// 更新指标（增量计算）
    fn update_indicators(&mut self, tick: &b_data_source::Tick) {
        let symbol = &tick.symbol;
        
        let ind = self.state.indicators
            .entry(symbol.clone())
            .or_insert_with(IndicatorData::default);

        // 添加价格到历史
        ind.price_history.push(tick.price);
        if ind.price_history.len() > 100 {
            ind.price_history.remove(0);
        }

        // 计算 EMA
        if ind.price_history.len() >= 5 {
            ind.ema5 = Some(Self::calc_ema(&ind.price_history, 5));
        }
        if ind.price_history.len() >= 20 {
            ind.ema20 = Some(Self::calc_ema(&ind.price_history, 20));
        }

        // 计算 RSI
        if ind.price_history.len() >= 14 {
            ind.rsi = Some(Self::calc_rsi(&ind.price_history));
        }
    }

    /// 计算 EMA
    fn calc_ema(prices: &[Decimal], period: usize) -> Decimal {
        if prices.is_empty() {
            return Decimal::ZERO;
        }
        let k = dec!(2) / Decimal::from(period + 1);
        let mut ema = prices[0];
        for price in prices.iter().skip(1) {
            ema = *price * k + ema * (Decimal::ONE - k);
        }
        ema
    }

    /// 计算 RSI
    fn calc_rsi(prices: &[Decimal]) -> Decimal {
        let mut gains = Decimal::ZERO;
        let mut losses = Decimal::ZERO;
        
        for window in prices.windows(2) {
            let change = window[1] - window[0];
            if change > Decimal::ZERO {
                gains += change;
            } else {
                losses -= change;
            }
        }
        
        let avg_gain = gains / dec!(14);
        let avg_loss = losses / dec!(14);
        
        if avg_loss.is_zero() {
            return dec!(100);
        }
        
        let rs = avg_gain / avg_loss;
        dec!(100) - dec!(100) / (dec!(1) + rs)
    }

    /// 策略决策
    fn decide(&self, tick: &b_data_source::Tick) -> Option<TradingDecision> {
        let symbol = &tick.symbol;
        
        let ind = self.state.indicators.get(symbol)?;
        
        let (ema5, ema20, rsi) = match (ind.ema5, ind.ema20, ind.rsi) {
            (Some(e5), Some(e20), Some(r)) => (e5, e20, r),
            _ => return None,
        };
        
        let position = self.state.positions.get(symbol)
            .cloned()
            .unwrap_or_default();

        // 无持仓 -> 买入条件
        if !position.has_position {
            if ema5 > ema20 && rsi < dec!(70) && rsi > dec!(30) {
                return Some(TradingDecision::new(
                    TradingAction::Long,
                    "EMA金叉",
                    80,
                    symbol.clone(),
                    dec!(0.01),
                    tick.price,
                    chrono::Utc::now().timestamp(),
                ));
            }
        }
        // 有持仓 -> 卖出条件
        else {
            if ema5 < ema20 || rsi > dec!(70) {
                return Some(TradingDecision::new(
                    TradingAction::Flat,
                    "EMA死叉或RSI超买",
                    80,
                    symbol.clone(),
                    dec!(0.01),
                    tick.price,
                    chrono::Utc::now().timestamp(),
                ));
            }
        }
        
        None
    }

    /// 风控检查（简化版，完整风控由外部执行）
    fn check_risk(&self, decision: &TradingDecision) -> Option<OrderRequest> {
        // 构造订单
        let order = OrderRequest {
            symbol: decision.symbol.clone(),
            side: match decision.action {
                TradingAction::Long => Side::Buy,
                TradingAction::Short | TradingAction::Flat | 
                TradingAction::Add | TradingAction::Reduce | 
                TradingAction::Hedge | TradingAction::Wait => Side::Sell,
            },
            order_type: OrderType::Market,
            qty: decision.qty,
            price: Some(decision.price),
        };

        // 简化风控：只检查数量是否合理
        if order.qty <= Decimal::ZERO {
            return None;
        }

        Some(order)
    }

    /// 异步下单
    ///
    /// 注意：Executor::send_order_simple 需要额外的参数
    /// 当前实现仅记录日志，实际下单由外部执行
    async fn submit_order(&mut self, order: OrderRequest) {
        let symbol = order.symbol.clone();
        let side = order.side;

        // 记录订单（实际下单由沙盒层执行）
        self.state.stats.total_orders += 1;
        tracing::info!("[Order] {} {:?} 记录: qty={}", symbol, side, order.qty);
    }

    /// 更新持仓状态
    fn update_position(&mut self, tick: &b_data_source::Tick) {
        // 预留扩展点
    }

    /// 获取统计
    pub fn stats(&self) -> &EngineStats {
        &self.state.stats
    }
}
