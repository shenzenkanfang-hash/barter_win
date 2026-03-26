//! f_trader.rs - 引擎对接层
//!
//! 职责：配置存储、状态更新、健康检查
//! 复杂逻辑（自循环、SQLite）放 e_trader_flow.rs

#![forbid(unsafe_code)]

use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use super::c_status::PinStatus;
use x_data::position::{PositionSide, LocalPosition};
use x_data::trading::signal::{StrategySignal, TradeCommand};

/// 交易器配置
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TraderConfig {
    pub symbol: String,
    pub interval_ms: u64,
    pub initial_ratio: Decimal,
}

impl Default for TraderConfig {
    fn default() -> Self {
        Self {
            symbol: "BTCUSDT".to_string(),
            interval_ms: 100,
            initial_ratio: dec!(0.05),
        }
    }
}

/// 交易器（引擎对接）
pub struct Trader {
    pub config: TraderConfig,
    pub status: PinStatus,
    pub position: Option<LocalPosition>,
}

impl Trader {
    pub fn new(symbol: &str) -> Self {
        Self {
            config: TraderConfig {
                symbol: symbol.to_string(),
                ..Default::default()
            },
            status: PinStatus::Initial,
            position: None,
        }
    }

    /// 下单后更新状态
    pub fn update_status(&mut self, signal: &StrategySignal) {
        match signal.command {
            TradeCommand::Open => {
                self.status = match signal.direction {
                    PositionSide::Long => PinStatus::LongFirstOpen,
                    PositionSide::Short => PinStatus::ShortFirstOpen,
                    _ => PinStatus::Initial,
                };
            }
            TradeCommand::Add => {
                self.status = match signal.direction {
                    PositionSide::Long => PinStatus::LongDoubleAdd,
                    PositionSide::Short => PinStatus::ShortDoubleAdd,
                    _ => PinStatus::Initial,
                };
            }
            TradeCommand::FlatPosition | TradeCommand::FlatAll => {
                self.status = PinStatus::Initial;
            }
            TradeCommand::HedgeOpen => {
                self.status = PinStatus::HedgeEnter;
            }
            TradeCommand::HedgeClose => {
                self.status = PinStatus::PosLocked;
            }
            _ => {}
        }
    }

    /// 健康检查
    pub fn health(&self) -> TraderHealth {
        TraderHealth {
            symbol: self.config.symbol.clone(),
            status: self.status.as_str().to_string(),
            has_position: self.position.is_some(),
        }
    }
}

impl Default for Trader {
    fn default() -> Self {
        Self::new("BTCUSDT")
    }
}

/// 健康状态
#[derive(Debug, Clone)]
pub struct TraderHealth {
    pub symbol: String,
    pub status: String,
    pub has_position: bool,
}
