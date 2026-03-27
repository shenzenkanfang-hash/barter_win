# barter-rs 量化交易系统

> 版本: 0.1.0
> 最后更新: 2026-03-27

---

## 当前状态

- 沙盒测试: 可用（见 `docs/sandbox.md`）
- 生产交易: 开发中
- 支持交易所: 仅沙盒模拟（Binance）

---

## 项目结构

```
crates/
├── a_common/     # 基础设施层: API/WS 网关
├── b_data_source/ # 业务数据层: DataFeeder, K线合成, 存储
├── c_data_process/ # 数据处理层: 指标计算, 信号生成
├── d_checktable/ # 检查层: 15分钟/日线检查
├── e_risk_monitor/ # 风控层: 风控, 持仓管理
├── f_engine/     # 引擎层: 订单执行, 策略调度
├── g_test/       # 测试层: 集成测试
├── h_sandbox/    # 沙盒层: 历史回放, 策略验证
└── x_data/       # 数据定义: 类型, trait
```

---

## 快速开始

### 运行沙盒测试

```bash
cargo run --bin full_production_sandbox -- \
  --symbol HOTUSDT \
  --start 2025-10-09T00:00:00Z \
  --end 2025-10-11T23:59:59Z
```

### 编译验证

```bash
cargo check --all
```

---

## 架构概览

详见 `docs/architecture.md`

---

## 技术栈

| 组件 | 技术 |
|------|------|
| Runtime | Tokio |
| 同步 | parking_lot::RwLock |
| 数值 | rust_decimal |
| 时间 | chrono |
| 错误 | thiserror |
| 数据库 | rusqlite 0.32 |

---

## 限制

- 仅支持单品种测试
- 沙盒为即时成交，无盘口深度模拟
- 内存存储，重启后数据丢失

---

## 相关文档

- `docs/architecture.md` - 系统架构
- `docs/sandbox.md` - 沙盒使用指南
- `CLAUDE.md` - AI 助手行为规则