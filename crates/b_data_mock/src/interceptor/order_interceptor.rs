//! 订单拦截器
//!
//! 包装 MockApiGateway，添加订单执行的延迟监控和心跳报到

use std::sync::Arc;
use parking_lot::RwLock;
use rust_decimal::Decimal;
use a_common::exchange::OrderResult;
use a_common::EngineError;
use a_common::models::types::Side;

use crate::api::mock_gateway::MockApiGateway;

/// 订单执行记录
#[derive(Debug, Clone)]
pub struct OrderExecutionRecord {
    pub symbol: String,
    pub side: Side,
    pub qty: Decimal,
    pub price: Decimal,
    /// 从下单到完成的延迟（毫秒）
    pub latency_ms: i64,
    /// 是否成功
    pub success: bool,
}

/// 订单拦截器配置
#[derive(Debug, Clone)]
pub struct OrderInterceptorConfig {
    /// 是否启用心跳报到
    pub enable_heartbeat: bool,
    /// 延迟警告阈值（毫秒）
    pub latency_warning_ms: i64,
    /// 延迟严重阈值（毫秒）
    pub latency_critical_ms: i64,
}

impl Default for OrderInterceptorConfig {
    fn default() -> Self {
        Self {
            enable_heartbeat: true,
            latency_warning_ms: 100,
            latency_critical_ms: 500,
        }
    }
}

/// 订单拦截器
///
/// 包装 MockApiGateway，添加执行延迟监控
pub struct OrderInterceptor {
    gateway: Arc<RwLock<MockApiGateway>>,
    config: OrderInterceptorConfig,
    /// 累计执行记录
    execution_records: Arc<RwLock<Vec<OrderExecutionRecord>>>,
}

impl OrderInterceptor {
    /// 创建订单拦截器
    pub fn new(gateway: MockApiGateway, config: OrderInterceptorConfig) -> Self {
        Self {
            gateway: Arc::new(RwLock::new(gateway)),
            config,
            execution_records: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// 使用默认配置创建
    pub fn with_default_config(gateway: MockApiGateway) -> Self {
        Self::new(gateway, OrderInterceptorConfig::default())
    }

    /// 下单（带延迟监控）
    pub fn place_order(
        &self,
        symbol: &str,
        side: Side,
        qty: Decimal,
        price: Option<Decimal>,
    ) -> Result<OrderResult, EngineError> {
        let start = std::time::Instant::now();

        let result = self.gateway.read().place_order(symbol, side.clone(), qty, price);

        let latency_ms = start.elapsed().as_millis() as i64;

        // 记录执行
        let record = OrderExecutionRecord {
            symbol: symbol.to_string(),
            side,
            qty,
            price: result.as_ref().map(|r| r.filled_price).unwrap_or(Decimal::ZERO),
            latency_ms,
            success: result.is_ok(),
        };
        self.execution_records.write().push(record);

        // 延迟警告
        if latency_ms > self.config.latency_critical_ms {
            tracing::warn!(
                "[OrderInterceptor] 订单执行延迟严重: {}ms (symbol: {})",
                latency_ms, symbol
            );
        } else if latency_ms > self.config.latency_warning_ms {
            tracing::warn!(
                "[OrderInterceptor] 订单执行延迟警告: {}ms (symbol: {})",
                latency_ms, symbol
            );
        }

        result
    }

    /// 获取执行记录统计
    pub fn get_stats(&self) -> OrderStats {
        let records = self.execution_records.read();
        let total = records.len();
        let success = records.iter().filter(|r| r.success).count();
        let failed = total - success;

        let avg_latency = if total > 0 {
            records.iter().map(|r| r.latency_ms).sum::<i64>() / total as i64
        } else {
            0
        };

        let max_latency = records.iter().map(|r| r.latency_ms).max().unwrap_or(0);

        OrderStats {
            total_orders: total,
            successful_orders: success,
            failed_orders: failed,
            avg_latency_ms: avg_latency,
            max_latency_ms: max_latency,
        }
    }

    /// 获取原始网关引用（用于不需要拦截的场景）
    pub fn get_gateway(&self) -> Arc<RwLock<MockApiGateway>> {
        Arc::clone(&self.gateway)
    }
}

impl Clone for OrderInterceptor {
    fn clone(&self) -> Self {
        Self {
            gateway: Arc::clone(&self.gateway),
            config: self.config.clone(),
            execution_records: Arc::clone(&self.execution_records),
        }
    }
}

/// 订单统计
#[derive(Debug, Clone)]
pub struct OrderStats {
    pub total_orders: usize,
    pub successful_orders: usize,
    pub failed_orders: usize,
    pub avg_latency_ms: i64,
    pub max_latency_ms: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_order_stats() {
        let gateway = MockApiGateway::with_default_config(dec!(10000));
        let interceptor = OrderInterceptor::with_default_config(gateway);

        // 获取初始统计
        let stats = interceptor.get_stats();
        assert_eq!(stats.total_orders, 0);
    }
}
