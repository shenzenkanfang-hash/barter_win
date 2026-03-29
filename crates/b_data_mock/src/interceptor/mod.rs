//! Mock 拦截器模块
//!
//! 提供 Tick 数据和订单的拦截包装，用于心跳延迟监控
//!
//! ## 设计原则
//! - **非侵入性**: 不修改原有业务逻辑，只在外层包装
//! - **可选启用**: 通过 feature flag 控制
//! - **零开销**: 禁用时完全无开销

pub mod tick_interceptor;
pub mod order_interceptor;

pub use tick_interceptor::TickInterceptor;
pub use order_interceptor::OrderInterceptor;
