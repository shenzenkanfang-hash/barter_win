# 外部集成

**分析日期：** 2026-03-20

## API 和外部服务

**加密货币交易所 (通过 WebSocket/REST)：**
- Binance 现货 - `barter-data/src/exchange/binance/spot/`
  - 市场数据：订单簿 (L1/L2)、成交、K线
  - 使用 `reqwest` 处理 HTTP，`tokio-tungstenite` 处理 WebSocket

- Binance 期货 - `barter-data/src/exchange/binance/futures/`
  - L2 订单簿、强平数据

- Bybit - `barter-data/src/exchange/bybit/`
  - 现货和期货市场
  - 订单簿 (L1/L2)、成交

- OKX - `barter-data/src/exchange/okx/`
  - 市场数据流

- Kraken - `barter-data/src/exchange/kraken/`
  - 订单簿 (L1)、成交数据

- Coinbase - `barter-data/src/exchange/coinbase/`
  - 通过 WebSocket 获取市场数据

- Gate.io - `barter-data/src/exchange/gateio/`
  - 现货、永续、期权、期货

- BitMEX - `barter-data/src/exchange/bitmex/`
  - 成交和订单簿数据

- Bitfinex - `barter-data/src/exchange/bitfinex/`
  - 成交数据

**协议支持：**
- 通过 `tokio-tungstenite` 连接交易所 WebSocket
- 通过 `reqwest` 调用 REST/HTTP API
- 认证端点使用 HMAC-SHA256 签名 (Binance 等)
  - `hmac`、`sha2`、`hex`、`base64` crates

## 数据存储

**数据库：**
- 未直接使用
- 内存数据结构存储订单簿、持仓、余额
- 基于文件的配置 (JSON)

**缓存：**
- 内存 `parking_lot` RwLock 用于共享状态
- `indexmap` 用于保持索引的哈希映射
- 无外部缓存 (Redis、Memcached)

## 认证与身份

**交易所认证：**
- API 请求的 HMAC-SHA256 签名生成
  - 实现于 `barter-integration/src/protocol/http/private/encoder.rs`
- API 密钥通过用户配置存储/加载 (不在代码中)
- 无 OAuth 或第三方认证提供商

## 监控与可观测性

**日志：**
- `tracing` 配合 `tracing-subscriber`
- 通过 `tracing-subscriber` 的 `env-filter`、`json`、`registry` 特性输出 JSON 格式
- 无外部日志聚合服务

**错误跟踪：**
- 通过 `thiserror` 自定义错误类型
- 无外部错误跟踪服务 (Sentry 等)

## CI/CD 与部署

**托管：**
- GitHub Actions 用于 CI
- GitHub 仓库：`barter-rs/barter-rs`

**CI 流水线 (`.github/workflows/ci.yml`)：**
- 推送到 develop 分支时执行 cargo check
- 推送到 develop 分支时执行 cargo test
- cargo fmt 检查
- cargo clippy 代码检查 `-D warnings`

**发布流水线 (`.github/workflows/release-plz.yml`)：**
- release-plz 用于自动化发布
- 在 main 分支推送时运行
- 通过 `CARGO_REGISTRY_TOKEN` 发布到 crates.io
- 创建变更日志 PR

**依赖更新：**
- 配置了 Dependabot (`.github/dependabot.yml`)

## 环境配置

**必需配置：**
- 系统设置的 JSON 配置文件
- 示例：`barter/examples/config/system_config.json`
- 交易所按名称指定 (例如 "binance_spot")
- JSON 中的初始余额、合约、执行设置

**无需环境变量：**
- 未检测到 `.env` 文件或环境变量使用
- 配置完全基于文件

## Webhook 和回调

**入站：**
- WebSocket 连接连接到交易所 (客户端发起)
- 无入站 webhook 端点

**出站：**
- 无 (这是客户端库，不是服务器)

---

*集成审计：2026-03-20*
