//! b_signal_generator.rs - Pin策略信号生成器
//!
//! 完全对齐 pin_main.py 逻辑：
//! - 市场状态检测 (PIN/RANGE/TREND)
//! - Pin状态机 (INITIAL → FIRST_OPEN → DOUBLE_ADD/HEDGE → POS_LOCKED)
//! - 开仓/加仓/平仓/对冲

#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use super::a_market_data::MarketData;
use super::c_status::PinStatus;
use x_data::position::PositionSide as XPositionSide;
use x_data::trading::signal::{StrategySignal, TradeCommand, StrategyId};

/// Pin策略常量（完全对齐 pin_main.py）
pub mod config {
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;

    /// 1%盈利平仓阈值
    pub const PROFIT_THRESHOLD: Decimal = dec!(0.01);
    /// 价格下跌2%阈值（对冲触发）
    pub const PRICE_DOWN_THRESHOLD: Decimal = dec!(0.98);
    /// 价格上涨2%阈值（对冲/加仓触发）
    pub const PRICE_UP_THRESHOLD: Decimal = dec!(1.02);
    /// 价格下跌10%硬阈值
    pub const PRICE_DOWN_HARD: Decimal = dec!(0.90);
    /// 价格上涨10%硬阈值
    pub const PRICE_UP_HARD: Decimal = dec!(1.10);
    /// 多头加仓价格阈值
    pub const LONG_ADD_THRESHOLD: Decimal = dec!(1.02);
    /// 多头加仓硬阈值
    pub const LONG_ADD_HARD: Decimal = dec!(1.08);
    /// 空头加仓价格阈值
    pub const SHORT_ADD_THRESHOLD: Decimal = dec!(0.98);
    /// 空头加仓硬阈值
    pub const SHORT_ADD_HARD: Decimal = dec!(0.92);
}

/// 市场状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketStatus {
    Pin,   // 插针（极端波动）
    Range, // 震荡（低波动、低动能）
    Trend, // 趋势（有明确方向）
    Invalid, // 数据无效（超时/异常）
}

/// 订单类型（对齐 pin_main.py）
#[derive(Debug, Clone, Copy)]
pub enum OrderType {
    Open = 0,        // 初始开仓
    Hedge = 1,       // 对冲开仓
    DoubleAdd = 2,   // 翻倍加仓
    DoubleClose = 3, // 翻倍平仓
    DayHedge = 4,    // 日线对冲
    DayClose = 5,    // 日线关仓
}

/// Pin策略信号生成器
pub struct PinSignalGenerator {
    /// 当前市场状态
    market_status: MarketStatus,
    /// 当前Pin状态
    current_status: PinStatus,
    /// 上次订单时间戳
    last_order_timestamp: i64,
}

impl PinSignalGenerator {
    pub fn new() -> Self {
        Self {
            market_status: MarketStatus::Invalid,
            current_status: PinStatus::Initial,
            last_order_timestamp: 0,
        }
    }

    /// 设置市场状态
    pub fn set_market_status(&mut self, status: MarketStatus) {
        self.market_status = status;
    }

    /// 获取市场状态
    pub fn market_status(&self) -> MarketStatus {
        self.market_status
    }

    /// 设置当前状态
    pub fn set_status(&mut self, status: PinStatus) {
        self.current_status = status;
    }

    /// 获取当前状态
    pub fn current_status(&self) -> PinStatus {
        self.current_status
    }

    /// 检测市场状态
    pub fn detect_market_status(volatility: f64, tr_ratio: f64) -> MarketStatus {
        if volatility > 0.15 || tr_ratio > 0.1 {
            MarketStatus::Pin
        } else if volatility < 0.02 && tr_ratio < 0.01 {
            MarketStatus::Range
        } else {
            MarketStatus::Trend
        }
    }

    /// 生成交易信号（主入口）
    ///
    /// 对应 pin_main.py 的 open_position() 逻辑
    pub fn generate(
        &mut self,
        market: &MarketData,
        // Pin入场检测器
        check_long_entry: impl Fn() -> bool,
        check_short_entry: impl Fn() -> bool,
        check_long_add: impl Fn() -> bool,
        check_short_add: impl Fn() -> bool,
        check_long_hedge: impl Fn() -> bool,
        check_short_hedge: impl Fn() -> bool,
        check_exit_high_vol: impl Fn() -> bool,
        // 日线检测器
        check_day_long_entry: impl Fn() -> bool,
        check_day_short_entry: impl Fn() -> bool,
        check_day_long_hedge: impl Fn() -> bool,
        check_day_short_hedge: impl Fn() -> bool,
    ) -> Option<PinTradeSignal> {
        let price = market.price;
        let long_price = market.long_price_all;
        let short_price = market.short_price_all;
        let long_num = market.long_num_all;
        let short_num = market.short_num_all;

        // 更新市场状态
        self.market_status = Self::detect_market_status(market.volatility, market.volatility);

        match self.market_status {
            MarketStatus::Pin => self.generate_pin_signals(
                market, price, long_price, short_price, long_num, short_num,
                check_long_entry, check_short_entry, check_long_add, check_short_add,
                check_long_hedge, check_short_hedge, check_exit_high_vol,
            ),
            MarketStatus::Trend => self.generate_trend_signals(
                price, long_price, short_price, long_num, short_num,
                check_day_long_entry, check_day_short_entry, check_day_long_hedge, check_day_short_hedge,
            ),
            _ => None,
        }
    }

    /// PIN行情信号生成
    ///
    /// 对应 pin_main.py 插针行情开仓逻辑
    #[allow(clippy::too_many_arguments)]
    fn generate_pin_signals(
        &mut self,
        market: &MarketData,
        price: Decimal,
        long_price: Decimal,
        short_price: Decimal,
        long_num: Decimal,
        short_num: Decimal,
        check_long_entry: impl Fn() -> bool,
        check_short_entry: impl Fn() -> bool,
        check_long_add: impl Fn() -> bool,
        check_short_add: impl Fn() -> bool,
        check_long_hedge: impl Fn() -> bool,
        check_short_hedge: impl Fn() -> bool,
        check_exit_high_vol: impl Fn() -> bool,
    ) -> Option<PinTradeSignal> {
        // ========== 盈利1%平仓 ==========
        // 多头盈利1%平仓
        if long_num > Decimal::ZERO && price > long_price * (dec!(1) + config::PROFIT_THRESHOLD) {
            let side_count = market.kline_1m.as_ref()
                .map(|k| k.volume > Decimal::ZERO)
                .unwrap_or(false);
            if side_count {
                self.current_status = PinStatus::LongInitial;
                return Some(PinTradeSignal {
                    order_type: OrderType::DoubleClose,
                    side: XPositionSide::Long,
                    qty: long_num,
                    reason: "多头盈利1%平仓".to_string(),
                });
            }
        }
        // 空头盈利1%平仓
        if short_num > Decimal::ZERO && price < short_price * (dec!(1) - config::PROFIT_THRESHOLD) {
            let side_count = market.kline_1m.as_ref()
                .map(|k| k.volume > Decimal::ZERO)
                .unwrap_or(false);
            if side_count {
                self.current_status = PinStatus::ShortInitial;
                return Some(PinTradeSignal {
                    order_type: OrderType::DoubleClose,
                    side: XPositionSide::Short,
                    qty: short_num,
                    reason: "空头盈利1%平仓".to_string(),
                });
            }
        }

        // ========== 最低平仓线平仓 ==========
        // 多头最低平仓
        if long_num > Decimal::ZERO && price < long_price * (dec!(1) + config::PROFIT_THRESHOLD) {
            let side_count = market.kline_1m.as_ref()
                .map(|k| k.volume > Decimal::ZERO)
                .unwrap_or(false);
            if side_count {
                self.current_status = PinStatus::LongInitial;
                return Some(PinTradeSignal {
                    order_type: OrderType::DoubleClose,
                    side: XPositionSide::Long,
                    qty: long_num,
                    reason: "多头最低平仓线".to_string(),
                });
            }
        }
        // 空头最低平仓
        if short_num > Decimal::ZERO && price < short_price * (dec!(1) - config::PROFIT_THRESHOLD) {
            let side_count = market.kline_1m.as_ref()
                .map(|k| k.volume > Decimal::ZERO)
                .unwrap_or(false);
            if side_count {
                self.current_status = PinStatus::ShortInitial;
                return Some(PinTradeSignal {
                    order_type: OrderType::DoubleClose,
                    side: XPositionSide::Short,
                    qty: short_num,
                    reason: "空头最低平仓线".to_string(),
                });
            }
        }

        // ========== 初始/多头第一次开仓 ==========
        if matches!(self.current_status, PinStatus::Initial | PinStatus::LongInitial) {
            if check_long_entry() {
                self.current_status = PinStatus::LongFirstOpen;
                return Some(PinTradeSignal {
                    order_type: OrderType::Open,
                    side: XPositionSide::Long,
                    qty: self.calc_open_qty(market),
                    reason: "多头第一次开仓".to_string(),
                });
            }
        }

        // ========== 初始/空头第一次开仓 ==========
        if matches!(self.current_status, PinStatus::Initial | PinStatus::ShortInitial) {
            if check_short_entry() {
                self.current_status = PinStatus::ShortFirstOpen;
                return Some(PinTradeSignal {
                    order_type: OrderType::Open,
                    side: XPositionSide::Short,
                    qty: self.calc_open_qty(market),
                    reason: "空头第一次开仓".to_string(),
                });
            }
        }

        // ========== 多头翻倍加仓 ==========
        if matches!(self.current_status, PinStatus::LongFirstOpen | PinStatus::LongDoubleAdd | PinStatus::HedgeEnter) {
            let cond1 = check_long_add() && price > long_price * config::LONG_ADD_THRESHOLD;
            let cond2 = price > long_price * config::LONG_ADD_HARD;
            if cond1 || cond2 {
                self.current_status = PinStatus::LongDoubleAdd;
                return Some(PinTradeSignal {
                    order_type: OrderType::DoubleAdd,
                    side: XPositionSide::Long,
                    qty: long_num,
                    reason: "多头翻倍加仓".to_string(),
                });
            }
        }

        // ========== 空头翻倍加仓 ==========
        if matches!(self.current_status, PinStatus::ShortFirstOpen | PinStatus::ShortDoubleAdd | PinStatus::HedgeEnter) {
            let cond1 = check_short_add() && price < short_price * config::SHORT_ADD_THRESHOLD;
            let cond2 = price < short_price * config::SHORT_ADD_HARD;
            if cond1 || cond2 {
                self.current_status = PinStatus::ShortDoubleAdd;
                return Some(PinTradeSignal {
                    order_type: OrderType::DoubleAdd,
                    side: XPositionSide::Short,
                    qty: short_num,
                    reason: "空头翻倍加仓".to_string(),
                });
            }
        }

        // ========== 多头第一次开仓 → 进入对冲 ==========
        if self.current_status == PinStatus::LongFirstOpen && short_num > Decimal::ZERO {
            let cond1 = check_long_hedge() && price < long_price * config::PRICE_DOWN_THRESHOLD;
            let cond2 = price < long_price * config::PRICE_DOWN_HARD;
            if cond1 || cond2 {
                self.current_status = PinStatus::HedgeEnter;
                return Some(PinTradeSignal {
                    order_type: OrderType::Hedge,
                    side: XPositionSide::Short,
                    qty: long_num,
                    reason: "多头对冲".to_string(),
                });
            }
        }

        // ========== 空头第一次开仓 → 进入对冲 ==========
        if self.current_status == PinStatus::ShortFirstOpen && long_num > Decimal::ZERO {
            let cond1 = check_short_hedge() && price > short_price * config::PRICE_UP_THRESHOLD;
            let cond2 = price > short_price * config::PRICE_UP_HARD;
            if cond1 || cond2 {
                self.current_status = PinStatus::HedgeEnter;
                return Some(PinTradeSignal {
                    order_type: OrderType::Hedge,
                    side: XPositionSide::Long,
                    qty: short_num,
                    reason: "空头对冲".to_string(),
                });
            }
        }

        // ========== 波动率降低 → 切换到日线趋势模式 ==========
        if check_exit_high_vol() {
            self.market_status = MarketStatus::Trend;
            self.current_status = PinStatus::PosLocked;
            tracing::info!("[Pin] 波动率降低，切换为趋势模式，仓位锁定");
        }

        None
    }

    /// 趋势行情信号生成
    ///
    /// 对应 pin_main.py 趋势行情处理逻辑
    #[allow(clippy::too_many_arguments)]
    fn generate_trend_signals(
        &mut self,
        _price: Decimal,
        long_price: Decimal,
        short_price: Decimal,
        long_num: Decimal,
        short_num: Decimal,
        check_day_long_entry: impl Fn() -> bool,
        check_day_short_entry: impl Fn() -> bool,
        check_day_long_hedge: impl Fn() -> bool,
        check_day_short_hedge: impl Fn() -> bool,
    ) -> Option<PinTradeSignal> {
        // 仅在 PosLocked 状态处理
        if self.current_status != PinStatus::PosLocked {
            return None;
        }

        // ========== 保本平仓 ==========
        // 有持仓时平仓
        if long_price > Decimal::ZERO || short_price > Decimal::ZERO {
            // 先平空头
            if short_num > Decimal::ZERO {
                self.current_status = PinStatus::LongDayAllow;
                return Some(PinTradeSignal {
                    order_type: OrderType::Hedge,
                    side: XPositionSide::Short,
                    qty: short_num,
                    reason: "保本平空头".to_string(),
                });
            }
            // 再平多头
            if long_num > Decimal::ZERO {
                self.current_status = PinStatus::ShortDayAllow;
                return Some(PinTradeSignal {
                    order_type: OrderType::Hedge,
                    side: XPositionSide::Long,
                    qty: long_num,
                    reason: "保本平多头".to_string(),
                });
            }
        }

        // ========== 指标平仓 ==========
        // 多头平仓（对冲空头）
        if long_num > Decimal::ZERO && short_num > Decimal::ZERO && check_day_long_entry() {
            self.current_status = PinStatus::LongDayAllow;
            return Some(PinTradeSignal {
                order_type: OrderType::DayClose,
                side: XPositionSide::Short,
                qty: short_num,
                reason: "日线多头平仓".to_string(),
            });
        }
        // 空头平仓（对冲多头）
        if long_num > Decimal::ZERO && short_num > Decimal::ZERO && check_day_short_entry() {
            self.current_status = PinStatus::ShortDayAllow;
            return Some(PinTradeSignal {
                order_type: OrderType::DayClose,
                side: XPositionSide::Long,
                qty: long_num,
                reason: "日线空头平仓".to_string(),
            });
        }

        // ========== 日线对冲 ==========
        if long_num > Decimal::ZERO && short_num == Decimal::ZERO && check_day_long_hedge() {
            if self.current_status == PinStatus::LongDayAllow {
                self.current_status = PinStatus::PosLocked;
                return Some(PinTradeSignal {
                    order_type: OrderType::DayHedge,
                    side: XPositionSide::Short,
                    qty: long_num,
                    reason: "日线多头对冲".to_string(),
                });
            }
        }
        if short_num > Decimal::ZERO && long_num == Decimal::ZERO && check_day_short_hedge() {
            if self.current_status == PinStatus::ShortDayAllow {
                self.current_status = PinStatus::PosLocked;
                return Some(PinTradeSignal {
                    order_type: OrderType::DayHedge,
                    side: XPositionSide::Long,
                    qty: short_num,
                    reason: "日线空头对冲".to_string(),
                });
            }
        }

        None
    }

    /// 计算开仓数量
    fn calc_open_qty(&self, _market: &MarketData) -> Decimal {
        dec!(0.05) // TODO: 实际计算
    }
}

impl Default for PinSignalGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// Pin交易信号
#[derive(Debug, Clone)]
pub struct PinTradeSignal {
    pub order_type: OrderType,
    pub side: XPositionSide,
    pub qty: Decimal,
    pub reason: String,
}

impl PinTradeSignal {
    /// 转换为 StrategySignal
    pub fn to_strategy_signal(&self, symbol: &str, price: Decimal) -> StrategySignal {
        let command = match self.order_type {
            OrderType::Open => TradeCommand::Open,
            OrderType::Hedge => TradeCommand::HedgeOpen,
            OrderType::DoubleAdd => TradeCommand::Add,
            OrderType::DoubleClose | OrderType::DayClose => TradeCommand::FlatPosition,
            OrderType::DayHedge => TradeCommand::HedgeOpen,
        };

        let (target_price, full_close) = match self.order_type {
            OrderType::DoubleClose | OrderType::DayClose => (Decimal::ZERO, true),
            _ => (price, false),
        };

        StrategySignal {
            command,
            direction: self.side,
            quantity: self.qty,
            target_price,
            strategy_id: StrategyId::new_pin_minute(symbol),
            position_ref: None,
            full_close,
            stop_loss_price: None,
            take_profit_price: None,
            reason: self.reason.clone(),
            confidence: 80,
            timestamp: chrono::Utc::now().timestamp(),
        }
    }
}
