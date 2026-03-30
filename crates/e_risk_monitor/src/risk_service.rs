//! risk_service.rs - 风控服务统一接口
//!
//! 提供两阶段风控检查接口：
//! - Stage 1 (PreCheck): 下单前风控预检
//! - Stage 2 (ReCheck): 成交后风控复核
//!
//! Engine/Executor 通过此 trait 调用统一的风控服务。

#![forbid(unsafe_code)]

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// 风控服务错误类型
#[derive(Debug, Clone, thiserror::Error)]
pub enum RiskServiceError {
    #[error("风控拒绝: {0}")]
    Rejected(String),

    #[error("服务不可用: {0}")]
   Unavailable(String),

    #[error("无效参数: {0}")]
    InvalidArgument(String),

    #[error("系统错误: {0}")]
    SystemError(String),
}

impl Default for RiskServiceError {
    fn default() -> Self {
        RiskServiceError::SystemError("Unknown error".to_string())
    }
}

// ==================== Stage 1: PreCheck 请求/结果 ====================

/// 第一阶段预检请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreCheckRequest {
    /// 订单ID
    pub order_id: String,
    /// 品种符号
    pub symbol: String,
    /// 策略ID
    pub strategy_id: String,
    /// 订单方向 (long/short)
    pub side: RiskSide,
    /// 订单数量
    pub quantity: Decimal,
    /// 订单价值（名义价值）
    pub order_value: Decimal,
    /// 可用余额
    pub available_balance: Decimal,
    /// 当前持仓数量（带方向）
    pub current_position_qty: Decimal,
    /// 总权益
    pub total_equity: Decimal,
}

/// 订单方向
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskSide {
    Long,
    Short,
}

/// 第一阶段预检结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreCheckResult {
    /// 是否通过
    pub passed: bool,
    /// 冻结金额
    pub frozen_amount: Decimal,
    /// 拒绝原因
    pub reject_reason: Option<String>,
    /// 风险等级
    pub risk_level: RiskLevel,
    /// 检查时间戳
    pub checked_at: DateTime<Utc>,
}

/// 风险等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskLevel {
    /// 低风险
    Low,
    /// 中风险
    Medium,
    /// 高风险
    High,
    /// 拒绝
    Rejected,
}

impl Default for RiskLevel {
    fn default() -> Self {
        RiskLevel::Low
    }
}

// ==================== Stage 2: ReCheck 请求/结果 ====================

/// 第二阶段复核请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReCheckRequest {
    /// 订单ID
    pub order_id: String,
    /// 成交ID
    pub fill_id: String,
    /// 品种符号
    pub symbol: String,
    /// 策略ID
    pub strategy_id: String,
    /// 订单方向
    pub side: RiskSide,
    /// 成交数量
    pub filled_qty: Decimal,
    /// 成交价格
    pub fill_price: Decimal,
    /// 成交价值
    pub fill_value: Decimal,
    /// 成交时间
    pub fill_time: DateTime<Utc>,
    /// 当前持仓数量（带方向）
    pub current_position_qty: Decimal,
    /// 可用余额
    pub available_balance: Decimal,
    /// 总权益
    pub total_equity: Decimal,
}

/// 第二阶段复核结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReCheckResult {
    /// 是否通过
    pub passed: bool,
    /// 警告信息
    pub warnings: Vec<String>,
    /// 需要关注的标志
    pub alert_flagged: bool,
    /// 复核时间戳
    pub checked_at: DateTime<Utc>,
}

// ==================== 风控快照 ====================

/// 风控服务快照（用于监控）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskSnapshot {
    /// 服务名称
    pub service_name: String,
    /// 是否可用
    pub available: bool,
    /// 预检通过率
    pub pre_check_pass_rate: f64,
    /// 复核拒绝率
    pub re_check_reject_rate: f64,
    /// 总预检次数
    pub total_pre_checks: u64,
    /// 总复核次数
    pub total_re_checks: u64,
    /// 最后检查时间
    pub last_check_at: Option<DateTime<Utc>>,
}

impl Default for RiskSnapshot {
    fn default() -> Self {
        Self {
            service_name: "RiskService".to_string(),
            available: true,
            pre_check_pass_rate: 1.0,
            re_check_reject_rate: 0.0,
            total_pre_checks: 0,
            total_re_checks: 0,
            last_check_at: None,
        }
    }
}

// ==================== RiskService Trait ====================

/// RiskService - 风控服务统一接口
///
/// 提供两阶段风控检查：
/// 1. PreCheck（下单前）：检查订单是否允许下单
/// 2. ReCheck（成交后）：检查成交是否符合风控规则
#[async_trait]
pub trait RiskService: Send + Sync {
    /// 获取服务名称
    fn name(&self) -> &str;

    /// 健康检查
    async fn health_check(&self) -> Result<bool, RiskServiceError>;

    // ==================== Stage 1: PreCheck ====================

    /// 下单前风控预检
    ///
    /// 在订单发送前调用，检查：
    /// - 资金是否足够
    /// - 持仓比例是否超限
    /// - 波动率模式是否允许
    /// - 品种是否注册
    async fn pre_check(&self, request: PreCheckRequest) -> Result<PreCheckResult, RiskServiceError>;

    /// 冻结订单保证金（预占）
    ///
    /// 如果 pre_check 通过，调用此方法冻结保证金
    async fn freeze(&self, order_id: &str, amount: Decimal) -> Result<(), RiskServiceError>;

    /// 解冻订单保证金（取消）
    ///
    /// 如果订单取消或失败，调用此方法释放冻结的保证金
    async fn unfreeze(&self, order_id: &str) -> Result<Decimal, RiskServiceError>;

    // ==================== Stage 2: ReCheck ====================

    /// 成交后风控复核
    ///
    /// 在订单成交后调用，检查：
    /// - 持仓是否超限
    /// - 名义价值是否合理
    /// - 是否需要触发告警
    async fn re_check(&self, request: ReCheckRequest) -> Result<ReCheckResult, RiskServiceError>;

    /// 确认保证金（从冻结转为占用）
    ///
    /// 订单完全成交后调用，将冻结的保证金转为已占用
    async fn confirm(&self, order_id: &str, fill_value: Decimal) -> Result<(), RiskServiceError>;

    // ==================== 监控 ====================

    /// 获取风控快照
    async fn snapshot(&self) -> Result<RiskSnapshot, RiskServiceError>;

    /// 重置统计计数器
    async fn reset_stats(&self) -> Result<(), RiskServiceError>;
}

// ==================== RiskServiceAdapter ====================

/// RiskServiceAdapter - 将现有 RiskPreChecker 适配为 RiskService
///
/// 用于将已有的 RiskPreChecker 组件适配为统一的 RiskService 接口。
pub struct RiskServiceAdapter {
    inner: Arc<dyn RiskService>,
}

impl RiskServiceAdapter {
    /// 从内部服务创建适配器
    pub fn new(inner: Arc<dyn RiskService>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl RiskService for RiskServiceAdapter {
    fn name(&self) -> &str {
        self.inner.name()
    }

    async fn health_check(&self) -> Result<bool, RiskServiceError> {
        self.inner.health_check().await
    }

    async fn pre_check(&self, request: PreCheckRequest) -> Result<PreCheckResult, RiskServiceError> {
        self.inner.pre_check(request).await
    }

    async fn freeze(&self, order_id: &str, amount: Decimal) -> Result<(), RiskServiceError> {
        self.inner.freeze(order_id, amount).await
    }

    async fn unfreeze(&self, order_id: &str) -> Result<Decimal, RiskServiceError> {
        self.inner.unfreeze(order_id).await
    }

    async fn re_check(&self, request: ReCheckRequest) -> Result<ReCheckResult, RiskServiceError> {
        self.inner.re_check(request).await
    }

    async fn confirm(&self, order_id: &str, fill_value: Decimal) -> Result<(), RiskServiceError> {
        self.inner.confirm(order_id, fill_value).await
    }

    async fn snapshot(&self) -> Result<RiskSnapshot, RiskServiceError> {
        self.inner.snapshot().await
    }

    async fn reset_stats(&self) -> Result<(), RiskServiceError> {
        self.inner.reset_stats().await
    }
}

// ==================== MockRiskService（测试用） ====================

/// MockRiskService - 用于测试的模拟风控服务
#[derive(Debug, Clone, Default)]
pub struct MockRiskService {
    pub pre_check_pass: bool,
    pub re_check_pass: bool,
}

impl MockRiskService {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_pre_check_pass(mut self, pass: bool) -> Self {
        self.pre_check_pass = pass;
        self
    }

    pub fn with_re_check_pass(mut self, pass: bool) -> Self {
        self.re_check_pass = pass;
        self
    }
}

#[async_trait]
impl RiskService for MockRiskService {
    fn name(&self) -> &str {
        "MockRiskService"
    }

    async fn health_check(&self) -> Result<bool, RiskServiceError> {
        Ok(true)
    }

    async fn pre_check(&self, _request: PreCheckRequest) -> Result<PreCheckResult, RiskServiceError> {
        if self.pre_check_pass {
            Ok(PreCheckResult {
                passed: true,
                frozen_amount: Decimal::ZERO,
                reject_reason: None,
                risk_level: RiskLevel::Low,
                checked_at: Utc::now(),
            })
        } else {
            Ok(PreCheckResult {
                passed: false,
                frozen_amount: Decimal::ZERO,
                reject_reason: Some("Mock rejection".to_string()),
                risk_level: RiskLevel::Rejected,
                checked_at: Utc::now(),
            })
        }
    }

    async fn freeze(&self, _order_id: &str, _amount: Decimal) -> Result<(), RiskServiceError> {
        Ok(())
    }

    async fn unfreeze(&self, _order_id: &str) -> Result<Decimal, RiskServiceError> {
        Ok(Decimal::ZERO)
    }

    async fn re_check(&self, _request: ReCheckRequest) -> Result<ReCheckResult, RiskServiceError> {
        if self.re_check_pass {
            Ok(ReCheckResult {
                passed: true,
                warnings: vec![],
                alert_flagged: false,
                checked_at: Utc::now(),
            })
        } else {
            Ok(ReCheckResult {
                passed: false,
                warnings: vec!["Mock rejection".to_string()],
                alert_flagged: true,
                checked_at: Utc::now(),
            })
        }
    }

    async fn confirm(&self, _order_id: &str, _fill_value: Decimal) -> Result<(), RiskServiceError> {
        Ok(())
    }

    async fn snapshot(&self) -> Result<RiskSnapshot, RiskServiceError> {
        Ok(RiskSnapshot::default())
    }

    async fn reset_stats(&self) -> Result<(), RiskServiceError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[tokio::test]
    async fn test_mock_risk_service_pre_check_pass() {
        let service = MockRiskService::new().with_pre_check_pass(true);

        let request = PreCheckRequest {
            order_id: "order_1".to_string(),
            symbol: "BTCUSDT".to_string(),
            strategy_id: "trend".to_string(),
            side: RiskSide::Long,
            quantity: dec!(0.1),
            order_value: dec!(1000),
            available_balance: dec!(10000),
            current_position_qty: dec!(0),
            total_equity: dec!(10000),
        };

        let result = service.pre_check(request).await.unwrap();
        assert!(result.passed);
        assert_eq!(result.risk_level, RiskLevel::Low);
    }

    #[tokio::test]
    async fn test_mock_risk_service_pre_check_reject() {
        let service = MockRiskService::new().with_pre_check_pass(false);

        let request = PreCheckRequest {
            order_id: "order_1".to_string(),
            symbol: "BTCUSDT".to_string(),
            strategy_id: "trend".to_string(),
            side: RiskSide::Long,
            quantity: dec!(0.1),
            order_value: dec!(1000),
            available_balance: dec!(10000),
            current_position_qty: dec!(0),
            total_equity: dec!(10000),
        };

        let result = service.pre_check(request).await.unwrap();
        assert!(!result.passed);
        assert_eq!(result.risk_level, RiskLevel::Rejected);
        assert!(result.reject_reason.is_some());
    }

    #[tokio::test]
    async fn test_mock_risk_service_re_check_pass() {
        let service = MockRiskService::new().with_re_check_pass(true);

        let request = ReCheckRequest {
            order_id: "order_1".to_string(),
            fill_id: "fill_1".to_string(),
            symbol: "BTCUSDT".to_string(),
            strategy_id: "trend".to_string(),
            side: RiskSide::Long,
            filled_qty: dec!(0.1),
            fill_price: dec!(10000),
            fill_value: dec!(1000),
            fill_time: Utc::now(),
            current_position_qty: dec!(0.1),
            available_balance: dec!(9000),
            total_equity: dec!(10000),
        };

        let result = service.re_check(request).await.unwrap();
        assert!(result.passed);
        assert!(!result.alert_flagged);
    }

    #[tokio::test]
    async fn test_mock_risk_service_re_check_with_warning() {
        let service = MockRiskService::new().with_re_check_pass(false);

        let request = ReCheckRequest {
            order_id: "order_1".to_string(),
            fill_id: "fill_1".to_string(),
            symbol: "BTCUSDT".to_string(),
            strategy_id: "trend".to_string(),
            side: RiskSide::Long,
            filled_qty: dec!(0.1),
            fill_price: dec!(10000),
            fill_value: dec!(1000),
            fill_time: Utc::now(),
            current_position_qty: dec!(0.1),
            available_balance: dec!(9000),
            total_equity: dec!(10000),
        };

        let result = service.re_check(request).await.unwrap();
        assert!(!result.passed);
        assert!(result.alert_flagged);
        assert!(!result.warnings.is_empty());
    }

    #[tokio::test]
    async fn test_mock_risk_service_health_check() {
        let service = MockRiskService::new();
        let result = service.health_check().await.unwrap();
        assert!(result);
    }

    #[test]
    fn test_risk_level_default() {
        let level = RiskLevel::default();
        assert_eq!(level, RiskLevel::Low);
    }

    #[test]
    fn test_risk_service_error_default() {
        let error = RiskServiceError::default();
        assert!(matches!(error, RiskServiceError::SystemError(_)));
    }

    #[test]
    fn test_pre_check_result_passed() {
        let result = PreCheckResult {
            passed: true,
            frozen_amount: dec!(100),
            reject_reason: None,
            risk_level: RiskLevel::Low,
            checked_at: Utc::now(),
        };

        assert!(result.passed);
        assert!(result.reject_reason.is_none());
    }

    #[test]
    fn test_snapshot_default() {
        let snapshot = RiskSnapshot::default();
        assert!(snapshot.available);
        assert_eq!(snapshot.total_pre_checks, 0);
    }

    #[test]
    fn test_pre_check_request_fields() {
        let request = PreCheckRequest {
            order_id: "test_order".to_string(),
            symbol: "ETHUSDT".to_string(),
            strategy_id: "min_trend".to_string(),
            side: RiskSide::Short,
            quantity: dec!(0.5),
            order_value: dec!(2000),
            available_balance: dec!(50000),
            current_position_qty: dec!(-0.3),
            total_equity: dec!(52000),
        };

        assert_eq!(request.order_id, "test_order");
        assert_eq!(request.symbol, "ETHUSDT");
        assert_eq!(request.side, RiskSide::Short);
        assert_eq!(request.quantity, dec!(0.5));
    }

    #[test]
    fn test_re_check_request_fields() {
        let request = ReCheckRequest {
            order_id: "test_order".to_string(),
            fill_id: "test_fill".to_string(),
            symbol: "BTCUSDT".to_string(),
            strategy_id: "day_trend".to_string(),
            side: RiskSide::Long,
            filled_qty: dec!(0.01),
            fill_price: dec!(45000),
            fill_value: dec!(450),
            fill_time: Utc::now(),
            current_position_qty: dec!(0.01),
            available_balance: dec!(9500),
            total_equity: dec!(10000),
        };

        assert_eq!(request.order_id, "test_order");
        assert_eq!(request.fill_id, "test_fill");
        assert_eq!(request.side, RiskSide::Long);
        assert_eq!(request.fill_value, dec!(450));
    }

    #[test]
    fn test_risk_side_equality() {
        assert_eq!(RiskSide::Long, RiskSide::Long);
        assert_eq!(RiskSide::Short, RiskSide::Short);
        assert_ne!(RiskSide::Long, RiskSide::Short);
    }

    #[test]
    fn test_risk_level_all_variants() {
        assert_eq!(RiskLevel::Low, RiskLevel::Low);
        assert_eq!(RiskLevel::Medium, RiskLevel::Medium);
        assert_eq!(RiskLevel::High, RiskLevel::High);
        assert_eq!(RiskLevel::Rejected, RiskLevel::Rejected);
    }

    #[test]
    fn test_risk_service_error_variants() {
        let errors = vec![
            RiskServiceError::Rejected("test".to_string()),
            RiskServiceError::Unavailable("unavailable".to_string()),
            RiskServiceError::InvalidArgument("invalid".to_string()),
            RiskServiceError::SystemError("system".to_string()),
        ];

        assert!(matches!(errors[0], RiskServiceError::Rejected(_)));
        assert!(matches!(errors[1], RiskServiceError::Unavailable(_)));
        assert!(matches!(errors[2], RiskServiceError::InvalidArgument(_)));
        assert!(matches!(errors[3], RiskServiceError::SystemError(_)));
    }

    #[test]
    fn test_pre_check_result_rejected() {
        let result = PreCheckResult {
            passed: false,
            frozen_amount: dec!(0),
            reject_reason: Some("insufficient margin".to_string()),
            risk_level: RiskLevel::Rejected,
            checked_at: Utc::now(),
        };

        assert!(!result.passed);
        assert_eq!(result.risk_level, RiskLevel::Rejected);
        assert!(result.reject_reason.is_some());
        assert_eq!(result.reject_reason.unwrap(), "insufficient margin");
    }

    #[test]
    fn test_re_check_result_with_warnings() {
        let result = ReCheckResult {
            passed: true,
            warnings: vec![
                "position approaching limit".to_string(),
                "high volatility detected".to_string(),
            ],
            alert_flagged: true,
            checked_at: Utc::now(),
        };

        assert!(result.passed);
        assert!(result.alert_flagged);
        assert_eq!(result.warnings.len(), 2);
    }

    #[test]
    fn test_re_check_result_clean() {
        let result = ReCheckResult {
            passed: true,
            warnings: vec![],
            alert_flagged: false,
            checked_at: Utc::now(),
        };

        assert!(result.passed);
        assert!(!result.alert_flagged);
        assert!(result.warnings.is_empty());
    }

    #[tokio::test]
    async fn test_mock_risk_service_freeze_unfreeze() {
        let service = MockRiskService::new();

        // freeze
        let freeze_result = service.freeze("order_1", dec!(100)).await;
        assert!(freeze_result.is_ok());

        // unfreeze
        let unfreeze_result = service.unfreeze("order_1").await;
        assert!(unfreeze_result.is_ok());
        assert_eq!(unfreeze_result.unwrap(), dec!(0)); // Mock returns 0
    }

    #[tokio::test]
    async fn test_mock_risk_service_confirm() {
        let service = MockRiskService::new();

        let result = service.confirm("order_1", dec!(100)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_mock_risk_service_snapshot() {
        let service = MockRiskService::new();

        let snapshot = service.snapshot().await.unwrap();
        assert!(snapshot.available);
        // MockRiskService 使用默认 snapshot，默认 service_name 为 "RiskService"
        assert_eq!(snapshot.service_name, "RiskService");
    }

    #[tokio::test]
    async fn test_mock_risk_service_reset_stats() {
        let service = MockRiskService::new();

        let result = service.reset_stats().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_mock_risk_service_name() {
        let service = MockRiskService::new();
        assert_eq!(service.name(), "MockRiskService");
    }

    #[test]
    fn test_risk_service_adapter() {
        let inner = Arc::new(MockRiskService::new());
        let adapter = RiskServiceAdapter::new(inner.clone());

        assert_eq!(adapter.name(), "MockRiskService");
    }

    #[test]
    fn test_risk_snapshot_fields() {
        let snapshot = RiskSnapshot {
            service_name: "TestService".to_string(),
            available: true,
            pre_check_pass_rate: 0.95,
            re_check_reject_rate: 0.02,
            total_pre_checks: 100,
            total_re_checks: 50,
            last_check_at: Some(Utc::now()),
        };

        assert_eq!(snapshot.service_name, "TestService");
        assert!(snapshot.available);
        assert_eq!(snapshot.total_pre_checks, 100);
        assert_eq!(snapshot.total_re_checks, 50);
    }
}
