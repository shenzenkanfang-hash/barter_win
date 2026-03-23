================================================================================
v1.1 Phase Plan - MockBinanceGateway + Signal Synthesis Layer
================================================================================

## Phase Goal
实现 MockBinanceGateway（模拟币安网关）和信号综合层（通道退出逻辑）

## Sub-Tasks

### A. MockBinanceGateway - 模拟币安网关
--------------------------------------------------------------------------------
[A.1] 创建 engine/src/mock_binance_gateway.rs
     - 实现 MockAccount 模拟账户结构
     - 实现 MockPosition 模拟持仓结构
     - 实现 MockOrder 模拟订单结构
     - 实现 MockMargin 模拟保证金计算
     - 实现 ExchangeGateway trait

[A.2] 实现风控检查（与币安一致）
     - check_account_balance() 可用余额检查
     - check_position_limit() 持仓限制检查（95%）
     - check_margin_sufficient() 保证金充足检查
     - check_forced_liquidation() 强制平仓检查（0.5%）

[A.3] 实现立即成交机制（Market Order）
     - 市价单立即成交
     - 更新持仓、保证金、未实现盈亏

[A.4] CSV 输出功能
     - trades.csv 交易记录
     - positions.csv 持仓变化
     - risk_log.csv 风控日志
     - account_snapshot.csv 账户快照
     - indicator_comparison.csv 指标对比

### B. Signal Synthesis Layer - 通道退出逻辑
--------------------------------------------------------------------------------
[B.1] 实现 tr_ratio < 1 退出条件判断
     - 参考 pin_status_detector.py check_exit_high_volatility()

[B.2] 实现日线趋势平仓条件
     - ma5_close 位置判断
     - PineColor 颜色判断

[B.3] 输出 trigger_log.csv
     - 通道状态变化记录

### C. Complete Test Coverage
--------------------------------------------------------------------------------
[C.1] 指标层测试
     - EMA 增量计算测试
     - RSI 计算测试
     - PineColor 判断测试
     - BigCycleCalculator 测试

[C.2] 策略层测试
     - TrendStrategy 状态机测试
     - PinStrategy 状态机测试

[C.3] 风控层测试
     - RiskPreChecker 测试
     - RiskReChecker 测试
     - AccountPool 测试

[C.4] 引擎层测试
     - VolatilityChannel 通道切换测试
     - TradingEngine 集成测试

### D. Indicator Comparison - 指标对比验证
--------------------------------------------------------------------------------
[D.1] 同步输出 Rust 计算的指标值
[D.2] 提供 Python 指标对比接口
[D.3] 生成 indicator_comparison.csv

## Verification
- cargo check 通过
- cargo test 通过
- CSV 输出格式正确

## Dependencies
- Phase 7 完成
- Phase 6 TradingEngine 完成
