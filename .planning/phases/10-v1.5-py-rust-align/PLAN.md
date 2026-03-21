# v1.5: Python-Rust 功能对齐 - 执行计划

**Phase ID**: 10-v1.5-py-rust-align

---

## 执行顺序

### Phase 1: TrendStatusDetector

**目标**: 实现日线趋势检测器

**文件**: `crates/indicator/src/day/signal_generator.rs` 已存在，需补充完整逻辑

**任务**:
1. [ ] `_validate_pine_color_groups()` - 按组校验Pine颜色(12_26/20_50/100_200)
2. [ ] `check_long_entry()` - 做多开仓条件
3. [ ] `check_short_entry()` - 做空开仓条件
4. [ ] `check_long_exit()` - 做多平仓条件
5. [ ] `check_short_exit()` - 做空平仓条件
6. [ ] `check_long_hedge()` - 多单回落对冲
7. [ ] `check_short_hedge()` - 空单回升对冲

---

### Phase 2: LocalPositionManager 统一接口

**目标**: 统一仓位管理接口 + 品种锁

**文件**: `crates/engine/src/position/position_manager.rs`

**任务**:
1. [ ] 添加品种锁 `SymbolMutex` (parking_lot::RwLock)
2. [ ] 实现 `calculate_position_summary()` - 仓位汇总计算
3. [ ] 统一接口封装 - open/close/update/get

---

### Phase 3: 交易所状态同步

**目标**: sync_account_data / sync_position_data

**文件**: `crates/engine/src/` 新增同步模块

**任务**:
1. [ ] `sync_account_data()` - 从交易所同步账户状态
2. [ ] `sync_position_data()` - 从交易所同步持仓状态
3. [ ] 异常处理和重试机制

---

### Phase 4: TR比率排名

**目标**: 实现TR比率20日百分比排名

**文件**: `crates/indicator/src/`

**任务**:
1. [ ] TR比率计算
2. [ ] 20日滚动窗口排名
3. [ ] 百分位计算

---

### Phase 5: SymbolRuleParser 同步

**目标**: 实时规则更新，状态追踪

**文件**: `crates/engine/src/shared/symbol_rules.rs`

**任务**:
1. [ ] 规则变更监听
2. [ ] 规则版本追踪
3. [ ] 缓存失效机制

---

### Phase 6: 其他中优先级

**任务**:
1. [ ] `LeverageCommissionCache` - 杠杆档位缓存
2. [ ] Binance手续费率API
3. [ ] `MarketDetector.validate_data` - 数据校验
4. [ ] `_validate_pine_color_groups` - 多周期分组校验

---

## 验证标准

- [ ] 所有新增模块编译通过
- [ ] 单元测试覆盖核心逻辑
- [ ] 数据流转符合CheckTable架构
