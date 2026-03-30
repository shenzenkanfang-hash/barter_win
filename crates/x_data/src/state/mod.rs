//! state - 状态管理模块
//!
//! 提供统一的状态管理 trait 和系统快照类型。

pub mod traits;
pub mod component;
pub mod center;

pub use traits::{
    StateViewer,
    StateManager,
    UnifiedStateView,
    SystemSnapshot,
};

pub use component::{
    ComponentStatus,
    ComponentState,
};

pub use center::{
    StateCenter,
    StateCenterImpl,
    StateCenterTrait,
};
