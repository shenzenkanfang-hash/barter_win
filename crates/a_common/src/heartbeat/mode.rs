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
                use rand::Rng;
                use rand::SeedableRng;
                thread_local! {
                    static RNG: std::cell::RefCell<rand::prelude::SmallRng> =
                        std::cell::RefCell::new(rand::prelude::SmallRng::from_entropy());
                }
                RNG.with(|rng| rng.borrow_mut().gen_range(0..*n) == 0)
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
