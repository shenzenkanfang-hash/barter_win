================================================================================
语言与运行时
================================================================================

Rust
----
版本: 2021 Edition
说明: 核心开发语言，整个交易系统使用 Rust 实现

Tokio
-----
版本: 1.x (workspace dependency)
特征: full
说明: 异步运行时，用于所有异步 IO 操作和任务调度
位置:
  - 主程序 (Cargo.toml)
  - a_common (workspace)
  - b_data_source (workspace, features: full)
  - c_data_process (workspace)
  - d_checktable (workspace)
  - e_risk_monitor (workspace)
  - f_engine (workspace)
  - x_data (dev-dependencies)

================================================================================
核心框架与库
================================================================================

异步编程
--------
async-trait    版本 0.1    - 异步 trait 支持，用于定义异步接口
futures        版本 0.3    - 异步原语和工具库
futures-util   版本 0.3    - futures 的工具函数

同步与并发
----------
parking_lot    版本 0.12   - 比 std RwLock 更高效的同步原语
                位置: 所有 crate (workspace)

数值计算
--------
rust_decimal   版本 1.36   - 金融计算避免浮点精度问题
rust_decimal_macros 版本 1.36 - Decimal 字面量宏
                位置: a_common, b_data_source, c_data_process, d_checktable, e_risk_monitor, f_engine (workspace)

时间处理
--------
chrono         版本 0.4    - DateTime<Utc> 时间处理
特征: serde
                位置: a_common, b_data_source, c_data_process, d_checktable, e_risk_monitor, f_engine, x_data (workspace)

错误处理
--------
thiserror      版本 2.0    - 清晰的错误类型层次
                位置: a_common, b_data_source, c_data_process, d_checktable, e_risk_monitor, f_engine, x_data (workspace)

序列化
-------
serde          版本 1.0    - Serialize/Deserialize
特征: derive
serde_json     版本 1.0    - JSON 序列化
                位置: 多个 crate (workspace)

日志追踪
--------
tracing        版本 0.1    - 结构化日志
tracing-subscriber 版本 0.3 - tracing 的日志格式化和订阅器
                位置: 主程序, f_engine (workspace)

================================================================================
数据存储
================================================================================

SQLite
------
rusqlite       版本 0.32
特征: bundled - 使用绑定的 SQLite 库
位置:
  - c_data_process (workspace)
  - d_checktable (version 0.32, features: bundled)
  - e_risk_monitor (workspace)

r2d2           版本 0.8    - 连接池管理器
r2d2_sqlite    版本 0.25   - SQLite 连接池实现
位置: d_checktable

持久化表:
  - account_snapshots     账户快照
  - exchange_positions   交易所持仓
  - local_positions      本地仓位记录
  - channel_events       通道切换事件
  - risk_events          风控事件
  - indicator_events     指标事件
  - orders               订单记录
  - sync_log             同步日志

================================================================================
网络与通信
================================================================================

HTTP 客户端
-----------
reqwest        版本 0.12
特征: json, blocking, native-tls
位置:
  - a_common (workspace)
  - b_data_source (version 0.12, features: json)
  - e_risk_monitor (workspace)
  - 主程序 (features: json, blocking, native-tls)

WebSocket
---------
tokio-tungstenite 版本 0.24/0.26
特征: native-tls
位置:
  - a_common (version 0.26)
  - b_data_source (version 0.26)
  - 主程序 (version 0.24)

native-tls     版本 0.2    - TLS 支持

================================================================================
缓存与恢复
================================================================================

Redis
-----
redis          版本 0.27
特征: tokio-comp, connection-manager
位置: b_data_source
说明: 仅用于灾备恢复用途，存储:
  - K线数据快照
  - 指标快照
  - 高波动窗口标记
  - 最后 checkpoint 时间戳

================================================================================
其他工具库
================================================================================

fnv            版本 1.0    - FnvHashMap，O(1) 查找
               位置: 多个 crate (workspace)

once_cell     版本 1.19   - 一次性初始化单元
               位置: b_data_source

rand          版本 0.8    - 随机数生成
               位置: b_data_source (GaussianNoise 实现)

dashmap       版本 5.5    - 并发 HashMap
               位置: 主程序

anyhow        版本 1.0    - 简化的错误处理
               位置: 主程序

tempfile      版本 3.10   - 临时文件/目录创建
               位置: d_checktable, e_risk_monitor (workspace)

csv           版本 1.1    - CSV 文件读写
               位置: 主程序 (workspace)

clap          版本 4.4    - 命令行参数解析
特征: derive
               位置: 主程序

================================================================================
项目结构
================================================================================

Workspace 成员:
  - a_common      公共类型、错误、配置、备份
  - b_data_source WebSocket、K线Store、mock_ws、mock_api
  - c_data_process 数据处理、信号类型 (TradingDecision/TradingAction)
  - d_checktable  15分钟 Trader/Executor/Repository
  - e_risk_monitor 风控监控
  - f_engine      事件驱动引擎 (内建 EMA/RSI 策略)
  - g_test        测试
  - x_data        数据相关类型

入口:
  - 主 bin: trading-system (src/main.rs)

================================================================================
配置管理
================================================================================

环境变量:
  - HTTP_PROXY / http_proxy - HTTP 代理支持 (binance_api.rs)
  - RUST_LOG - 日志级别
  - RUST_BACKTRACE - 崩溃回溯

代理配置:
  - reqwest 支持通过环境变量配置 HTTP 代理
  - WebSocket 连接支持重连 (指数退避: 5s -> 10s -> 20s -> ... -> 120s 最大)
