//! Tick 握手控制器
//!
//! 用于同步模式：等待引擎处理完成后再推送下一个 Tick。
//!
//! # 架构
//! ```text
//! Generator ──Tick──► Engine
//!     ▲                 │
//!     │                 │ 处理
//!     │                 ▼
//!     └────── done ─────┘
//! ```
//!
//! # 使用方式
//! ```ignore
//! // 1. 创建握手通道
//! let (mut tick_tx, tick_rx, mut done_rx) = create_handshake_channel();
//!
//! // 2. 生成器等待 done 信号
//! for tick in generator {
//!     done_rx.recv().await;  // 等待上一个处理完
//!     tick_tx.send(tick).await;
//! }
//!
//! // 3. 引擎处理完后发送 done
//! while let Some(tick) = tick_rx.recv().await {
//!     process(tick).await;
//!     done_tx.send(()).await;
//! }
//! ```

use tokio::sync::mpsc;

// ============================================================================
// 握手通道
// ============================================================================

/// Tick 握手通道
///
/// 组成：
/// - tick_tx: 发送 Tick 给引擎
/// - tick_rx: 引擎接收 Tick
/// - done_tx: 引擎发送完成信号
/// - done_rx: 生成器接收完成信号
#[derive(Debug)]
pub struct TickHandshakeChannel {
    /// Tick 发送端（生成器用）
    pub tick_tx: mpsc::UnboundedSender<SimulatedTick>,
    /// Tick 接收端（引擎用）
    pub tick_rx: mpsc::UnboundedReceiver<SimulatedTick>,
    /// 完成信号发送端（引擎用）
    pub done_tx: mpsc::Sender<()>,
    /// 完成信号接收端（生成器用）
    pub done_rx: mpsc::Receiver<()>,
}

/// Tick 发送端（生成器侧）
#[derive(Debug)]
pub struct TickSender {
    /// unbounded sender 不会阻塞，可以 clone
    pub tick_tx: mpsc::UnboundedSender<SimulatedTick>,
    pub done_rx: mpsc::Receiver<()>,
}

/// Tick 接收端（引擎侧）
#[derive(Debug)]
pub struct TickReceiver {
    pub tick_rx: mpsc::UnboundedReceiver<SimulatedTick>,
    pub done_tx: mpsc::Sender<()>,
}

// Re-export SimulatedTick for convenience
pub use super::tick_generator::SimulatedTick;

/// 创建握手通道
///
/// # 参数
/// - tick_buffer: Tick channel 缓冲区大小（握手模式填 1，忽略）
/// - done_buffer: 完成信号 channel 缓冲区大小（建议 1）
///
/// Tick channel 使用 unbounded sender（可 clone，不阻塞），
/// Done channel 使用 bounded sender（需要等待 engine 发信号）。
///
/// # 返回
/// - (Sender, Receiver)
pub fn create_handshake_channel(
    _tick_buffer: usize,
    done_buffer: usize,
) -> (TickSender, TickReceiver) {
    // Tick channel: unbounded（不阻塞，可 clone）
    let (tick_tx, tick_rx) = mpsc::unbounded_channel::<SimulatedTick>();
    // Done channel: bounded（engine 发 ack，generator 收）
    let (done_tx, done_rx) = mpsc::channel::<()>(done_buffer);

    (
        TickSender {
            tick_tx,
            done_rx,
        },
        TickReceiver {
            tick_rx,
            done_tx,
        },
    )
}

/// 创建自由流通道（无握手）
///
/// # 参数
/// - buffer: Tick channel 缓冲区大小
///
/// # 返回
/// - (tick_tx, tick_rx)
pub fn create_stream_channel<T>(buffer: usize) -> (mpsc::Sender<T>, mpsc::Receiver<T>) {
    mpsc::channel(buffer)
}

impl TickSender {
    /// 自由流模式：直接发送 Tick（不等待）
    ///
    /// UnboundedSender::send 不会阻塞
    pub fn send(&self, tick: SimulatedTick) -> Result<(), TickSendError> {
        self.tick_tx
            .send(tick)
            .map_err(|_| TickSendError("channel closed".to_string()))?;
        Ok(())
    }
}

impl TickReceiver {
    /// 接收 Tick，处理完后发送完成信号
    ///
    /// 这是引擎侧的主要使用方式
    pub async fn recv_and_ack(&mut self) -> Option<SimulatedTick> {
        let tick = self.tick_rx.recv().await?;
        Some(tick)
    }

    /// 接收 Tick，处理完后发送完成信号（带回调）
    ///
    /// # 参数
    /// - f: 处理函数，返回 Future
    pub async fn recv_and_process<F, Fut>(&mut self, f: F)
    where
        F: FnOnce(SimulatedTick) -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        if let Some(tick) = self.tick_rx.recv().await {
            f(tick).await;
            self.done_tx.send(()).await.ok();
        }
    }

    /// 发送完成信号
    pub async fn ack(&self) -> Result<(), mpsc::error::SendError<()>> {
        self.done_tx.send(()).await
    }

    /// 尝试发送完成信号（非阻塞）
    pub fn try_ack(&self) -> Result<(), mpsc::error::TrySendError<()>> {
        self.done_tx.try_send(())
    }
}

// ============================================================================
// 错误类型
// ============================================================================

/// Tick 发送错误
#[derive(Debug)]
pub struct TickSendError(pub String);

impl std::fmt::Display for TickSendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TickSendError: {}", self.0)
    }
}

impl std::error::Error for TickSendError {}

/// Tick 尝试发送错误
#[derive(Debug, thiserror::Error)]
pub enum TickTrySendError {
    #[error("channel full")]
    Full,
    #[error("channel closed")]
    Closed,
}

// ============================================================================
// 握手模式生成器
// ============================================================================

use super::tick_generator::StreamTickGenerator;

/// 握手模式的 Tick 生成器包装
///
/// 组合 StreamTickGenerator 和 TickSender，
/// 提供 await-able 的 next() 方法，自动处理握手
pub struct HandshakeGenerator {
    /// 底层 Tick 生成器
    generator: StreamTickGenerator,
    /// Tick 发送器（带握手）
    sender: TickSender,
    /// 第一个 Tick 标志（第一次不需要等待 done 信号）
    first_tick: bool,
}

impl HandshakeGenerator {
    /// 创建握手生成器
    pub fn new(generator: StreamTickGenerator, sender: TickSender) -> Self {
        Self {
            generator,
            sender,
            first_tick: true,
        }
    }

    /// 生成并发送下一个 Tick，等待引擎处理完成
    ///
    /// 流程：
    /// 1. 如果不是第一个 Tick，等待 done 信号
    /// 2. 生成下一个 Tick
    /// 3. UnboundedSender::send() 不会阻塞（无界缓冲），直接推入 channel
    /// 4. yield_now() 让出执行权给 engine task
    /// 5. 等待 done 信号
    /// 6. 返回 Tick
    pub async fn next(&mut self) -> Option<SimulatedTick> {
        // 1. 等待上一个处理完成（第一个 Tick 除外）
        if !self.first_tick {
            self.sender.done_rx.recv().await;
        }
        self.first_tick = false;

        // 2. 生成 Tick
        let tick = self.generator.next()?;

        // 3. 发送 Tick（unbounded 不会阻塞，除非 receiver 关闭）
        self.sender.send(tick.clone()).ok();

        // 4. 强制让出执行权给 engine task（多线程 runtime 需要显式 yield）
        tokio::task::yield_now().await;

        // 5. 等待 engine 完成信号
        self.sender.done_rx.recv().await;

        // 6. 返回 Tick
        Some(tick)
    }

    /// 获取底层生成器引用（用于非握手操作）
    pub fn generator(&self) -> &StreamTickGenerator {
        &self.generator
    }
}

/// 引擎侧的 Tick 处理循环
///
/// # 示例
/// ```ignore
/// engine_loop(receiver, |tick| async move {
///     // 处理 tick
/// }).await;
/// ```
pub async fn engine_loop<F, Fut>(mut receiver: TickReceiver, mut process_fn: F)
where
    F: FnMut(SimulatedTick) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    while let Some(tick) = receiver.recv_and_ack().await {
        process_fn(tick).await;
        receiver.ack().await.ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{KLine, Period};
    use chrono::Utc;
    use rust_decimal_macros::dec;

    fn create_test_klines() -> Vec<KLine> {
        vec![
            KLine {
                symbol: "BTCUSDT".to_string(),
                period: Period::Minute(1),
                open: dec!(50000),
                high: dec!(50500),
                low: dec!(49500),
                close: dec!(50200),
                volume: dec!(100),
                timestamp: Utc::now(),
                is_closed: false,
            },
            KLine {
                symbol: "BTCUSDT".to_string(),
                period: Period::Minute(1),
                open: dec!(50200),
                high: dec!(51000),
                low: dec!(50100),
                close: dec!(50800),
                volume: dec!(150),
                timestamp: Utc::now(),
                is_closed: false,
            },
        ]
    }

    /// 测试握手通道的基本流程：
    /// - Generator: next() 推送 tick 并等待 done
    /// - Engine: 用 next() 返回的 tick 处理，发 ack
    #[tokio::test]
    async fn test_handshake_channel() {
        let (sender, receiver) = create_handshake_channel(1, 1);

        // 生成测试 Tick
        let klines = create_test_klines();
        let generator = StreamTickGenerator::new("BTCUSDT".to_string(), Box::new(klines.into_iter()));

        let mut handshake_gen = HandshakeGenerator::new(generator, sender);
        let engine_receiver = receiver;

        // 第一个 Tick - 不需要等待 done
        let tick1 = handshake_gen.next().await.unwrap();
        assert_eq!(tick1.is_last_in_kline, false); // 第一个 K 线的第一个 tick

        // 引擎：用 tick1 处理
        assert_eq!(tick1.symbol, "BTCUSDT");
        // 发 ack 唤醒 generator
        engine_receiver.ack().await.unwrap();

        // 发送更多 tick 直到 K 线闭合（60 tick）
        for i in 1..60 {
            let tick = handshake_gen.next().await.unwrap();
            if i == 59 {
                assert!(tick.is_last_in_kline, "第 60 个 tick 应该是 K 线最后一根");
            }
            // 引擎：用 tick 处理完发 ack
            engine_receiver.ack().await.ok();
        }

        // 第二个 K 线
        let tick60 = handshake_gen.next().await.unwrap();
        assert!(tick60.is_last_in_kline, "第二个 K 线的最后一个 tick");
    }

    /// 测试握手循环：Generator 推送 + Engine 同步处理
    #[tokio::test]
    async fn test_handshake_loop() {
        let (sender, receiver) = create_handshake_channel(1, 1);
        let klines = create_test_klines();
        let generator = StreamTickGenerator::new("BTCUSDT".to_string(), Box::new(klines.into_iter()));
        let mut handshake_gen = HandshakeGenerator::new(generator, sender);
        let engine_receiver = receiver;

        let mut tick_count = 0;
        let mut kline_closed_count = 0;

        loop {
            // Generator: 推送 tick 并等 engine 完成
            let tick = match handshake_gen.next().await {
                Some(t) => t,
                None => break, // 数据耗尽
            };
            tick_count += 1;
            if tick.is_last_in_kline {
                kline_closed_count += 1;
            }
            // Engine: 发 ack 唤醒 generator
            engine_receiver.ack().await.ok();
        }

        // 2 根 K 线 * 60 tick
        assert_eq!(tick_count, 120);
        assert_eq!(kline_closed_count, 2);
    }
}
