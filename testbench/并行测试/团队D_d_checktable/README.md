# 团队D - d_checktable 模块测试

## 负责测试点

| 序号 | 测试点ID | 测试内容 | 优先级 | 状态 |
|------|---------|---------|--------|------|
| 1 | DT-001 | CheckTable 检查表注册与调度 | P0 | 待测 |
| 2 | DT-002 | h_15m::Trader 15分钟高频交易员 | P0 | 待测 |
| 3 | DT-003 | h_15m::Executor 15分钟信号执行器 | P0 | 待测 |
| 4 | DT-004 | h_15m::QuantityCalculator 数量计算器 | P1 | 待测 |
| 5 | DT-005 | h_15m::Repository 持仓仓储 | P1 | 待测 |
| 6 | DT-006 | h_15m::Signal 信号转换 | P1 | 待测 |
| 7 | DT-007 | h_15m::Status 交易状态管理 | P1 | 待测 |
| 8 | DT-008 | l_1d::Signal 1天低频信号 | P1 | 待测 |
| 9 | DT-009 | l_1d::QuantityCalculator 低频数量计算 | P1 | 待测 |
| 10 | DT-010 | l_1d::Status 低频状态管理 | P1 | 待测 |
| 11 | DT-011 | CheckChainContext 检查链上下文传递 | P1 | 待测 |
| 12 | DT-012 | CheckSignal 检查信号生成 | P1 | 待测 |

## 依赖关系
- 前置依赖: 团队C (c_data_process 策略信号输出给 d_checktable)

## 执行日志存放路径
testbench/并行测试/执行日志/团队D_*.log

## 测试报告输出路径
testbench/并行测试/团队D_d_checktable/

## 开始时间: ________
## 预计完成时间: ________
## 实际完成时间: ________
## 执行人签字: ________
