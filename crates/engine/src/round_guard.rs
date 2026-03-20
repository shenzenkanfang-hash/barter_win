use std::sync::atomic::{AtomicU64, Ordering};

/// 一轮编码守卫 - 确保计算周期一致性
///
/// 目的: 确保同一轮内的计算可以互相操作，防止竞态条件
///
/// 设计:
/// - round_id 原子递增
/// - 每次流水线启动时获取当前 round_id
/// - 计算完成后验证 round_id 未变化
pub struct RoundGuard {
    round_id: AtomicU64,
}

impl RoundGuard {
    /// 创建新的 RoundGuard
    pub fn new() -> Self {
        Self {
            round_id: AtomicU64::new(0),
        }
    }

    /// 获取下一轮次ID (原子递增)
    /// 返回递增后的新值
    pub fn next_round_id(&self) -> u64 {
        self.round_id.fetch_add(1, Ordering::SeqCst);
        self.round_id.load(Ordering::SeqCst)
    }

    /// 获取当前轮次ID (不递增)
    pub fn current_round_id(&self) -> u64 {
        self.round_id.load(Ordering::SeqCst)
    }

    /// 验证 round_id 是否未变化
    /// 返回 true 表示轮次一致，可以继续
    pub fn verify(&self, expected: u64) -> bool {
        self.round_id.load(Ordering::SeqCst) == expected
    }
}

impl Default for RoundGuard {
    fn default() -> Self {
        Self::new()
    }
}

/// RoundGuard 的作用域封装
/// 用于确保一轮计算的完整性
pub struct RoundGuardScope<'a> {
    guard: &'a RoundGuard,
    round_id: u64,
    valid: bool,
}

impl<'a> RoundGuardScope<'a> {
    /// 开始一轮计算
    pub fn new(guard: &'a RoundGuard) -> Self {
        let round_id = guard.next_round_id();
        Self {
            guard,
            round_id,
            valid: true,
        }
    }

    /// 获取轮次ID
    pub fn round_id(&self) -> u64 {
        self.round_id
    }

    /// 验证当前轮次是否有效
    pub fn is_valid(&self) -> bool {
        self.valid && self.guard.verify(self.round_id)
    }

    /// 标记轮次结束 (可选，用于日志)
    pub fn finish(mut self) {
        self.valid = false;
    }
}

impl<'a> Drop for RoundGuardScope<'a> {
    fn drop(&mut self) {
        self.valid = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_round_guard_basic() {
        let guard = RoundGuard::new();
        assert_eq!(guard.current_round_id(), 0);

        let id1 = guard.next_round_id();
        assert_eq!(id1, 1);
        assert_eq!(guard.current_round_id(), 1);

        let id2 = guard.next_round_id();
        assert_eq!(id2, 2);
    }

    #[test]
    fn test_round_guard_verify() {
        let guard = RoundGuard::new();
        let id = guard.next_round_id();
        assert!(guard.verify(id));

        guard.next_round_id(); // 递增
        assert!(!guard.verify(id));
    }

    #[test]
    fn test_round_guard_scope() {
        let guard = RoundGuard::new();
        {
            let scope = RoundGuardScope::new(&guard);
            assert_eq!(scope.round_id(), 1);
            assert!(scope.is_valid());
        } // scope 结束

        // 新一轮
        let scope = RoundGuardScope::new(&guard);
        assert_eq!(scope.round_id(), 2);
        assert!(scope.is_valid());
    }
}
