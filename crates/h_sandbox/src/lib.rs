#![forbid(unsafe_code)]

pub mod config;
pub mod simulator;
pub mod gateway;
pub mod tick_generator;

pub use config::ShadowConfig;
pub use simulator::{Account, OrderEngine, Position, Side};
pub use gateway::ShadowBinanceGateway;
pub use tick_generator::{TickGenerator, TickDriver, SimulatedTick, KLineInput};
