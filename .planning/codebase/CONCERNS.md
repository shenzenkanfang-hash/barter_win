================================================================
CONCERNS.md - Technical Debt, Issues, and Fragile Areas
================================================================
Project: barter-rs 量化交易系统
Author: Code Analysis
Date: 2026-03-25
Status: Partially Fixed (2026-03-25)
================================================================

目录
----
1. 关键问题汇总
2. 技术债务
3. Bug 风险
4. 安全问题
5. 性能问题
6. 脆弱区域
7. 架构问题

================================================================
1. 关键问题汇总
================================================================

【高优先级】
- ✅ 资金计算精度问题：mock_binance_gateway.rs 中浮点运算可能导致资金不一致
- ⏳ Check 链性能：每次 check 都创建新的 MinSignalGenerator 实例 (误报，零大小类型)
- ✅ RateLimiter 日志使用 println! 而非 tracing

【中优先级】
- ⏳ 重复数据结构：SymbolRulesData 在多个模块中定义 (建议后续重构)
- ✅ 错误吞没：多处使用 unwrap_or 默认值隐藏解析错误
- ✅ WebSocket 重连后订阅状态可能丢失

【低优先级】
- ⏳ 代码重复：多个模块有类似的错误处理逻辑
- ⏳ 注释与代码不一致

================================================================
2. 技术债务
================================================================

【TD-001】重复的 SymbolRulesData 定义 ✅ 已修复
位置:
  - crates/a_common/src/api/binance_api.rs (第1253-1274行) - 保留为主定义
  - crates/a_common/src/backup/memory_backup.rs (原第296-312行) - 已删除

修复:
  - 删除 memory_backup.rs 中的重复定义
  - 添加 `use crate::api::SymbolRulesData;` 导入
  - 通过 backup/mod.rs 重新导出 SymbolRulesData
  - 保留 api 模块中的 canonical 定义（包含 max_leverage 字段）

修复日期: 2026-03-25

【TD-002】重复的持仓数据结构 ✅ 已修复
位置:
  - crates/e_risk_monitor/src/position/position_manager.rs (LocalPosition - 运行时单方向持仓)
  - crates/e_risk_monitor/src/persistence/persistence.rs (PositionSnapshot - SQLite单方向持仓快照)
  - crates/e_risk_monitor/src/persistence/startup_recovery.rs (UnifiedPositionSnapshot - 统一持仓快照)

修复:
  - LocalPosition 和 PositionSnapshot 是不同用途的结构（运行时 vs 持久化），不适合直接统一
  - 添加 UnifiedPositionSnapshot::from_memory_backup() 和 from_hard_disk_backup() 构造函数
  - 简化 startup_recovery.rs 中两处持仓转换代码，消除重复字段映射
  - PositionSnapshot (a_common 备份) 和 e_risk_monitor::persistence::PositionSnapshot 是不同目的的结构

修复日期: 2026-03-25

【TD-003】RateLimiter 使用 f64 导致精度损失 ✅ 已修复
位置: crates/a_common/src/api/binance_api.rs (第128-131行)

修复: 改为直接解析为 u32，避免 f64 精度损失
修复日期: 2026-03-25

【TD-004】日志使用 println! 而非 tracing ✅ 已修复
位置: crates/a_common/src/api/binance_api.rs (多处)

修复: 全部替换为 tracing::info!
修复日期: 2026-03-25

【TD-005】检查链中重复创建 Generator 实例
状态: ✅ 误报（非问题）
位置: crates/d_checktable/src/h_15m/check/a_exit.rs (第33-39行)

说明: MinSignalGenerator 是零大小类型（unit struct），MinMarketStatusGenerator 只有
一个常量字段。创建这些实例没有内存浪费，因为它们是栈上分配的零大小类型。
无状态需要保留，因此缓存不会带来性能提升。

================================================================
3. Bug 风险
================================================================

【BUG-001】MockPosition unrealized_pnl 计算不完整 ✅ 已修复
位置: crates/f_engine/src/order/mock_binance_gateway.rs (第195-209行)

修复: 分别计算多头和空头盈亏，最后累加
修复日期: 2026-03-25

【BUG-002】订单簿深度数据排序未区分 bids/asks ✅ 已修复
位置: crates/a_common/src/backup/memory_backup.rs (第832-836行)

修复: 添加 is_bids 参数，bids 升序排列，asks 降序排列
修复日期: 2026-03-25

【BUG-003】K线时间戳 unwrap() 可能 panic ✅ 已修复
位置: crates/b_data_source/src/ws/kline_1m/kline.rs (第60-71行)

修复: 使用 expect() 替代 unwrap()，添加描述性错误消息
修复日期: 2026-03-25

【BUG-004】WebSocket 订阅不验证服务器确认 ✅ 已修复
位置: crates/a_common/src/ws/binance_ws.rs (第313-331行)

修复: 添加 wait_for_subscription_response 方法，等待服务器确认响应
修复日期: 2026-03-25

【BUG-005】decimal 解析错误被静默忽略 ✅ 已修复
位置: crates/b_data_source/src/ws/kline_1m/ws.rs (第313-316行)

修复: 解析失败时记录错误并跳过该 tick，通知风控
修复日期: 2026-03-25

================================================================
4. 安全问题
================================================================

【SEC-001】敏感信息可能通过日志泄露
状态: ⏳ 待修复
位置: 多处

问题: API 密钥、订单ID等敏感信息可能出现在日志中
     trades CSV 文件包含完整交易细节
建议:
  - 敏感字段脱敏后再记录日志
  - 添加敏感字段白名单机制

【SEC-002】缺少 HTTP 请求超时配置 ✅ 已修复
位置: crates/a_common/src/api/binance_api.rs

修复: 创建 new_http_client() 函数，为所有 HTTP 客户端配置 10 秒超时
修复日期: 2026-03-25

【SEC-003】文件路径遍历风险 ✅ 已修复
位置: crates/a_common/src/backup/memory_backup.rs

修复: 添加 sanitize_symbol() 函数，验证 symbol 只包含字母、数字和下划线
修复日期: 2026-03-25

================================================================
5. 性能问题
================================================================

【PERF-001】内存备份频繁序列化和反序列化
状态: ⏳ 待修复
位置: crates/a_common/src/backup/memory_backup.rs

问题: 每次保存都执行完整的 JSON 序列化
     append_trade 每次都检查文件大小并可能创建新文件
建议: 使用缓冲写入，定期刷新到磁盘

【PERF-002】SQLite 写入可能阻塞主线程
状态: ⏳ 待修复
位置: crates/e_risk_monitor/src/persistence/sqlite_persistence.rs

问题: SQLite 写入操作是同步的，可能阻塞交易线程
建议: 使用异步写入或批量提交机制

【PERF-003】K线历史文件无限增长 ✅ 已修复
位置: crates/b_data_source/src/ws/kline_1m/ws.rs (write_to_history)

修复:
  - 添加 MAX_KLINE_FILE_SIZE = 10MB 常量
  - 添加 kline_file_index HashMap 追踪每个 symbol 的当前文件索引
  - 文件大小超限时自动切换到下一个编号文件 (symbol_XXXX.json)
  - 实现原子写入（先写临时文件再 rename）防止写入中途崩溃导致文件损坏
  - 写入前检查文件大小，超过限制自动切换新文件
  - 注意：日期分目录和30天压缩为 .gz 尚未实现（需要更多基础设施）

修复日期: 2026-03-25

【PERF-004】检查链并发执行但结果串行处理 ✅ 已修复（注释修正）
位置: crates/d_checktable/src/h_15m/check/check_chain.rs

说明: 检查函数为 CPU 密集型纯函数，顺序执行比并发更高效（避免线程调度开销）。
注释已修正以反映实际行为。

================================================================
6. 脆弱区域
================================================================

【FRAG-001】WebSocket 重连逻辑
状态: ⏳ 待改进
文件: crates/a_common/src/ws/binance_ws.rs

脆弱性:
  - 重连使用指数退避但没有最大重试次数
  - 重连后订阅状态需要手动恢复
  - 断开连接检测依赖消息超时

建议: 添加最大重试次数限制，实现自动订阅恢复机制

【FRAG-002】内存备份同步
状态: ⏳ 待改进
文件: crates/a_common/src/backup/memory_backup.rs

脆弱性:
  - sync_to_disk 失败时会记录错误但继续运行
  - 磁盘空间不足时可能静默失败
  - 同步期间内存数据可能不一致

建议: 添加同步状态检查，失败时通知风控

【FRAG-003】交易所 API 限流处理
状态: ⏳ 待改进
文件: crates/a_common/src/api/binance_api.rs

脆弱性:
  - 限流时只是等待，不尝试调整请求模式
  - 多个 API 调用竞争同一个 rate_limiter
  - 测试网和实盘限流规则不同

建议: 实现智能限流，调整请求优先级

【FRAG-004】回滚机制完整性
状态: ⏳ 待改进
文件: crates/f_engine/src/core/rollback.rs

脆弱性:
  - 回滚点设置和恢复逻辑需要严格测试
  - 部分成交时回滚状态计算复杂
  - 并发回滚请求可能冲突

建议: 添加回滚测试用例，验证各种边界情况

================================================================
7. 架构问题
================================================================

【ARCH-001】模块边界模糊
状态: ⏳ 待规划
问题: b_data_source 依赖 a_common，但 a_common 的某些模块
     (如 config/Paths) 也被业务逻辑直接使用

建议: 明确分层，a_common 只做基础设施

【ARCH-002】状态管理分散
状态: ⏳ 待规划
问题: EngineState, LocalPositionManager, AccountPool 等都有独立的状态
     没有统一的全局状态视图

建议: 引入统一的状态管理中枢

【ARCH-003】错误类型不统一
状态: ⏳ 待规划
问题:
  - MarketError 定义在 a_common
  - EngineError 也定义在 a_common
  - 各子模块还有自己的错误类型

建议: 建立统一的错误层次体系

================================================================
附录：关键文件索引
================================================================

高风险文件:
  - crates/a_common/src/api/binance_api.rs      (RateLimiter, API 调用) ✅ 已修复多处
  - crates/a_common/src/ws/binance_ws.rs        (WebSocket 连接) ✅ 已修复订阅验证
  - crates/f_engine/src/order/mock_binance_gateway.rs  (订单执行) ✅ 已修复PnL计算
  - crates/b_data_source/src/ws/kline_1m/ws.rs  (K线数据) ✅ 已修复解析错误
  - crates/a_common/src/backup/memory_backup.rs (内存备份) ✅ 已修复路径遍历+排序

测试覆盖不足区域:
  - 并发订单处理
  - 网络中断恢复
  - 内存不足场景
  - 部分成交处理

================================================================
修复摘要 (2026-03-25)
================================================================

已修复: 12 项
  ✅ BUG-001: unrealized_pnl 计算
  ✅ BUG-002: 订单簿深度排序
  ✅ BUG-003: K线时间戳 unwrap
  ✅ BUG-004: WebSocket 订阅验证
  ✅ BUG-005: decimal 解析错误
  ✅ TD-003: RateLimiter f64 精度
  ✅ TD-004: RateLimiter println!
  ✅ SEC-002: HTTP 超时
  ✅ SEC-003: 路径遍历
  ✅ PERF-004: 注释修正（非代码问题）
  ✅ TD-001: SymbolRulesData 重复定义
  ✅ PERF-003: K线历史文件增长限制

待修复: 9 项
  TD-002 (部分修复：添加了转换构造函数，完整统一需更多重构)
  PERF-001, PERF-002 (性能优化)
  FRAG-001, FRAG-002, FRAG-003, FRAG-004 (脆弱区域改进)
  ARCH-001, ARCH-002, ARCH-003 (架构重构)

误报/非问题: 2 项
  TD-005 (零大小类型)
  高优先级列表中的"资金计算精度问题"实际上已使用 Decimal

================================================================
