# Rust量化系统 Python风控+策略迁移方案

版本: 2026-03-28
状态: 待实施
负责人: 软件架构师

---

## 当前架构

```
┌─────────────────────────────────────────────────────┐
│ Rust完全自闭环                                        │
│                                                     │
│ 行情 → 指标计算 → 策略决策 → 风控检查 → 订单执行    │
│ (b_data)   (c_data)  (f_engine) (e_risk) (h_sandbox)│
└─────────────────────────────────────────────────────┘
```

## 目标架构

```
┌─────────────────────────────────────────────────────┐
│ Rust 主引擎 (不变)                                    │
│                                                     │
│ 行情 → 指标计算 ───────────────────────────────────┐
│ (b_data)  (c_data)                                 │
└─────────────────────────────────────────────────────┘
                      ↓
              Python 回调
                      ↓
┌─────────────────────────────────────────────────────┐
│ Python 层                                            │
│                                                     │
│ 策略决策 (strategy.py) ←→ 风控检查 (risk.py)        │
│        ↑                                            │
│        └──────── 持仓/资金状态共享 ────────────────┘
└─────────────────────────────────────────────────────┘
                      ↓
              Rust 执行层（订单/撮合）
```

---

## 迁移原则

| 保留Rust | 迁移Python |
|---------|-----------|
| 行情订阅、WebSocket | 策略逻辑 |
| K线合成、指标计算 | 风控规则 |
| 订单撮合、延迟模拟 | 持仓/资金管理 |
| 数据库读写 | 策略参数配置 |

---

## 迁移阶段

### 阶段1：架构改造（最小改动）

**目标**: Rust引擎支持Python回调，不改变现有逻辑

**新增文件**:
```
crates/
└── py_bridge/                 # 新增：Rust-Python桥接层
    ├── Cargo.toml
    └── src/
        ├── lib.rs            # PyO3入口
        ├── event.rs          # 事件定义
        ├── adapter.rs        # 数据转换
        └── executor.rs       # 执行器包装
```

**修改文件**:
```
f_engine/src/core/engine.rs     # 增加策略回调槽
h_sandbox/src/simulator/       # 增加Python风控回调
```

**验收标准**:
- [ ] Python策略能收到Tick/Bar数据
- [ ] Python返回的信号能触发Rust下单
- [ ] 原有Rust策略作为默认，Python策略可选

---

### 阶段2：策略Python化

**目标**: 所有策略逻辑迁移到Python，Rust只负责执行

**迁移内容**:
```
Python文件：
strategies/
├── __init__.py
├── base.py              # 策略基类
├── ema_cross.py         # EMA金叉策略
├── rsi_range.py         # RSI区间策略
├── macd_signal.py       # MACD信号策略
└── composite.py         # 组合策略
```

**验收标准**:
- [ ] 3种以上策略可切换
- [ ] 策略热加载（不改代码换策略）
- [ ] 历史回测结果与原Rust策略一致

---

### 阶段3：风控Python化

**目标**: 风控规则可配置、可热更新

**迁移内容**:
```
python/
└── risk/
    ├── __init__.py
    ├── base.py           # 风控基类
    ├── position_limit.py # 仓位限制
    ├── drawdown.py       # 回撤控制
    ├── exposure.py        # 敞口管理
    └── circuit_breaker.py # 熔断机制
```

**验收标准**:
- [ ] 风控参数可从配置文件加载
- [ ] 触发风控时记录日志
- [ ] 风控可热更新（不停盘改规则）

---

### 阶段4：数据整合

**目标**: Python层统一管理持仓/资金状态

```
Python数据模型：
┌─────────────┐     ┌─────────────┐
│  Account    │────→│  Position   │
│  账户资金   │     │   持仓     │
├─────────────┤     ├─────────────┤
│ balance     │     │ symbol      │
│ equity      │     │ qty         │
│ margin      │     │ entry_price │
│ available   │     │ unrealized   │
└─────────────┘     └─────────────┘
```

**验收标准**:
- [ ] Python侧实时同步Rust侧账户状态
- [ ] 策略可读取当前持仓/资金
- [ ] 回测结束后数据一致

---

## 文件结构

```
D:/Rust项目/barter-rs-main/
├── Cargo.toml                    # 加入 py_bridge crate
├── crates/
│   ├── py_bridge/               # 新增：Rust-Python桥接
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── bridge.rs       # 桥接核心
│   │       └── types.rs        # 数据类型
│   │
│   ├── c_data_process/          # 保留：K线+指标
│   ├── h_sandbox/               # 保留：撮合+模拟
│   └── ...
│
├── python/                       # 新增：Python层
│   ├── __init__.py
│   ├── engine.py               # Python引擎（整合策略+风控）
│   ├── config.py               # 配置加载
│   │
│   ├── strategies/             # 策略目录
│   │   ├── __init__.py
│   │   ├── base.py
│   │   └── ema_cross.py
│   │
│   └── risk/                  # 风控目录
│       ├── __init__.py
│       ├── base.py
│       └── position_limit.py
│
├── pyproject.toml              # Python依赖
└── build_bridge.py            # PyO3构建脚本
```

---

## Python依赖

```
# pyproject.toml
[project]
name = "barter-python"
version = "0.1.0"
requires-python = ">=3.10"

dependencies = [
    "rust_decimal>=1.36",
    "pandas>=2.0",
    "numpy>=1.24",
    "pyyaml>=6.0",
    "loguru>=0.7",
]

[project.optional-dependencies]
dev = [
    "pytest>=7.0",
    "pytest-asyncio>=0.21",
    "black>=23.0",
    "ruff>=0.1",
]

[build-system]
requires = ["maturin>=1.0"]
build-backend = "maturin"
```

---

## 实施顺序

```
阶段1 ─────────────────────────────────────────────────────→ 阶段4

1. 创建 py_bridge crate（Rust-Python桥接）
   ↓
2. 在 sandbox_pure.rs 中集成 Python 策略回调
   ↓
3. 编写 Python 策略基类和示例策略
   ↓
4. 将 e_risk_monitor 逻辑迁移到 Python risk/
   ↓
5. 创建 Python Account/Position 数据管理层
   ↓
6. 完善日志、配置、热加载
```

---

## 风险与注意事项

1. **GIL影响**: Python回调会阻塞Rust事件循环，策略函数必须快速返回
2. **数据类型转换**: Decimal/Float转换注意精度损失
3. **异常处理**: Python策略崩溃不能影响Rust引擎
4. **向后兼容**: 保留Rust策略作为fallback
