# Milestone 1.0: 事件驱动协程自治架构迁移

## 目标
将 Barter-Rs 从 PipelineBus+Actor 模式迁移到 v3.1 事件驱动协程自治架构。

## 迁移策略
**选择 B**: 完全迁移到 v3.1（废弃 PipelineBus+Actor，采用 SharedStore+独立协程）

## 依赖关系
```
Phase 1 (StateCenter API标准化)
    │
    ├── Phase 2 (EngineManager 自动重启)
    ├── Phase 3 (风控服务两阶段抽取)
    ├── Phase 4 (SharedStore 序列号完善)
    ├── Phase 5 (独立指标服务实现) ← 依赖 Phase 4
    └── Phase 6 (策略协程自治 + BarterWin 融合) ← 依赖 Phase 2,3,5
```

## 阶段列表

1. **Phase 1**: StateCenter API 标准化与完善 (0.5天)
2. **Phase 2**: EngineManager 自动重启机制 (1天)
3. **Phase 3**: 风控服务两阶段抽取 (0.5天)
4. **Phase 4**: SharedStore 序列号机制完善 (0.5天)
5. **Phase 5**: 独立指标服务实现 (2天)
6. **Phase 6**: 策略协程自治 + BarterWin 融合 (2.5天)

**总计**: 6.5天

## 验收标准
- [ ] main.rs < 50行
- [ ] StateCenter 轻量心跳上报正常
- [ ] 风控两阶段检查生效
- [ ] 策略协程自循环运行
- [ ] EngineManager 自动重启
- [ ] BarterWin Engine 融合
- [ ] cargo check 零警告
- [ ] cargo test 全通过
- [ ] cargo clippy 全通过
