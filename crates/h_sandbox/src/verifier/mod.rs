//! Verifier - 生产级验证系统
//!
//! 验证系统:
//! 1. 状态一致性 - 沙盒与 Trader 状态对账
//! 2. 资金曲线 - PnL 独立计算验证
//! 3. 故障注入 - 网络延迟、数据丢失、价格跳空
//! 4. 性能基准 - Tick-to-Decision 延迟分布
//! 5. 报告生成 - Markdown + JSON 报告

#![forbid(unsafe_code)]

pub mod consistency;
pub mod fund_curve;
pub mod fault_injector;
pub mod performance;
pub mod reporter;

pub use consistency::{ConsistencyChecker, ConsistencyReport};
pub use fund_curve::{FundCurveValidator, FundCurveReport};
pub use fault_injector::{FaultInjector, FaultConfig, FaultType};
pub use performance::{PerformanceBenchmark, PerformanceReport};
pub use reporter::{VerificationReporter, VerificationReport};