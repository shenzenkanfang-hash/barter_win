# v1.5: Python-Rust 功能对齐

**Goal**: 根据Python-Rust功能验证结果，实现核心缺失功能

**Start Date**: 2026-03-21

**Status**: IN PROGRESS

---

## 待实现功能

### 高优先级

| 序号 | 模块 | 功能 | 说明 |
|------|------|------|------|
| 1 | e_strategy | TrendStatusDetector | Pine颜色分组校验(12_26/20_50/100_200周期) |
| 2 | d_risk_monitor | LocalPositionManager统一接口 | +品种锁，统一接口调用 |
| 3 | d_risk_monitor | sync_account_data | 同步交易所账户状态 |
| 4 | d_risk_monitor | sync_position_data | 同步交易所持仓状态 |
| 5 | c_data_process | TR比率排名 | 20日百分比排名 |

### 中优先级

| 序号 | 模块 | 功能 | 说明 |
|------|------|------|------|
| 6 | d_risk_monitor | SymbolRuleParser同步 | 实时规则更新，状态追踪 |
| 7 | d_risk_monitor | LeverageCommissionCache | 杠杆档位缓存，1小时过期 |
| 8 | a_common | Binance手续费率API | 实时手续费率获取 |
| 9 | e_strategy | MarketDetector.validate_data | 数据有效性校验 |
| 10 | e_strategy | _validate_pine_color_groups | 多周期分组校验 |

---

## 已确认功能

| 功能 | 验证结果 | 说明 |
|------|----------|------|
| PinStatusDetector | ✅ 已实现 | 4条件版本，确认足够 |
| 指标数据共享 | ✅ 确认架构 | CheckTable内存共享 |
| OrderExecutor | ✅ 服务对象 | 持有gateway+risk_checker |
| Lua脚本 | ✅ RwLock替代 | 用Rust RwLock管理原子性 |

---

## 数据流转架构（已确认）

```
指标服务(子循环) → 写 CheckTable → (无锁，并行)
                              ↓
Strategy Processor → 读 CheckTable → (无锁，并行)
                              ↓
                         下单
                          ↓
                    全局锁(品种锁)
```

**关键点**：
- 指标写入 CheckTable：无锁，并行
- 策略读取 CheckTable：无锁，并行
- 下单时：加全局锁（品种锁）

---

## 验证文档

- `.planning/py-rust-audit/VERIFICATION_SUMMARY.csv` - 79条功能验证记录
- `.planning/py-rust-audit/VERIFICATION_REPORT.md` - 详细验证报告
- `.planning/py-rust-audit/COMPARISON_REPORT.md` - Python-Rust对比报告
