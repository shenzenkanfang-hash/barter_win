//! mock_ws - 模拟WebSocket数据注入
//!
//! 用历史K线数据替代真实Binance WebSocket数据
//!
//! # 模块结构
//! - `noise.rs` - 高斯噪声生成器
//! - `tick_generator.rs` - K线转Tick流生成器
//! - `handshake.rs` - Tick握手控制器（同步模式）
//!
//! # 两种模式
//! ## 自由流模式
//! ```ignore
//! for tick in generator {
//!     tx.send(tick).await;
//! }
//! ```
//!
//! ## 握手模式
//! ```ignore
//! let (sender, receiver) = create_handshake_channel(1, 1);
//! let mut gen = HandshakeGenerator::new(generator, sender);
//! let mut recv = receiver;
//!
//! // 生成器侧
//! let tick = gen.next().await;
//!
//! // 引擎侧
//! let tick = recv.recv_and_ack().await;
//! recv.ack().await;
//! ```

pub mod noise;
pub mod tick_generator;
pub mod handshake;

pub use tick_generator::{StreamTickGenerator, SimulatedTick};
pub use handshake::{
    create_handshake_channel,
    create_stream_channel,
    TickHandshakeChannel,
    TickSender,
    TickReceiver,
    HandshakeGenerator,
    engine_loop,
    TickSendError,
    TickTrySendError,
};
