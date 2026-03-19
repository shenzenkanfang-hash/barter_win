# 技术栈

**分析日期：** 2026-03-20

## 编程语言

**主要语言：**
- Rust 1.42+ (stable) - 所有 crates 的核心语言

## 运行时

**环境：**
- Native Rust 运行时
- Tokio 异步运行时用于异步操作

**包管理器：**
- Cargo (Rust 包管理器)
- Lockfile: `Cargo.lock` (gitignored, 不提交)

## 框架

**核心：**
- Barter 生态系统 - 由多个 crates 组成的模块化交易框架：
  - `barter` (0.12.4) - 带状态管理的交易引擎
  - `barter-data` (0.11.0) - 从交易所获取市场数据流
  - `barter-instrument` (0.3.1) - 交易所/合约/资产数据结构
  - `barter-execution` (0.7.0) - 订单执行和账户数据
  - `barter-integration` (0.10.0) - 低层 REST/WebSocket 框架
  - `barter-macro` (0.2.0) - 过程宏

**异步：**
- Tokio 1.42 - 支持多线程的异步运行时
- Tokio-stream 0.1.17 - 异步流工具
- Futures 0.3.31 - 异步抽象

**测试：**
- Tokio-test 0.4.4 - 异步测试工具
- Criterion 0.5.1 - 基准测试框架

**构建/开发：**
- release-plz (0.5) - 发布自动化和变更日志管理
- rustfmt - 代码格式化
- clippy - 代码检查

## 关键依赖

**协议/网络：**
- `reqwest` 0.12.9 - HTTP 客户端 (使用 rustls-tls)
- `tokio-tungstenite` 0.26.0 - WebSocket 客户端 (使用 rustls-tls)
- `url` 2.5.4 - URL 解析

**序列化：**
- `serde` 1.0.216 - 序列化框架
- `serde_json` 1.0.133 - JSON 序列化
- `serde_qs` 0.13.0 - 查询字符串序列化
- `serde_urlencoded` 0.7.1 - URL 编码序列化
- `prost` 0.12.4 - Protocol buffers

**数据结构：**
- `rust_decimal` 1.36.0 - 金融计算的十进制运算
- `smol_str` 0.3.2 - 短字符串优化
- `indexmap` 2.6.0 - 保持索引的映射
- `vecmap-rs` 0.2.2 - 基于向量的映射
- `parking_lot` 0.12.3 - 同步原语
- `fnv` 1.0.7 - FNV 哈希函数

**加密：**
- `hmac` 0.12.1 - 用于认证的 HMAC
- `sha2` 0.10.8 - SHA-2 哈希
- `hex` 0.4.3 - 十六进制编码
- `base64` 0.22.1 - Base64 编码

**日志：**
- `tracing` 0.1.41 - 结构化日志
- `tracing-subscriber` 0.3.19 - 带 JSON 输出的日志订阅器

**错误处理：**
- `thiserror` 2.0.8 - 错误派生宏

**时间：**
- `chrono` 0.4.39 - 日期/时间处理

**过程宏：**
- `proc-macro2` 1.0.49 - 宏基础设施
- `syn` 1.0.107 - Rust 语法解析
- `quote` 1.0.23 - Token 生成
- `convert_case` 0.6.0 - 大小写转换工具

## 配置

**环境：**
- JSON 配置文件 (例如 `system_config.json`, `backtest_config.json`)
- 不直接使用环境变量；配置基于文件

**构建：**
- `rust-toolchain.toml` - 指定带 clippy、rustfmt 的 stable 工具链
- `rustfmt.toml` - 代码格式化配置
- `release-plz.toml` - 发布自动化配置
- Workspace resolver version 3

## 平台要求

**开发：**
- Rust stable 工具链
- Cargo
- Git

**生产：**
- 带 Rust 运行时的 Linux/macOS/Windows
- 无额外平台依赖

---

*技术栈分析：2026-03-20*
