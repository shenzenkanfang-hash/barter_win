================================================================================
TESTING.md - Rust 量化交易系统测试规范
================================================================================

Author: 代码分析
Created: 2026-03-28
Status: 已确认
================================================================================


一、测试模块架构
================================================================================

1. g_test crate - 集中测试模块

   crates/g_test/src/
   ├── lib.rs           # 模块入口
   ├── b_data_source/   # b_data_source 相关测试
   │   ├── trader_pool_test.rs
   │   ├── replay_source_test.rs
   │   └── ...
   └── strategy/        # 策略层黑盒测试
       ├── strategy_executor_test.rs
       ├── trading_integration_test.rs
       └── mock_gateway.rs


二、测试文件组织
================================================================================

1. 集成测试 (g_test crate)

   特点:
   - 黑盒测试，通过公共 API 测试功能
   - 集中管理，所有 crate 的测试在一处
   - 模拟真实交易流程

2. 命名规范

   - 测试文件: *_test.rs
   - 测试模块: #[cfg(test)] mod tests
   - 测试函数: test_功能_场景()

3. 测试分组结构

   // ========================================================================
   // 分组标题
   // ========================================================================

   #[test]
   fn test_xxx() {
       // 测试代码
   }


三、测试辅助工具
================================================================================

1. Mock 实现

   a) MockExchangeGateway (b_data_source/src/api/mock_api/)

      pub struct MockExchangeGateway {
          balance: RwLock<Decimal>,
          positions: RwLock<HashMap<String, Position>>,
          orders: RwLock<Vec<OrderResult>>,
          reject: RwLock<Option<String>>,
      }

      impl MockExchangeGateway {
          pub fn new(initial_balance: Decimal) -> Self
          pub fn default_test() -> Self  // 默认测试实例
          pub fn set_reject(&self, msg: Option<String>)
      }

   b) MockMarketConnector (a_common/src/ws/)

      提供模拟市场数据流

   c) StreamTickGenerator (b_data_source/src/ws/mock_ws/)

      K线生成Tick流 (Iterator模式)

2. Test Strategy Trait Implementation

   struct TestStrategy { ... }

   impl Strategy for TestStrategy {
       fn id(&self) -> &str { &self.id }
       fn on_bar(&self, _bar: &StrategyKLine) -> Option<TradingSignal> {
           self.signals_to_return.read().first().cloned()
       }
   }

3. Builder 模式测试数据

   创建复杂测试数据:

   fn create_pinbar_long_entry_input() -> MinSignalInput {
       MinSignalInput {
           tr_base_60min: dec!(0.20),
           zscore_14_1m: dec!(2.5),
           pine_bg_color: "纯绿".to_string(),
           // ...
       }
   }


四、测试用例编写规范
================================================================================

1. 基本结构

   #[test]
   fn test_功能_预期结果() {
       // Arrange - 准备测试数据
       let pool = TraderPool::new();

       // Act - 执行被测操作
       pool.register(SymbolMeta::new("BTCUSDT".to_string()));

       // Assert - 验证结果
       assert!(pool.is_trading("BTCUSDT"));
   }

2. 断言风格

   assert!(condition, "失败信息");
   assert_eq!(actual, expected, "相等性检查");
   assert!(result.is_ok(), "Result 检查");
   assert!(result.is_err(), "Error 检查");

3. 测试用例命名

   test_trader_pool_register_and_unregister
   test_signal_generator_long_entry
   test_end_to_end_signal_to_decision

4. 边界条件测试

   #[test]
   fn test_boundary_tr_ratio_threshold() {
       // 边界: tr_base_60min = 0.15 (刚好等于阈值)
       let input = create_test_signal_input(dec!(0.15), ...);
       let output = generator.generate(&input, &VolatilityTier::High);
       assert!(!output.long_entry);
   }


五、测试覆盖范围
================================================================================

1. TraderPool 测试 (trader_pool_test.rs)

   覆盖:
   - 注册/注销
   - 重复注册
   - 状态更新
   - 批量操作
   - 清理功能

   #[test]
   fn test_trader_pool_register_and_unregister()
   #[test]
   fn test_trader_pool_duplicate_register()
   #[test]
   fn test_trader_pool_update_status()
   #[test]
   fn test_trader_pool_register_batch()
   #[test]
   fn test_trader_pool_clear()

2. StrategyExecutor 测试 (strategy_executor_test.rs)

   覆盖:
   - 策略注册与计数
   - 信号分发
   - 多策略调度
   - 禁用策略处理
   - 信号聚合

   #[test]
   fn test_executor_register_and_count()
   #[test]
   fn test_executor_dispatch_to_multiple_strategies()
   #[test]
   fn test_executor_dispatch_disabled_strategy()
   #[test]
   fn test_signal_aggregator_same_direction_max_qty()

3. TradingSignal 测试

   覆盖:
   - Builder 模式
   - 有效性验证
   - 方向判断
   - 枚举默认值

   #[test]
   fn test_trading_signal_builder_pattern()
   #[test]
   fn test_trading_signal_is_valid()
   #[test]
   fn test_direction_default()

4. 集成测试 (trading_integration_test.rs)

   覆盖完整流程:
   - 数据流: Tick -> K线合成 -> 指标计算
   - 信号生成: MinSignalGenerator
   - 风控检查: RiskPreChecker
   - 引擎执行: TradingEngineV2

   #[test]
   fn test_end_to_end_signal_to_decision()
   #[test]
   fn test_end_to_end_short_entry_flow()
   #[test]
   fn test_rejected_order_handling()


六、Mock 组件使用
================================================================================

1. MockExchangeGateway 使用

   let gateway = Arc::new(MockExchangeGateway::default_test());

   // 测试订单执行
   let result = gateway.place_order(OrderRequest { ... });
   assert!(result.is_ok());

   // 测试持仓跟踪
   let position = gateway.get_position("BTCUSDT").unwrap().unwrap();
   assert_eq!(position.long_qty, dec!(0.1));

   // 测试拒绝订单
   gateway.set_reject(Some("Rate limit".to_string()));
   let result = gateway.place_order(...);
   assert_eq!(result.unwrap().status, OrderStatus::Rejected);

2. MinSignalGenerator 使用

   let generator = MinSignalGenerator::new();
   let input = create_pinbar_long_entry_input();
   let output = generator.generate(&input, &VolatilityTier::High);

   assert!(output.long_entry);
   assert!(!output.short_entry);

3. RiskPreChecker 使用

   let checker = RiskPreChecker::new(dec!(0.95), dec!(1000));
   let result = checker.pre_check("BTCUSDT", available, order_value, equity);

   assert!(result.is_ok());  // 或
   assert!(result.is_err()); // 检查错误


七、测试配置
================================================================================

1. 依赖

   [dev-dependencies] (各 crate Cargo.toml)

   # g_test/Cargo.toml
   parking_lot = "0.12"
   rust_decimal_macros = "1.36"

2. 测试宏

   #![forbid(unsafe_code)]  // 保持与生产代码一致

   use rust_decimal_macros::dec;  // 便捷Decimal字面量


八、测试最佳实践
================================================================================

1. 独立性

   每个测试独立运行，不依赖其他测试的状态

2. 可重复性

   测试结果确定，不受执行顺序影响

3. 清晰断言

   包含有意义的断言消息

   assert_eq!(pool.count(), 1, "注册后策略数量应为 1");

4. 覆盖边界

   - 零值、空集合
   - 阈值边界
   - 相反方向
   - 错误路径

5. 中文注释

   测试用例使用中文注释说明测试目的


================================================================================
END OF TESTING.md
================================================================================
