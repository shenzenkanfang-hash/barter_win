================================================================================
v1.1 MockBinanceGateway + Signal Synthesis Layer 待办清单
================================================================================

## A. MockBinanceGateway - 模拟币安网关
--------------------------------------------------------------------------------
[x] 创建 engine/src/mock_binance_gateway.rs
[x] 实现 MockAccount 模拟账户
[x] 实现 MockPosition 模拟持仓
[x] 实现 MockOrder 模拟订单
[x] 实现 MockMargin 模拟保证金计算
[x] 实现风控检查（与币安一致）
    [x] check_account_balance() 可用余额检查
    [x] check_position_limit() 持仓限制检查
    [x] check_margin_sufficient() 保证金充足检查
    [x] check_forced_liquidation() 强制平仓检查
[x] 实现立即成交机制（Market Order）
[ ] CSV 输出
    [ ] trades.csv 交易记录
    [ ] positions.csv 持仓变化
    [ ] risk_log.csv 风控日志
    [ ] account_snapshot.csv 账户快照
    [ ] indicator_comparison.csv 指标对比

## B. 信号综合层 - 通道退出逻辑
--------------------------------------------------------------------------------
参考:
  - pin_status_detector.py (分钟级): 进入高速/退出高速/马丁策略
  - trend_status_detector.py (日线级): 趋势策略/平仓条件

通道切换逻辑：
  1. 进入高速：15min>=13% 或 1min>=3% → 马丁策略(分钟级)
  2. 退出高速：tr_ratio < 1 → 回到慢速通道
  3. 日线趋势平仓：ma5_close位置 + Pine颜色

[x] 实现 tr_ratio < 1 退出条件判断
[x] 实现日线趋势平仓条件(ma5_close + PineColor)
[x] 实现通道状态变化记录
[ ] 输出 trigger_log.csv

## C. 完整测试用例
--------------------------------------------------------------------------------
[ ] 指标层测试
    [ ] EMA 增量计算测试
    [ ] RSI 计算测试
    [ ] PineColor 判断测试
    [ ] BigCycleCalculator 测试
[ ] 策略层测试
    [ ] TrendStrategy 状态机测试
    [ ] PinStrategy 状态机测试
[ ] 风控层测试
    [ ] RiskPreChecker 测试
    [ ] RiskReChecker 测试
    [ ] AccountPool 测试
[ ] 引擎层测试
    [ ] VolatilityChannel 通道切换测试
    [ ] TradingEngine 集成测试
[x] MockBinanceGateway 测试
    [x] 正常交易流程测试
    [x] 风控拒绝场景测试
    [x] 强制平仓场景测试

## D. 指标对比验证
--------------------------------------------------------------------------------
[ ] 同步输出 Rust 计算的指标值
[ ] 提供 Python 指标对比接口
[ ] 生成 indicator_comparison.csv
[ ] 用户验证准确性

================================================================================
