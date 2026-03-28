//! f_engine 事件驱动核心
//!
//! # 架构原则
//! - **零轮询**: `recv().await` 阻塞等待，无 `tokio::time::sleep`
//! - **零 spawn**: 无 `tokio::spawn` 后台任务，单事件循环
//! - **单事件流**: 一个 Tick 驱动完整处理链
//!
//! # 事件流
//! ```
//! StreamTickGenerator
//!         │ tick_tx.send()
//!         ▼
//!   mpsc::channel
//!         │ tick_rx.recv().await
//!         ▼
//!   EventEngine::run()
//!         │
//!         ├─► on_tick()     → 完整处理链
//!         │      │
//!         │      ├─► update_store()
//!         │      ├─► calc_indicators()
//!         │      ├─► strategy.decide()
//!         │      ├─► risk_checker.pre_check()
//!         │      └─► gateway.place_order()
//! ```

pub mod event_bus;
pub mod event_engine;

#[cfg(test)]
mod tests;

pub use event_bus::{EventBus, EventBusHandle, DEFAULT_CHANNEL_BUFFER};
pub use event_engine::{
    EventEngine, EngineConfig, EngineState, 
    TickEvent, KlineData, IndicatorCache, PineColor,
    Strategy, ExchangeGateway,
    AccountInfo, PositionInfo, OrderResult, GatewayError,
    IndicatorCalculator,
};
