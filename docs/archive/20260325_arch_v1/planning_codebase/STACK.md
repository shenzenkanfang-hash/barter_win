================================================================================
技术栈文档 - barter-rs 量化交易系统
================================================================================

项目: barter-rs Quantitative Trading System
版本: 0.1.0
更新: 2026-03-25

================================================================================
一、编程语言与运行时
================================================================================

语言: Rust
  - Edition 2021 (主要)
  - Edition 2024 (部分新模块)

编译器配置:
  - rustc.exe: C:\Users\char\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin\rustc.exe
  - cargo.exe: C:\Users\char\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin\cargo.exe
  - 构建环境变量: RUSTC=C:\Users\char\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin\rustc.exe

================================================================================
二、核心依赖
================================================================================

1. 异步运行时
   - tokio = "1.x"
     * features: ["full"]
     * 用于异步 IO、多线程任务调度
     * WebSocket 连接、HTTP 请求、定时器

2. 同步原语
   - parking_lot = "0.12"
     * RwLock, Mutex 比 std 更高效
     * 用于低频路径（账户、持仓、资金池）

3. 数值计算
   - rust_decimal = "1.36"
     * features: ["maths"]
     * 金融计算避免浮点精度问题
   - rust_decimal_macros = "1.36"
     * dec!() 宏创建 Decimal 常量

4. 时间处理
   - chrono = "0.4"
     * features: ["serde"]
     * DateTime<Utc> 时间戳处理

5. 错误处理
   - thiserror = "2.0"
     * 清晰的错误类型层次
     * #[derive(Error)] 派生宏

6. 日志
   - tracing = "0.1"
     * 结构化日志 info!/warn!/error!
   - tracing-subscriber = "0.3"
     * 日志格式化和输出

7. 序列化
   - serde = "1.0"
     * features: ["derive"]
   - serde_json = "1.0"
     * JSON 序列化/反序列化

================================================================================
三、网络与通信
================================================================================

1. HTTP 客户端
   - reqwest = "0.12"
     * features: ["json", "blocking"]
     * Binance REST API 调用
     * 交易规则拉取、账户查询

2. WebSocket
   - tokio-tungstenite = "0.26"
     * features: ["native-tls"]
     * Binance WebSocket 连接
   - native-tls = "0.2"
     * TLS 加密支持
   - futures-util = "0.3"
     * StreamExt, SinkExt trait

================================================================================
四、数据存储
================================================================================

1. SQLite (bundled)
   - rusqlite = "0.32"
     * features: ["bundled"]
     * 交易事件持久化
     * 表: account_snapshots, exchange_positions, local_positions,
          channel_events, risk_events, indicator_events, orders, sync_log

2. 内存备份
   - 自定义 MemoryBackup 模块
   - 高速内存盘 (tmpfs)
   - 定期同步到磁盘

3. CSV 输出
   - 内置 IndicatorCsvWriter
   - 指标对比数据导出

================================================================================
五、其他依赖
================================================================================

1. 状态管理
   - fnv = "1.0"
     * FnvHashMap O(1) 查找

2. 异步 trait
   - async-trait = "0.1"
     * 异步方法 trait 定义

3. 命令行解析
   - clap = "4.4"
     * features: ["derive"]
     * 命令行参数解析

4. 测试
   - tempfile = "3.10"
     * 临时文件和目录用于测试

5. Parquet (h_sandbox)
   - parquet = "56"
     * features: ["snap"]
     * 实验性数据回放功能

================================================================================
六、工作区结构
================================================================================

[workspace]
members = [
    "crates/a_common",       # 基础设施层
    "crates/b_data_source",  # 数据源层
    "crates/c_data_process", # 数据处理层
    "crates/d_checktable",   # 检查层
    "crates/e_risk_monitor",  # 风控层
    "crates/f_engine",       # 引擎层
    "crates/g_test",         # 测试层
    "crates/h_sandbox",      # 沙盒层
]
resolver = "3"

================================================================================
七、平台路径配置
================================================================================

Platform::detect() 自动选择:

Windows:
  - memory_backup_dir: E:/shm/backup
  - disk_sync_dir: E:/backup/sync
  - sqlite_db_path: E:/backup/trading_events.db
  - csv_output_path: E:/backup/output/indicator_comparison.csv
  - symbols_rules_dir: E:/shm/backup/symbols_rules

Linux:
  - memory_backup_dir: /dev/shm/backup
  - disk_sync_dir: data/backup
  - sqlite_db_path: data/trading_events.db
  - csv_output_path: output/indicator_comparison.csv
  - symbols_rules_dir: /dev/shm/backup/symbols_rules

================================================================================
八、模块架构
================================================================================

crates/
├── a_common/           # 工具层: API/WS网关、配置、错误类型
├── b_data_source/     # 数据层: DataFeeder、K线合成、Tick
├── c_data_process/     # 信号生成层: 指标计算、信号生成
├── d_checktable/       # 检查层: CheckTable汇总（异步并发）
├── e_risk_monitor/     # 合规约束层: 风控检查、仓位管理
├── f_engine/           # 引擎运行时层: 核心执行
├── g_test/             # 测试层: 集成测试
└── h_sandbox/          # 沙盒层: 实验性代码

================================================================================
