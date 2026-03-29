// Note: AtomicU8 and Ordering reserved for future sync features
#[allow(unused_imports)]
use std::sync::atomic::{AtomicU8, Ordering};

/// 报告模式 - 支持 Full/Sampling/Disabled 三种模式
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportMode {
    /// 正常模式 - 每次都报到
    Full,
    /// 采样模式 - 1/N 概率报到
    Sampling(u32),
    /// 禁用模式 - 完全关闭
    Disabled,
}

impl ReportMode {
    /// 判断是否应该报到
    pub fn should_report(&self) -> bool {
        match self {
            ReportMode::Full => true,
            ReportMode::Sampling(n) => {
                use std::collections::hash_map::RandomState;
                use std::hash::{BuildHasher, Hash, Hasher};
                let rng: u32 = RandomState::new()
                    .build_hasher()
                    .finish() as u32;
                rng % n == 0
            },
            ReportMode::Disabled => false,
        }
    }
}

impl Default for ReportMode {
    fn default() -> Self {
        ReportMode::Full
    }
}
