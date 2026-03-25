================================================================
CONCERNS.md - Technical Debt, Issues, and Fragile Areas
================================================================
Project: barter-rs 量化交易系统
Author: Code Analysis
Date: 2026-03-25
Status: In Progress
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
- 资金计算精度问题：mock_binance_gateway.rs 中浮点运算可能导致资金不一致
- Check 链性能：每次 check 都创建新的 MinSignalGenerator 实例
- RateLimiter 日志使用 println! 而非 tracing

【中优先级】
- 重复数据结构：SymbolRulesData 在多个模块中定义
- 错误吞没：多处使用 unwrap_or 默认值隐藏解析错误
- WebSocket 重连后订阅状态可能丢失

【低优先级】
- 代码重复：多个模块有类似的错误处理逻辑
- 注释与代码不一致

================================================================
2. 技术债务
================================================================

【TD-001】重复的 SymbolRulesData 定义
位置:
  - crates/a_common/src/api/binance_api.rs (第1238-1268行)
  - crates/a_common/src/backup/memory_backup.rs (第296-312行)

问题: 两个结构体几乎完全相同，违反 DRY 原则
影响: 维护成本增加，类型不一致风险
建议: 统一使用一个定义，通过 re-export 共享

【TD-002】重复的持仓数据结构
位置:
  - crates/e_risk_monitor/src/position/position_manager.rs (LocalPosition)
  - crates/e_risk_monitor/src/persistence/sqlite_persistence.rs (PositionSnapshot, ExchangePositionRecord)

问题: 多处定义相似的持仓结构体
建议: 统一持仓类型，使用泛型或 trait 抽象

【TD-003】RateLimiter 使用 f64 导致精度损失
位置: crates/a_common/src/api/binance_api.rs (第128-131行)

代码:
    if let Ok(weight) = weight_str.parse::<f64>() {
        *used_weight = weight as u32;  // 精度损失

问题: f64 -> u32 转换会丢失小数部分，且 weight > 0.0 的判断不够精确
建议: 使用 Decimal 或直接解析为 u32

【TD-004】日志使用 println! 而非 tracing
位置: crates/a_common/src/api/binance_api.rs (多处)

代码示例 (第78-82行):
    println!("[RateLimiter] 设置 REQUEST_WEIGHT 限制: {}", limit.limit);

问题: println! 在生产环境中无法被日志系统捕获
建议: 全部替换为 tracing::info! 或 tracing::warn!

【TD-005】检查链中重复创建 Generator 实例
位置: crates/d_checktable/src/h_15m/check/a_exit.rs (第33-39行)

代码:
    pub fn check_long_exit(input: &MinSignalInput) -> bool {
        let generator = MinSignalGenerator::new();  // 每次调用都创建新实例
        let status_gen = MinMarketStatusGenerator::new();
        ...
    }

问题: 每帧都重新创建 generator，浪费内存且无法保持状态
建议: 使用单例模式或缓存 generator 实例

================================================================
3. Bug 风险
================================================================

【BUG-001】MockPosition unrealized_pnl 计算不完整
位置: crates/f_engine/src/order/mock_binance_gateway.rs (第195-209行)

代码:
    pub fn update_pnl(&self, symbol: &str, current_price: Decimal) {
        let mut positions = self.positions.write();
        if let Some(pos) = positions.get_mut(symbol) {
            if pos.long_qty > Decimal::ZERO {
                let long_pnl = (current_price - pos.long_avg_price) * pos.long_qty;
                pos.unrealized_pnl = long_pnl;  // 直接覆盖，非累加
            }
            if pos.short_qty > Decimal::ZERO {
                let short_pnl = (pos.short_avg_price - current_price) * pos.short_qty;
                pos.unrealized_pnl += short_pnl;  // 只有空头时才是累加
            }
        }
    }

问题: 当同时有多头和空头持仓时，计算逻辑混乱（先覆盖再加）
建议: 分别计算多头和空头盈亏，最后累加

【BUG-002】订单簿深度数据排序可能反向
位置: crates/a_common/src/backup/memory_backup.rs (第836行)

代码:
    fn trim_depth_entries(&self, v: &mut Vec<DepthEntry>, max: usize) {
        while v.len() > max {
            v.remove(0);
        }
        v.sort_by(|a, b| b.price.cmp(&a.price));  // 降序排列
    }

问题: bids 应该升序（价格从低到高），asks 应该降序（价格从高到低）
     统一使用降序可能导致买卖盘数据混乱
建议: 添加 direction 参数区分 bids/asks

【BUG-003】K线时间戳边界处理可能丢失数据
位置: crates/b_data_source/src/ws/kline_1m/kline.rs (第60-71行)

代码:
    fn period_start(&self, timestamp: DateTime<Utc>) -> DateTime<Utc> {
        match self.period {
            Period::Minute(m) => {
                let minutes = (timestamp.timestamp() / 60 / m as i64) * 60 * m as i64;
                DateTime::from_timestamp(minutes as i64, 0).unwrap()  // unwrap 可能 panic
            }
            ...
        }
    }

问题: unwrap() 在边界情况下可能 panic
建议: 返回 Result 或使用 expect 并添加说明

【BUG-004】WebSocket 订阅后 is_subscribed 状态不准确
位置: crates/a_common/src/ws/websocket.rs (BinanceCombinedStream)

代码:
    pub async fn subscribe(&mut self, streams: &[String]) -> Result<(), MarketError> {
        ...
        self.subscribed = true;  // 只设置标志，不验证服务器响应
        tracing::info!("Subscribed to streams: {:?}", streams);
        Ok(())
    }

问题: 订阅消息发送后不等待服务器确认就设置 subscribed=true
     如果订阅失败，状态仍然显示为已订阅
建议: 等待服务器确认后再设置标志

【BUG-005】decimal 解析错误被静默忽略
位置: crates/b_data_source/src/ws/kline_1m/ws.rs (第313-316行)

代码:
    let parse_price = |s: &str| -> rust_decimal::Decimal {
        s.parse::<rust_decimal::Decimal>().unwrap_or(rust_decimal::Decimal::ZERO)
    };

问题: 如果 Binance 发送无效价格，静默返回 0 可能导致严重后果
建议: 解析失败时记录错误并通知风控

================================================================
4. 安全问题
================================================================

【SEC-001】敏感信息可能通过日志泄露
位置: 多处

问题: API 密钥、订单ID等敏感信息可能出现在日志中
     trades CSV 文件包含完整交易细节
建议:
  - 敏感字段脱敏后再记录日志
  - 添加敏感字段白名单机制

【SEC-002】缺少请求超时配置
位置: crates/a_common/src/api/binance_api.rs

问题: HTTP 请求没有设置超时，可能导致线程阻塞
建议: 为 Client 配置 timeout:
    Client::builder()
        .timeout(Duration::from_secs(10))
        .build()

【SEC-003】文件路径遍历风险
位置: crates/a_common/src/backup/memory_backup.rs

代码:
    pub async fn save_symbol_rules(&self, symbol: &str, rules: &SymbolRulesData) {
        let path = format!("{}/{}/{}.json", self.tmpfs_dir, RULES_DIR, symbol);
        ...
    }

问题: 如果 symbol 包含 "../" 可能导致路径遍历
建议: 验证 symbol 只包含合法字符，或使用 sanitize_username 类似函数

================================================================
5. 性能问题
================================================================

【PERF-001】内存备份频繁序列化和反序列化
位置: crates/a_common/src/backup/memory_backup.rs

问题: 每次保存都执行完整的 JSON 序列化
     append_trade 每次都检查文件大小并可能创建新文件
建议: 使用缓冲写入，定期刷新到磁盘

【PERF-002】SQLite 写入可能阻塞主线程
位置: crates/e_risk_monitor/src/persistence/sqlite_persistence.rs

问题: SQLite 写入操作是同步的，可能阻塞交易线程
建议: 使用异步写入或批量提交机制

【PERF-003】K线历史文件无限增长
位置: crates/b_data_source/src/ws/kline_1m/ws.rs (write_to_history)

代码:
    // 读取现有数据或创建新数组
    let mut data: Vec<serde_json::Value> = Vec::new();
    if std::path::Path::new(&path).exists() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(existing) = serde_json::from_str::<Vec<serde_json::Value>>(&content) {
                data = existing;
            }
        }
    }
    // 追加新K线
    data.push(ohlcvt);

问题: 每次追加都读取整个文件到内存，文件越大越慢
建议: 使用追加写入模式，或限制文件大小

【PERF-004】检查链并发执行但结果串行处理
位置: crates/d_checktable/src/h_15m/check/check_chain.rs

代码:
    pub fn run_check_chain(symbol: &str, input: &MinSignalInput) -> Option<TriggerEvent> {
        let exit_result = a_exit::check(input);
        let close_result = b_close::check(input);
        ...

问题: 注释说"并发执行以提高吞吐"，但实际是串行调用
建议: 使用 tokio::spawn 并行执行各检查

================================================================
6. 脆弱区域
================================================================

【FRAG-001】WebSocket 重连逻辑
文件: crates/a_common/src/ws/websocket.rs

脆弱性:
  - 重连使用指数退避但没有最大重试次数
  - 重连后订阅状态需要手动恢复
  - 断开连接检测依赖消息超时

建议: 添加最大重试次数限制，实现自动订阅恢复机制

【FRAG-002】内存备份同步
文件: crates/a_common/src/backup/memory_backup.rs

脆弱性:
  - sync_to_disk 失败时会记录错误但继续运行
  - 磁盘空间不足时可能静默失败
  - 同步期间内存数据可能不一致

建议: 添加同步状态检查，失败时通知风控

【FRAG-003】交易所 API 限流处理
文件: crates/a_common/src/api/binance_api.rs

脆弱性:
  - 限流时只是等待，不尝试调整请求模式
  - 多个 API 调用竞争同一个 rate_limiter
  - 测试网和实盘限流规则不同

建议: 实现智能限流，调整请求优先级

【FRAG-004】回滚机制完整性
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
问题: b_data_source 依赖 a_common，但 a_common 的某些模块
     (如 config/Paths) 也被业务逻辑直接使用

建议: 明确分层，a_common 只做基础设施

【ARCH-002】状态管理分散
问题: EngineState, LocalPositionManager, AccountPool 等都有独立的状态
     没有统一的全局状态视图

建议: 引入统一的状态管理中枢

【ARCH-003】错误类型不统一
问题:
  - MarketError 定义在 a_common
  - EngineError 也定义在 a_common
  - 各子模块还有自己的错误类型

建议: 建立统一的错误层次体系

================================================================
附录：关键文件索引
================================================================

高风险文件:
  - crates/a_common/src/api/binance_api.rs      (RateLimiter, API 调用)
  - crates/a_common/src/ws/websocket.rs         (WebSocket 连接)
  - crates/f_engine/src/order/mock_binance_gateway.rs  (订单执行)
  - crates/b_data_source/src/ws/kline_1m/ws.rs  (K线数据)
  - crates/d_checktable/src/h_15m/check/a_exit.rs (退出检查)

测试覆盖不足区域:
  - 并发订单处理
  - 网络中断恢复
  - 内存不足场景
  - 部分成交处理

================================================================
