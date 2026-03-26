//! h_15m - 分钟级策略
//!
//! 交易容器框架，对标 Python 版本的 singleAssetTrader。
//!
//! ## 包含内容
//!
//! - 交易规则管理 (Coins_Info)
//! - 余额/风控管理 (balance_management)
//! - 仓位风险管理 (_set_position_risk)
//! - 数据持久化 (_store_data / _load_data)
//! - 循环控制 (startloop / stoploop / run)
//! - 心跳检查 (health_check)
//! - 开仓/平仓 (open_position / close_position) - 策略逻辑注入点

#![forbid(unsafe_code)]

pub mod trader;

// Re-export 所有公开类型
pub use trader::{
    // 枚举
    Direction, TradeStatus, TrendStatus, Status,
    // 配置
    Config, CoinsInfo,
    // 账户
    AccountInformation, WalletBalance,
    // 持仓
    PositionInfo, PositionOrder, PositionRisk,
    // 交易器
    Trader,
    // 健康检查
    HealthCheck, RuntimeInfo,
    // 辅助函数
    calculate_avg_price,
};
