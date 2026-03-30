//! trade_lock.rs - 全局交易锁
//!
//! 确保同时只有一个协程在交易，防止多策略并发下单导致的资金占用冲突。

#![forbid(unsafe_code)]

use parking_lot::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// TradeLock 错误类型
#[derive(Debug, Clone, thiserror::Error)]
pub enum LockError {
    #[error("锁已被 [{0}] 持有")]
    AlreadyHeld(String),

    #[error("无锁可释放: 当前没有持有者")]
    NotHeld,

    #[error("释放失败: 锁被 [{0}] 持有，不是当前请求者")]
    NotOwner(String),
}

/// TradeLock 全局交易锁
///
/// 确保同时只有一个协程在交易。用于防止多策略并发下单导致的资金占用冲突。
///
/// # 架构
/// - 使用 RwLock 管理锁持有者
/// - 使用 AtomicU64 版本号实现乐观锁检测
/// - TradeLockGuard RAII 模式自动释放锁
#[derive(Debug, Clone)]
pub struct TradeLock {
    /// 当前持有锁的策略 ID
    holder: Arc<RwLock<Option<String>>>,
    /// 锁的版本号（用于乐观锁检测）
    version: Arc<AtomicU64>,
}

impl TradeLock {
    /// 创建新的 TradeLock
    pub fn new() -> Self {
        Self {
            holder: Arc::new(RwLock::new(None)),
            version: Arc::new(AtomicU64::new(0)),
        }
    }

    /// 创建带 Arc 的 TradeLock（用于跨线程共享）
    pub fn new_arc() -> Arc<Self> {
        Arc::new(Self::new())
    }

    /// 尝试获取锁
    ///
    /// 如果锁未被持有或已被当前 strategy_id 持有，则获取成功并返回 Guard。
    /// 否则返回 LockError::AlreadyHeld。
    pub fn acquire(&self, strategy_id: &str) -> Result<TradeLockGuard, LockError> {
        let mut holder = self.holder.write();
        match holder.as_ref() {
            Some(h) if h != strategy_id => {
                Err(LockError::AlreadyHeld(h.clone()))
            }
            _ => {
                *holder = Some(strategy_id.to_string());
                self.version.fetch_add(1, Ordering::SeqCst);
                Ok(TradeLockGuard {
                    lock: self.clone(),
                    strategy_id: strategy_id.to_string(),
                })
            }
        }
    }

    /// 释放锁（由 Guard Drop 时自动调用）
    ///
    /// 只有锁的持有者才能释放锁。
    pub fn release(&self, strategy_id: &str) -> Result<(), LockError> {
        let mut holder = self.holder.write();
        match holder.as_ref() {
            Some(h) if h == strategy_id => {
                *holder = None;
                self.version.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
            Some(h) => {
                Err(LockError::NotOwner(h.clone()))
            }
            None => {
                Err(LockError::NotHeld)
            }
        }
    }

    /// 检查锁是否被持有
    pub fn is_held(&self) -> bool {
        self.holder.read().is_some()
    }

    /// 获取当前锁的持有者（如果有）
    pub fn holder(&self) -> Option<String> {
        self.holder.read().clone()
    }

    /// 获取当前版本号
    pub fn version(&self) -> u64 {
        self.version.load(Ordering::SeqCst)
    }

    /// 强制释放锁（仅用于紧急情况，如策略崩溃）
    ///
    /// # Safety
    /// 仅在确认持有者已崩溃且不会恢复时使用。
    pub fn force_release(&self) {
        let mut holder = self.holder.write();
        *holder = None;
        self.version.fetch_add(1, Ordering::SeqCst);
    }
}

impl Default for TradeLock {
    fn default() -> Self {
        Self::new()
    }
}

// ==================== TradeLockGuard ====================

/// TradeLock Guard - RAII 锁 guard
///
/// 当 Guard 被 drop 时，锁自动释放。
#[derive(Debug)]
pub struct TradeLockGuard {
    lock: TradeLock,
    strategy_id: String,
}

impl TradeLockGuard {
    /// 获取持有者 ID
    pub fn strategy_id(&self) -> &str {
        &self.strategy_id
    }

    /// 获取锁的版本号
    pub fn version(&self) -> u64 {
        self.lock.version()
    }
}

impl Drop for TradeLockGuard {
    fn drop(&mut self) {
        if let Err(e) = self.lock.release(&self.strategy_id) {
            tracing::warn!(
                "TradeLockGuard drop: release failed for strategy {}: {}",
                self.strategy_id, e
            );
        }
    }
}

// ==================== Async Guard ====================

/// AsyncTradeLockGuard - 异步 RAII 锁 guard
///
/// 用于异步上下文中的锁管理。
#[derive(Debug)]
pub struct AsyncTradeLockGuard {
    guard: Option<TradeLockGuard>,
}

impl AsyncTradeLockGuard {
    /// 创建新的异步 guard（从同步 guard 转换）
    pub fn from_sync(guard: TradeLockGuard) -> Self {
        Self {
            guard: Some(guard),
        }
    }

    /// 获取持有者 ID
    pub fn strategy_id(&self) -> Option<&str> {
        self.guard.as_ref().map(|g| g.strategy_id())
    }

    /// 释放锁
    pub fn release(mut self) {
        self.guard = None;
    }
}

impl Drop for AsyncTradeLockGuard {
    fn drop(&mut self) {
        if let Some(guard) = self.guard.take() {
            drop(guard);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trade_lock_new() {
        let lock = TradeLock::new();
        assert!(!lock.is_held());
        assert!(lock.holder().is_none());
        assert_eq!(lock.version(), 0);
    }

    #[test]
    fn test_trade_lock_acquire_success() {
        let lock = TradeLock::new();

        let guard = lock.acquire("strategy_1").unwrap();
        assert!(lock.is_held());
        assert_eq!(lock.holder(), Some("strategy_1".to_string()));
        assert_eq!(guard.strategy_id(), "strategy_1");
        assert_eq!(lock.version(), 1);
    }

    #[test]
    fn test_trade_lock_acquire_same_strategy() {
        let lock = TradeLock::new();

        // 第一次获取
        let _guard1 = lock.acquire("strategy_1").unwrap();
        assert!(lock.is_held());

        // 同一 strategy_id 可以重入
        let _guard2 = lock.acquire("strategy_1").unwrap();
        assert!(lock.is_held());
        assert_eq!(lock.version(), 2);
    }

    #[test]
    fn test_trade_lock_acquire_different_strategy_blocked() {
        let lock = TradeLock::new();

        // strategy_1 获取锁
        let _guard1 = lock.acquire("strategy_1").unwrap();

        // strategy_2 无法获取
        let result = lock.acquire("strategy_2");
        assert!(matches!(result, Err(LockError::AlreadyHeld(h)) if h == "strategy_1"));
    }

    #[test]
    fn test_trade_lock_release_success() {
        let lock = TradeLock::new();

        let guard = lock.acquire("strategy_1").unwrap();
        assert!(lock.is_held());

        drop(guard);
        assert!(!lock.is_held());
        assert!(lock.holder().is_none());
        assert_eq!(lock.version(), 2);
    }

    #[test]
    fn test_trade_lock_release_not_owner() {
        let lock = TradeLock::new();

        let _guard = lock.acquire("strategy_1").unwrap();

        // strategy_2 尝试释放 strategy_1 的锁
        let result = lock.release("strategy_2");
        assert!(matches!(result, Err(LockError::NotOwner(h)) if h == "strategy_1"));
    }

    #[test]
    fn test_trade_lock_release_not_held() {
        let lock = TradeLock::new();

        let result = lock.release("strategy_1");
        assert!(matches!(result, Err(LockError::NotHeld)));
    }

    #[test]
    fn test_trade_lock_force_release() {
        let lock = TradeLock::new();

        let _guard = lock.acquire("strategy_1").unwrap();
        assert!(lock.is_held());

        lock.force_release();
        assert!(!lock.is_held());
        assert!(lock.holder().is_none());
    }

    #[test]
    fn test_trade_lock_guard_auto_release() {
        let lock = TradeLock::new();

        {
            let _guard = lock.acquire("strategy_1").unwrap();
            assert!(lock.is_held());
        } // guard 在此作用域结束时会自动释放

        assert!(!lock.is_held());
    }

    #[test]
    fn test_trade_lock_version_increment() {
        let lock = TradeLock::new();
        assert_eq!(lock.version(), 0);

        let guard1 = lock.acquire("strategy_1").unwrap();
        assert_eq!(lock.version(), 1);

        let guard2 = lock.acquire("strategy_1").unwrap();
        assert_eq!(lock.version(), 2);

        // guard1 先 drop（栈先进后出），此时锁被 guard2 持有
        // drop guard1 会释放锁（因为我们不跟踪重入计数）
        drop(guard1);
        assert_eq!(lock.version(), 3);

        // guard2 drop 时锁已被 guard1 释放，holder 为 None，不再增加版本
        drop(guard2);
        assert_eq!(lock.version(), 3);
    }

    #[test]
    fn test_trade_lock_arc() {
        let lock = Arc::new(TradeLock::new());

        let lock_clone = lock.clone();
        std::thread::spawn(move || {
            let _guard = lock_clone.acquire("strategy_1").unwrap();
            assert_eq!(lock_clone.holder(), Some("strategy_1".to_string()));
        }).join().unwrap();
    }

    #[tokio::test]
    async fn test_async_guard() {
        let lock = TradeLock::new_arc();
        let guard = lock.acquire("strategy_1").unwrap();

        let async_guard = AsyncTradeLockGuard::from_sync(guard);
        assert_eq!(async_guard.strategy_id(), Some("strategy_1"));

        assert!(lock.is_held());
        drop(async_guard);
        assert!(!lock.is_held());
    }

    #[test]
    fn test_trade_lock_default() {
        let lock = TradeLock::default();
        assert!(!lock.is_held());
        assert_eq!(lock.version(), 0);
    }
}
