# TODO - Milestone 1.0

## Phase 1: StateCenter API 标准化与完善
- [ ] 统一 StateCenterTrait API 命名
- [ ] 补全 get_stale() 方法实现
- [ ] 建立心跳超时阈值配置
- [ ] 更新所有现有调用方

## Phase 2: EngineManager 自动重启机制
- [ ] 实现 restart_loop() 后台监控循环
- [ ] 实现 handle_stale() 指数退避重启
- [ ] 实现 StrategyHandle::retry_count 和 active
- [ ] 与 StateCenter.get_stale() 联动

## Phase 3: 风控服务两阶段抽取
- [ ] 重命名 re_check() → final_check()
- [ ] 补全 RiskCheckRequest/RiskCheckResult 结构体
- [ ] 验证 TradeLock 两阶段锁机制

## Phase 4: SharedStore 序列号机制完善
- [ ] 验证 KlineWithSeq 结构实现
- [ ] 验证 StoreOutput<T> 泛型包装
- [ ] 补全 get_since() 增量读取

## Phase 5: 独立指标服务实现
- [ ] 创建 MinIndicatorService（日触发模式）
- [ ] 创建 DayIndicatorService（串行批量模式）
- [ ] 统一 IndicatorStore trait
- [ ] 废弃旧的 SignalProcessorIndicatorStore

## Phase 6: 策略协程自治 + BarterWin 融合
- [ ] 创建 H15mStrategyService trait + 实现
- [ ] 废弃 src/actors.rs 中的 run_strategy_actor()
- [ ] 重构 main.rs 为纯启动引导
- [ ] BarterWin Engine 融合
