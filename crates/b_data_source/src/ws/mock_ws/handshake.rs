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
    pub tick_tx: mpsc::Sender<SimulatedTick>,
    /// Tick 接收端（引擎用）
    pub tick_rx: mpsc::Receiver<SimulatedTick>,
    /// 完成信号发送端（引擎用）
    pub done_tx: mpsc::Sender<()>,
    /// 完成信号接收端（生成器用）
    pub done_rx: mpsc::Receiver<()>,
}

/// Tick 发送端（生成器侧）
#[derive(Debug)]
pub struct TickSender {
    pub tick_tx: mpsc::Sender<SimulatedTick>,
    pub done_rx: mpsc::Receiver<()>,
}

/// Tick 接收端（引擎侧）
#[derive(Debug)]
pub struct TickReceiver {
    pub tick_rx: mpsc::Receiver<SimulatedTick>,
    pub done_tx: mpsc::Sender<()>,
}

// Re-export SimulatedTick for convenience
pub use super::tick_generator::SimulatedTick;

/// 创建握手通道
///
/// # 参数
/// - tick_buffer: Tick channel 缓冲区大小（建议 1，用于握手模式）
/// - done_buffer: 完成信号 channel 缓冲区大小（建议 1）
///
/// # 返回
/// - (Sender, Receiver)
pub fn create_handshake_channel(
    tick_buffer: usize,
    done_buffer: usize,
) -> (TickSender, TickReceiver) {
    let (tick_tx, tick_rx) = mpsc::channel::<SimulatedTick>(tick_buffer);
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
    /// 握手模式：等待完成信号，然后发送 Tick
    ///
    /// 调用这个方法会阻塞，直到收到 done 信号
    pub async fn send_with_handshake(&mut self, tick: SimulatedTick) -> Result<(), TickSendError> {
        // 1. 等待上一个 Tick 处理完成
        self.done_rx.recv().await;

        // 2. 发送新 Tick
        self.tick_tx
            .send(tick)
            .await
            .map_err(|_| TickSendError("channel closed".to_string()))?;

        Ok(())
    }

    /// 自由流模式：直接发送 Tick（不等待）
    pub async fn send(&self, tick: SimulatedTick) -> Result<(), TickSendError> {
        self.tick_tx
            .send(tick)
            .await
            .map_err(|_| TickSendError("channel closed".to_string()))?;
        Ok(())
    }

    /// 尝试发送（非阻塞）
    pub fn try_send(&self, tick: SimulatedTick) -> Result<(), TickTrySendError> {
        self.tick_tx
            .try_send(tick)
            .map_err(|e| match e {
                mpsc::error::TrySendError::Full(_) => TickTrySendError::Full,
                mpsc::error::TrySendError::Closed(_) => TickTrySendError::Closed,
            })?;
        Ok(())
    }

    /// 检查 channel 是否还有容量
    pub fn remaining_capacity(&self) -> usize {
        self.tick_tx.capacity()
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
    /// 3. 发送 Tick
    /// 4. 返回 Tick（用于后续处理）
    pub async fn next(&mut self) -> Option<SimulatedTick> {
        // 1. 等待上一个处理完成（第一个 Tick 除外）
        if !self.first_tick {
            self.sender.done_rx.recv().await;
        }
        self.first_tick = false;

        // 2. 生成 Tick
        let tick = self.generator.next()?;

        // 3. 发送 Tick
        self.sender.tick_tx.send(tick.clone()).await.ok();

        // 4. 返回 Tick
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
            },
        ]
    }

    #[tokio::test]
    async fn test_handshake_channel() {
        let (sender, receiver) = create_handshake_channel(1, 1);

        // 生成测试 Tick
        let klines = create_test_klines();
        let generator = StreamTickGenerator::new("BTCUSDT".to_string(), Box::new(klines.into_iter()));

        let mut handshake_gen = HandshakeGenerator::new(generator, sender);
        let mut engine_receiver = receiver;

        // 第一个 Tick - 不需要等待
        let tick1 = handshake_gen.next().await.unwrap();
        assert_eq!(tick1.is_last_in_kline, false); // 第一个 K 线的第一个 tick

        // 确认引擎收到
        let received1 = engine_receiver.recv_and_ack().await.unwrap();
        assert_eq!(received1.symbol, "BTCUSDT");

        // 引擎发送完成信号
        engine_receiver.ack().await.unwrap();

        // 发送更多 tick 直到 K 线闭合
        for i in 1..60 {
            let tick = handshake_gen.next().await.unwrap();
            if i == 59 {
                assert!(tick.is_last_in_kline, "第 60 个 tick 应该是 K 线最后一根");
            }
            engine_receiver.recv_and_ack().await;
            engine_receiver.ack().await.ok();
        }

        // 第二个 K 线
        let tick60 = handshake_gen.next().await.unwrap();
        assert!(tick60.is_last_in_kline, "第二个 K 线的最后一个 tick");
    }

    #[tokio::test]
    async fn test_handshake_loop() {
        let (sender, receiver) = create_handshake_channel(1, 1);
        let klines = create_test_klines();
        let generator = StreamTickGenerator::new("BTCUSDT".to_string(), Box::new(klines.into_iter()));
        let mut handshake_gen = HandshakeGenerator::new(generator, sender);
        let mut receiver = receiver;

        let mut tick_count = 0;
        let mut kline_closed_count = 0;

        // 模拟引擎处理循环
        while let Some(tick) = receiver.recv_and_ack().await {
            tick_count += 1;
            if tick.is_last_in_kline {
                kline_closed_count += 1;
            }
            receiver.ack().await.ok();
        }

        // 生成器应该已经结束
        assert_eq!(tick_count, 120); // 2 根 K 线 * 60 tick
        assert_eq!(kline_closed_count, 2);
    }
}
