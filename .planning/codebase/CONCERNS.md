================================================================
CONCERNS.md - 技术债务、已知问题和架构隐患
================================================================

Author: Claude Code Analysis
Created: 2026-03-28
Updated: 2026-03-28
Status: P0问题已修复，待处理P1
================================================================

## 修复状态

| 问题 | 状态 | 修复日期 |
|------|------|----------|
| #![allow(dead_code)] | ✅ 已修复 | 2026-03-28 |
| check_risk() 未实现 | ✅ 已修复 | 2026-03-28 |
| strategy_loop.rs 废弃 | ⚠️ 已注释re-export | 2026-03-28 |

================================================================
一、死代码警告 (#![allow(dead_code])
================================================================

一、死代码警告 (#![allow(dead_code])
================================================================

【严重】6个核心crate的lib.rs全部禁用dead_code警告:

  crates/a_common/src/lib.rs
  crates/b_data_source/src/lib.rs
  crates/d_checktable/src/lib.rs
  crates/f_engine/src/lib.rs
  crates/e_risk_monitor/src/lib.rs
  crates/c_data_process/src/lib.rs

影响:
  - 无法通过编译器发现真正无用的代码
  - 代码库中可能存在大量从未调用的函数
  - 阻止Rust编译器的自动优化

建议:
  - 移除#![allow(dead_code)]，逐个解决警告
  - 或在特定模块/函数上使用#[allow(dead_code)]，而非全局

【废弃但未删除】strategy_loop.rs (f_engine/src/core/):
  - 文件头部标注"⚠️ 已废弃 (v3.0)"
  - 但代码仍在仓库中
  - 混用parking_lot::RwLock和tokio::sync::RwLock
  - 应尽快删除或完全迁移到EventEngine


二、Panic风险 - unwrap()/expect() 使用
================================================================

【高风险】全库281处unwrap()/expect()，分布在47个文件

主要风险区域:

1. crates/a_common/src/api/binance_api.rs
   - 1408行: serde_json::from_str(json).unwrap()
   - 1450行: BinanceAccountInfo 解析
   - 1470行: BinancePositionRisk 解析
   - 1518行: BinanceLeverageBracket 解析
   - 1566行: FuturesAccountResponse 解析
   - 1587行: FuturesPositionResponse 解析
   影响: 网络响应格式变化时直接panic

2. crates/c_data_process/src/strategy_state/db.rs
   - 多处unwrap()用于测试代码
   - in_memory()/save()/load()等核心函数
   影响: 数据库操作失败时panic

3. crates/g_test/src/ 多个测试文件
   - 测试代码中大量unwrap()
   - 虽在test环境，但表明错误处理不完善

4. crates/e_risk_monitor/src/shared/account_pool.rs
   - freeze()/deduct_margin()等函数
   - 多处unwrap()用于资金操作
   影响: 资金计算失败时panic，可能导致数据不一致

建议修复:
   - 所有外部输入解析改用unwrap_or()/unwrap_or_else()
   - 资金操作必须返回Result，利用?传播错误
   - 使用thiserror定义专用错误类型


三、锁使用策略问题
================================================================

【严重】parking_lot::RwLock 与 tokio::sync::RwLock 混用

位置: crates/f_engine/src/core/strategy_loop.rs
   29行: use parking_lot::RwLock;
   34行: use tokio::sync::RwLock as TokioRwLock;

问题:
   - 两种锁的API不同，parking_lot是同步锁，tokio是异步锁
   - 混用增加复杂度，容易出错
   - 异步代码中调用同步锁需要spawn_blocking

位置: crates/d_checktable/src/h_15m/trader.rs
   - 442,453,464行: "使用spawn_blocking访问parking_lot::RwLock"注释
   - 1044,1077,1195,1300行: "使用parking_lot::RwLock，read()阻塞式获取"
   - 1326,1335行: "使用parking_lot::RwLock，write()阻塞式获取"
   问题: 在异步上下文中频繁使用同步锁，性能损失

位置: crates/b_data_source/src/api/symbol_registry.rs
   - 使用tokio::sync::RwLock
   其他位置大多使用parking_lot::RwLock

建议:
   - 统一锁策略，高频路径(策略/指标)使用parking_lot
   - 异步上下文中的parking_lot锁用spawn_blocking包装
   - 或迁移到完全异步架构，避免混合


四、已知BUG标记
================================================================

【BUG-005】K线价格解析失败
文件: crates/b_data_source/src/ws/kline_1m/ws.rs
   359行: tracing::error!("[BUG-005] K线价格解析失败...");
   369行: tracing::error!("[BUG-005] K线价格解析失败，跳过...");
位置: 350-371行的parse_price闭包
问题: 解析失败时跳过tick，但已解析的K线数据可能不完整
影响: 数据不连续，指标计算可能异常


五、TODO 标记
================================================================

【P0 - 必须实现】

1. event_engine.rs:380 - 风控检查未实现
   async fn check_risk(&self, _decision: &TradingDecision) -> bool {
       // TODO: 实现风控检查
       true
   }
   问题: 当前直接返回true，任何订单都会被接受

2. trader.rs:730 - GC定时任务违反事件驱动原则
   // TODO: 重构为按需清理或外部驱动
   问题: 使用tokio::spawn启动后台任务，违反事件驱动架构


【P1 - 重要但不紧急】

3. build_signal_input() 硬编码问题
   文件: d_checktable/src/h_15m/trader.rs:214附近
   问题: 信号输入使用硬编码的fallback值，非真实数据
   状态: TODO已标注，待接入真实数据

4. 多处planning文档中的TODO:
   - h_15m_P0修复方案_20260327.md
   - h_15m_深度检查报告_完整版_20260327.md


六、架构设计问题
================================================================

【架构】事件驱动迁移未完成
   - strategy_loop.rs废弃但未删除
   - EventEngine已实现但可能未完全替代旧架构
   - 两套架构并存增加维护成本

【设计】沙盒/生产代码耦合
   - mock_ws和mock_api存在于生产代码中
   - 通过配置切换，而非完全分离
   - 错误配置可能导致生产事故

【性能】Arc::clone()链过长
   - 多处Arc::clone(&self.xxx)创建新引用
   - trader.rs:368,470,491,737,741,1488
   - processor.rs:567,568
   - 频繁clone增加内存压力

【设计】双写存储
   - MarketDataStoreImpl组合MemoryStore + HistoryStore
   - store_impl.rs:34-38从history恢复memory
   - 可能存在数据不一致窗口

【错误处理】统一错误类型缺失
   - 各模块定义自己的error类型
   - thiserror使用不统一
   - 错误传播链断裂风险


七、安全隐患
================================================================

【低】parking_lot Mutex用于异步上下文
   - 多处使用spawn_blocking包装
   - 如果阻塞时间过长会影响异步任务调度

【中】Binance API响应解析panic风险
   - binance_api.rs大量unwrap()
   - API格式变更会导致整个系统崩溃
   - 建议增加版本兼容和优雅降级

【低】hardcoded配置值
   - 多处使用dec!()硬编码参数
   - 交易参数(手续费/滑点等)应可配置


八、测试覆盖缺口
================================================================

1. integration测试相对完整(g_test crate)
2. 单元测试覆盖率未知
3. 错误路径测试缺失(网络失败/解析失败)
4. 并发测试缺失(锁竞争/死锁)


九、依赖管理
================================================================

1. 需定期检查依赖更新:
   - tokio
   - rust_decimal
   - parking_lot
   - rusqlite
   - serde

2. 建议添加:
   - cargo-audit检查安全漏洞
   - cargo-licenses检查许可合规


================================================================
建议优先级:
================================================================

P0 (立即修复) - ✅ 已完成:
  1. ✅ 移除#![allow(dead_code)]或改为精确标注
  2. ✅ 实现check_risk()风控检查
  3. ⏳ 修复binance_api.rs的unwrap() panic风险
  4. ⏳ 统一parking_lot/tokio锁策略

P1 (近期修复):
  5. ⏳ 删除废弃的strategy_loop.rs（已注释re-export）
  6. ⏳ 重构GC任务为外部驱动
  7. ⏳ 完善错误类型统一
  8. ⏳ 增加集成测试覆盖率

P2 (持续改进):
  9. ⏳ 优化Arc::clone()使用
  10. ⏳ 完善配置文件外部化
  11. ⏳ 添加cargo-audit到CI
================================================================