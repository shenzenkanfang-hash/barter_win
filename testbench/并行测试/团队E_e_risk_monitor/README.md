# 团队E - e_risk_monitor 模块测试

## 负责测试点

| 序号 | 测试点ID | 测试内容 | 优先级 | 状态 |
|------|---------|---------|--------|------|
| 1 | ER-001 | RiskPreChecker 订单预检查 | P0 | 待测 |
| 2 | ER-002 | RiskReChecker 订单复检 | P1 | 待测 |
| 3 | ER-003 | OrderCheck 订单风控校验 | P0 | 待测 |
| 4 | ER-004 | PinRiskLeverageGuard Pin策略杠杆守卫 | P0 | 待测 |
| 5 | ER-005 | PinLeverageConfig Pin策略杠杆配置 | P1 | 待测 |
| 6 | ER-006 | TrendRiskLimitGuard 趋势策略风险限制 | P0 | 待测 |
| 7 | ER-007 | TrendSymbolLimit 单品种限制 | P1 | 待测 |
| 8 | ER-008 | TrendGlobalLimit 全局限制 | P1 | 待测 |
| 9 | ER-009 | calculate_hour_open_notional 小时名义价值计算 | P1 | 待测 |
| 10 | ER-010 | calculate_minute_open_notional 分钟名义价值计算 | P1 | 待测 |
| 11 | ER-011 | LocalPositionManager 本地持仓管理 | P0 | 待测 |
| 12 | ER-012 | PositionExclusionChecker 持仓互斥检查 | P1 | 待测 |
| 13 | ER-013 | AccountPool 账户池管理 | P1 | 待测 |
| 14 | ER-014 | MarginPoolConfig 保证金配置 | P1 | 待测 |
| 15 | ER-015 | MarketStatusDetector 市场状态检测 | P1 | 待测 |
| 16 | ER-016 | PnlManager 盈亏管理 | P1 | 待测 |
| 17 | ER-017 | RoundGuard 舍入保护 | P2 | 待测 |
| 18 | ER-018 | PersistenceService 持久化服务 | P1 | 待测 |
| 19 | ER-019 | SqliteEventRecorder SQLite事件记录 | P2 | 待测 |
| 20 | ER-020 | DisasterRecovery 灾难恢复 | P1 | 待测 |
| 21 | ER-021 | StartupRecoveryManager 启动恢复管理 | P1 | 待测 |

## 依赖关系
- 前置依赖: 团队B (b_data_source 账户数据)

## 执行日志存放路径
testbench/并行测试/执行日志/团队E_*.log

## 测试报告输出路径
testbench/并行测试/团队E_e_risk_monitor/

## 开始时间: ________
## 预计完成时间: ________
## 实际完成时间: ________
## 执行人签字: ________
