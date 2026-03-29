================================================================================
STACK.md - 技术栈文档
================================================================================

项目: barter-rs 量化交易系统
路径: D:\Rust项目\barter-rs-main
最后更新: 2026-03-29

================================================================================
1. RUST EDITION
================================================================================

主要Edition: 2024
- a_common: 2024
- b_data_source: 2024
- c_data_process: 2024
- e_risk_monitor: 2024
- f_engine: 2021 (legacy)

================================================================================
2. TOKIO RUNTIME
================================================================================

异步运行时: tokio 1.x

workspace.dependencies 配置:
  tokio = { version = "1", features = ["full"] }

主要特性:
- 多线程任务调度
- 异步 IO 操作
- WebSocket 连接管理
- HTTP 客户端

使用场景:
- WebSocket 连接 (tokio-tungstenite)
- HTTP 请求 (reqwest)
- 异步任务管理

================================================================================
3. PARKING_LOT
================================================================================

同步原语: parking_lot 0.12

用途:
- RwLock: 替代 std::sync::RwLock，更高效
- Mutex: 替代 std::sync::Mutex

使用场景:
- RateLimiter 限速器
- 账户池 (AccountPool)
- 持仓管理 (PositionManager)
- 共享状态

================================================================================
4. RUST_DECIMAL
================================================================================

数值计算: rust_decimal 1.36

workspace.dependencies 配置:
  rust_decimal = { version = "1.36", features = ["maths"] }
  rust_decimal_macros = "1.36"

用途:
- 金融计算避免浮点精度问题
- 价格精度处理
- 数量精度处理
- 手续费计算
- 盈亏计算

使用场景:
- SymbolRulesData.tick_size, min_qty, step_size
- 订单价格/数量
- 手续费率 (maker_fee, taker_fee)
- 未实现盈亏 (unrealized_pnl)

================================================================================
5. CHRONO
================================================================================

时间处理: chrono 0.4

workspace.dependencies 配置:
  chrono = { version = "0.4", features = ["serde"] }

用途:
- DateTime<Utc> 时间处理
- 时间戳转换
- 时间间隔计算

使用场景:
- K线时间戳 (kline_start_time, kline_close_time)
- 事件时间 (event_time, trade_time)
- 速率限制窗口 (window_start)

================================================================================
6. THISERROR
================================================================================

错误处理: thiserror 2.0

workspace.dependencies 配置:
  thiserror = "2.0"

用途:
- 清晰的错误类型层次
- #[derive(Error)] 派生宏
- #[error()] 格式化错误消息

使用场景:
- MarketError 市场数据错误
- EngineError 引擎错误

================================================================================
7. TRACING
================================================================================

日志框架: tracing 0.1

workspace.dependencies 配置:
  tracing = "0.1"
  tracing-subscriber = "0.3"

用途:
- 结构化日志
- 日志级别控制
- Span 追踪

使用场景:
- WebSocket 连接状态
- API 请求日志
- 错误和警告记录

================================================================================
8. SERDE
================================================================================

序列化/反序列化: serde 1.0

workspace.dependencies 配置:
  serde = { version = "1.0", features = ["derive"] }
  serde_json = "1.0"

用途:
- JSON 序列化/反序列化
- 自定义序列化格式

使用场景:
- Binance API 响应解析
- WebSocket 消息解析
- 配置文件

================================================================================
9. RUSQLITE
================================================================================

数据库: rusqlite 0.32 (bundled)

workspace.dependencies 配置:
  rusqlite = { version = "0.32", features = ["bundled"] }

用途:
- SQLite 数据库操作
- 交易事件持久化
- 历史数据存储

路径:
- Windows: E:/backup/trading_events.db
- Linux: data/trading_events.db

================================================================================
10. 其他依赖
================================================================================

reqwest 0.12:
  HTTP 客户端，用于 Binance REST API 调用
  features: ["json", "blocking", "native-tls"]

tokio-tungstenite 0.24-0.26:
  WebSocket 客户端，用于 Binance WebSocket 连接
  features: ["native-tls"]

futures-util 0.3:
  异步流处理工具

fnv 1.0:
  FnvHashMap，O(1) 查找

async-trait 0.1:
  异步 trait 支持

dashmap 5.5:
  并发 HashMap

================================================================================
11. WORKSPACE 依赖汇总
================================================================================

[workspace.dependencies]
parking_lot = "0.12"
rand = "0.8"
rust_decimal = { version = "1.36", features = ["maths"] }
rust_decimal_macros = "1.36"
thiserror = "2.0"
tracing = "0.1"
tracing-subscriber = "0.3"
chrono = { version = "0.4", features = ["serde"] }
tokio = { version = "1", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
async-trait = "0.1"
futures = "0.3"
fnv = "1.0"
rusqlite = { version = "0.32", features = ["bundled"] }
tempfile = "3.10"
reqwest = { version = "0.12", features = ["json", "blocking", "native-tls"] }
csv = "1.1"

================================================================================
12. 模块依赖层次
================================================================================

a_common (基础设施层)
  -> tokio, parking_lot, reqwest, chrono, serde, rust_decimal, thiserror

b_data_source (业务数据层)
  -> a_common, tokio, async-trait, rust_decimal, chrono

c_data_process (数据处理层)
  -> a_common, b_data_source, parking_lot, rusqlite

e_risk_monitor (风控层)
  -> a_common, b_data_source, x_data, rusqlite, reqwest

f_engine (引擎层)
  -> a_common, b_data_source, d_checktable, x_data

x_data (数据类型层)
  -> (无外部依赖，纯数据类型)

================================================================================
