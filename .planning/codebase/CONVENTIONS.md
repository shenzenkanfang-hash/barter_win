================================================================================
CONVENTIONS.md - Rust 量化交易系统编码规范
================================================================================

Author: 代码分析
Created: 2026-03-28
Status: 已确认
================================================================================


一、基础配置规范
================================================================================

1. Rust 版本与 Edition
   - Edition: 2024
   - Workspace 管理多 crate

2. rustfmt.toml 规范
   - edition = "2024"
   - imports_granularity = "crate" (按 crate 聚合导入)

3. 全局安全规则
   - #![forbid(unsafe_code)] (强制禁止 unsafe 代码)
   - #![allow(dead_code)] (允许死代码警告抑制)


二、派生宏 (Derive) 顺序规范
================================================================================

标准类型派生顺序:
   #[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]

数据结构派生顺序:
   #[derive(Debug, Clone, Default, Serialize, Deserialize)]

枚举类型派生顺序:
   #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]

仅 Debug:
   #[derive(Debug, Clone, Copy)]

仅 Debug + Clone:
   #[derive(Debug, Clone)]


三、错误处理规范
================================================================================

1. 使用 thiserror 库定义错误类型

   #[derive(Debug, Clone, Eq, PartialEq, Error)]
   pub enum MyError {
       #[error("描述: {0}")]
       MyVariant(String),
   }

2. 错误类型层次结构

   a_common/src/claint/error.rs 定义三层错误体系:

   - EngineError: 引擎相关错误
     * RiskCheckFailed, OrderExecutionFailed, LockFailed
     * InsufficientFund, PositionLimitExceeded, ModeSwitchFailed
     * SymbolNotFound, Network, MemoryBackup

   - MarketError: 市场数据错误
     * WebSocketConnectionFailed, WebSocketError
     * SubscribeFailed, UnsubscribeFailed
     * ParseError, KLineError, OrderBookError
     * Timeout, RedisError, NetworkError

   - AppError: 统一应用错误 (整合 Engine + Market)
     * 使用 [Engine], [Market], [Data], [Infra], [Other] 前缀

3. From 实现用于错误转换

   impl From<EngineError> for AppError {
       fn from(e: EngineError) -> Self {
           match e {
               EngineError::Xxx(msg) => AppError::Xxx(msg),
           }
       }
   }


四、模块结构规范
================================================================================

1. 目录结构 (6层架构)

   crates/
   ├── a_common/      # 基础设施层: API/WS网关、配置、错误、数据模型
   ├── b_data_source/ # 业务数据层: 市场数据处理、K线合成、订单簿
   ├── c_data_process/# 数据处理层: 交易信号类型 (TradingDecision/TradingAction)
   ├── d_checktable/  # 检查表层: 15分钟Trader/Executor/Repository
   ├── e_risk_monitor/ # 风控监控层
   ├── f_engine/      # 引擎层: 事件驱动引擎
   ├── g_test/        # 测试模块 (黑盒测试)
   └── x_data/        # 数据相关模块

2. lib.rs 模块导出规范

   - 顶层 #![forbid(unsafe_code)]
   - 模块可见性: pub mod, pub use
   - Re-exports 集中管理 (类型、函数、错误)
   - 清晰的中文注释说明模块职责

3. 子模块组织

   每个 crate 按功能划分 submodules:
   - ws/       - WebSocket 数据接口
   - api/      - REST API 数据接口
   - models/   - 数据类型
   - 内部模块  - 私有实现


五、命名规范
================================================================================

1. 类型命名

   - 枚举成员: PascalCase (如 LongEntry, ShortEntry)
   - 结构体字段: snake_case
   - 泛型参数: PascalCase (如 T, R)

2. 函数命名

   - 查询型: get_xxx(), is_xxx(), has_xxx()
   - 动作型: update_xxx(), set_xxx(), process_xxx()
   - 创建型: new(), with_xxx() (builder模式)
   - 布尔型: is_xxx(), is_active(), is_enabled()

3. 常量与配置

   - 全局常量: SCREAMING_SNAKE_CASE (如 MAX_KLINE_ENTRIES)
   - 配置结构: PascalCase (如 VolatilityConfig)

4. 文件命名

   - 模块文件: snake_case (如 market_data.rs)
   - 模块目录: snake_case (如 market_data/)
   - 测试文件: *_test.rs 或 tests/*.rs


六、同步与并发规范
================================================================================

1. 使用 parking_lot 替代标准库锁

   parking_lot = "0.12"

   - RwLock: 读多写少场景
   - Mutex: 独占访问

2. 锁使用原则 (高频路径无锁)

   - Tick接收、指标更新、策略判断: 无锁
   - 下单和资金更新: 使用锁
   - 锁外预检所有风控条件


七、宏规范
================================================================================

1. 便捷宏命名

   #[macro_export]
   macro_rules! store_write_kline {
       ($symbol:expr, $kline:expr, $closed:expr) => {
           $crate::default_store().write_kline($symbol, $kline, $closed)
       };
   }

2. 宏命名: snake_case (如 store_write_kline)


八、代码风格规范
================================================================================

1. 导入顺序

   - 标准库导入
   - 第三方库导入
   - 当前 crate 导入
   - 按 crate 聚合 (imports_granularity = "crate")

2. 文档注释

   //! 用于 lib.rs 顶部模块文档
   /// 用于函数/结构体文档

3. 访问控制

   - pub: 公开 API
   - pub(crate): crate 内部
   - private: 默认

4. 类型别名

   使用 type 别名提高可读性:
   type Result<T> = std::result::Result<T, EngineError>;


九、Builder 模式规范
================================================================================

类型实现 with_xxx() 方法链式调用:

   #[derive(Debug, Clone)]
   pub struct Signal {
       pub price: Option<Decimal>,
       pub stop_loss: Option<Decimal>,
   }

   impl Signal {
       pub fn with_price(mut self, price: Decimal) -> Self {
           self.price = Some(price);
           self
       }
       pub fn with_stop_loss(mut self, sl: Decimal) -> Self {
           self.stop_loss = Some(sl);
           self
       }
   }

   // 使用:
   let signal = TradingSignal::new(...).with_price(dec!(50000));


十、代码组织原则
================================================================================

1. 高频路径无锁设计
   - Tick -> Store -> 策略 -> 决策 (无锁)
   - 锁仅用于下单和资金更新

2. 增量计算 O(1)
   - EMA, SMA, MACD 等指标增量计算
   - K线增量更新当前K线

3. 三层指标体系
   - TR (True Range): 波动率突破判断
   - Pine颜色: 趋势信号 (MACD + EMA10/20 + RSI)
   - 价格位置: 周期极值判断

4. 混合持仓模式
   - 资金池 RwLock 保护 (低频)
   - 策略持仓独立计算 (无锁)


================================================================================
END OF CONVENTIONS.md
================================================================================
