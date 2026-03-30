RUST 量化交易系统 - 测试指南

================================================================================
测试 crate 状态
================================================================================

g_test crate 当前为 DISABLED（禁用）状态。

原因：该 crate 存在 544 个编译错误需要修复。
该 crate 暂时从构建中排除，直至错误修复完成。

请勿尝试在当前阶段构建或测试 g_test。

================================================================================
测试位置
================================================================================

单元测试:
- #[test] 函数放置在与被测代码相同的文件中
- 规范：位于 crate/src/module.rs 文件底部
- 示例：crate/src/indicators/src/lib.rs 底部有 #[cfg(test)] 模块

集成测试:
- 位置：crates/*/tests/ 目录
- 示例：crates/b_data_source/tests/ 用于数据源集成测试
- 示例：crates/f_engine/tests/ 用于执行引擎测试

测试模块独立编译，仅在运行 cargo test 时执行。

================================================================================
测试工具
================================================================================

ReplaySource - 历史数据回放
位置：b_data_mock/replay_source.rs

用途：从 CSV 文件回放历史市场数据用于测试。

使用模式：
1. 加载包含历史 tick/k线 的 CSV 文件
2. 使用配置创建 ReplaySource（速度、开始时间等）
3. 调用 next() 获取下一个历史数据点
4. 模拟历史数据的实时传递

示例：
let replay = ReplaySource::from_csv("test_data/btc_usdt_1m.csv");
replay.set_speed(1.0);  // 1倍回放速度
while let Some(tick) = replay.next() {
    // 处理 tick
}

MockApiGateway - 沙箱测试
位置：b_data_mock/api/mock_gateway.rs

用途：模拟交易所 API 响应，用于无需真实连接的沙箱测试。

支持：
- 模拟订单下单和取消
- 模拟市场数据订阅
- 模拟账户余额查询
- 模拟网络延迟和故障

示例：
let gateway = MockApiGateway::new();
gateway.mock_order_response(OrderType::Limit, true);  // 始终成功
gateway.mock_fill_response(vec![Fill::partial(100, 50.0)]);

MockAccount - 账户模拟
位置：通常在 b_data_mock 或测试工具中

用途：模拟账户状态，无需真实交易所连接即可进行测试。

MockConfig - 配置模拟
用途：提供覆盖生产默认值的测试配置。

使用 MockConfig 的场景：
- 设置伪造的 API 密钥
- 配置测试特定的超时时间
- 将交易所端点覆盖为本地主机

================================================================================
平台守卫
================================================================================

某些测试是平台特定的，使用条件编译：

Windows 特定测试：
#[cfg(windows)]
#[test]
fn test_windows_path_handling() {
    // Windows 路径处理测试
}

Linux 特定测试：
#[cfg(target_os = "linux")]
#[test]
fn test_linux_socket_handling() {
    // Linux 套接字测试
}

常用守卫：
#[cfg(windows)]
#[cfg(target_os = "linux")]
#[cfg(target_os = "macos")]
#[cfg(all(windows, feature = "test"))]
#[cfg(unix)]

================================================================================
测试模式
================================================================================

MarketDataStoreImpl 测试辅助函数：

用于创建带临时目录的测试实例的常用模式：

impl MarketDataStoreImpl {
    /// 创建用于测试的临时目录新实例。
    /// 丢弃时自动清理临时目录。
    pub fn new_test() -> (Self, TempDir) {
        let temp_dir = TempDir::new("market_data_test").unwrap();
        let path = temp_dir.path().to_path_buf();
        let store = Self::new(path);
        (store, temp_dir)
    }
}

用法：
#[test]
fn test_store_and_retrieve() {
    let (store, _temp_dir) = MarketDataStoreImpl::new_test();
    store.insert(price_data).unwrap();
    let retrieved = store.get(&symbol).unwrap();
    assert_eq!(retrieved.price, expected_price);
}

================================================================================
测试框架
================================================================================

除 cargo test 外，不使用正式的测试框架。

标准 Rust 测试：
- #[test] 用于测试函数
- #[cfg(test)] 用于测试模块
- #[should_panic] 用于预期 panic
- #[ignore] 用于在正常 cargo test 中不运行的测试

当前未使用外部测试框架如 rstest、proptest 等。
如需属性测试，请先与团队讨论。

运行所有测试：
cargo test

运行特定 crate 的测试：
cargo test -p b_data_source

运行并输出：
RUST_BACKTRACE=1 cargo test -- --nocapture

================================================================================
用于可观测性测试的 PIPELINE STORE
================================================================================

PipelineStore 提供可观测性测试能力。

位置：可能在 c_data_process 或类似的管道相关 crate 中。

功能：
- 通过管道的 trace_id 追踪
- 组件的版本追踪
- 时间/指标收集

用途：验证可观测性基础设施正确地在数据处理管道中传播上下文。

示例用法：
let store = PipelineStore::new_test();
let trace_id = store.insert_trace("tick_processing".to_string());
// 通过管道处理
let completed = store.get_trace(trace_id).unwrap();
assert!(completed.stages.len() > 0);

trace_id：单个管道执行的唯一标识符，用于跨组件关联日志和指标。

版本追踪：记录处理数据的每个组件的版本，用于调试版本不匹配问题。

================================================================================
运行测试
================================================================================

基本测试运行：
cargo test

禁用输出捕获运行（查看 println）：
cargo test -- --nocapture

运行特定测试：
cargo test test_order_execution

以发布模式运行测试（可能发现不同 bug）：
cargo test --release

运行所有特性测试：
cargo test --all-features

检查测试覆盖率（需要 tarpaulin）：
cargo tarpaulin --verbose

================================================================================
手动测试
================================================================================

交易系统手动测试：

1. 使用 b_data_mock 生成模拟市场数据
2. 连接到沙箱交易所（如果有）
3. 使用模拟交易模式验证行为，无需真实资金

手动测试清单：
- [ ] 系统启动无 panic
- [ ] 市场数据流经管道
- [ ] 订单可创建和追踪
- [ ] 成交后持仓正确更新
- [ ] 风险限额被执行
- [ ] 日志包含 trace_id 用于调试

================================================================================
测试数据
================================================================================

测试数据文件通常存储在：
- crates/*/test_data/ 目录
- 或仓库根目录的 test_data/

市场数据的 CSV 格式：
timestamp,symbol,open,high,low,close,volume
2024-01-01T00:00:00Z,BTCUSDT,50000.0,50100.0,49900.0,50050.0,100.5

确保测试数据：
- 具有真实的价格范围
- 不包含 NaN 或无穷值
- 使用正确的 DateTime<Utc> 时间戳
- 没有会触发序列警告的缺口
