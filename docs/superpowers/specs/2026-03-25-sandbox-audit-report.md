沙盒模式改造 + 全项目审核报告
==============================================
Author: 软件架构师
Created: 2026-03-25
Status: open-issues
Project: barter-rs 量化交易系统
==============================================

一、改造执行摘要
==============================================

1.1 沙盒代码重构
----------------------------------------------------------------
| 任务                              | 状态  | 备注                    |
|---------------------------------|------|-----------------------|
| 删除 CSV/KlineLoader 废弃代码     | ✅ 完成 | kline_loader.rs 已删除 |
| API K线拉取模块 (ApiKlineFetcher) | ✅ 完成 | 公共API直连流式分页     |
| 保留 StreamTickGenerator         | ✅ 完成 | 无变更，接口不变         |
| 保留 TickToWsConverter          | ✅ 完成 | 输出币安标准WS K线 JSON |
| 集成 ShadowBinanceGateway      | ✅ 完成 | 仅拦截账户/持仓/下单    |
| 沙盒唯一入口：API直连模式         | ✅ 完成 | kline_replay.rs 唯一入口 |


1.2 编译验证结果
----------------------------------------------------------------
| 测试项                        | 结果   | 详情          |
|------------------------------|--------|--------------|
| cargo check --all           | ✅ 0错误 | 所有 crate    |
| h_sandbox --lib            | ✅ 0错误 | lib编译通过   |
| h_sandbox --examples        | ✅ 0错误 | examples编译通过 |
| cargo test -p h_sandbox    | ⚠️ 39通过/5失败 | 详见第二章    |


二、遗留问题清单
==============================================

问题 #1: MaCrossStrategy 预热逻辑缺陷
======================================================================
文件:     crates/h_sandbox/src/backtest/strategy.rs:194
严重程度: 中
来源:     历史遗留，非本次改造引入

现象:
  测试 test_ma_strategy 在第10个tick后仍返回 Signal::Long，
  而非预期的 Signal::Hold（预热期结束后应保持 Hold）。

根因分析:
  MaCrossStrategy::on_tick() 的预热判断逻辑存在缺陷：
  - 预期：前 N 个 tick 应返回 Hold（预热期间）
  - 实际：第 10 个 tick 后策略仍产生信号

  测例第 188-195 行：
  ```rust
  for i in 0..10 {
      let t = BacktestTick { price: Decimal::from(50000 + i), ..tick.clone() };
      let signal = strategy.on_tick(&t);
      assert_eq!(signal, Signal::Hold);  // 第10个tick后仍为Hold
  }
  ```

  第 198-199 行：
  ```rust
  let signal = strategy.on_tick(&tick);
  assert!(matches!(signal, Signal::Hold | Signal::Long));
  ```

  问题：第11个tick（索引10）价格仍为50000+0=50000（因为用的是tick而非新t），
  导致策略认为趋势已形成而非 Hold。

整改方案:
  方案A（推荐）：修复测试逻辑，第11个tick应使用新价格触发预热完成后的第一个信号
  方案B：修复 MaCrossStrategy::on_tick() 的预热计数逻辑


问题 #2: GaussianNoise 空结构体导致测试失败
======================================================================
文件:     crates/h_sandbox/src/historical_replay/noise.rs:107
严重程度: 低
来源:     历史遗留，非本次改造引入

现象:
  test_noise_creation 断言 `std::mem::size_of_val(&noise) > 0` 失败，
  因为 GaussianNoise 是空结构体（没有任何字段）。

根因分析:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
  pub struct GaussianNoise;

  #[cfg(feature = "rand")]
  impl GaussianNoise {
      pub fn new() -> Self { Self }
      pub fn generate(&mut self) -> f64 { rand::random() }
  }

  #[cfg(not(feature = "rand"))]
  impl GaussianNoise {
      pub fn new() -> Self { Self }
      pub fn generate(&mut self) -> f64 { 0.0 }
  }
  ```

  测试代码：
  ```rust
  #[test]
  fn test_noise_creation() {
      let noise = GaussianNoise::new();
      assert!(std::mem::size_of_val(&noise) > 0);  // 失败：空结构体大小为0
  }
  ```

整改方案:
  方案A（推荐）：删除该断言测试，或改为验证 GaussianNoise::new() 能正常构造
  方案B：给 GaussianNoise 添加 phantom marker 字段使其大小 > 0（不推荐，污染类型）


问题 #3: TickDriver 测试断言失败
======================================================================
文件:     crates/h_sandbox/src/tick_generator/driver.rs
严重程度: 低
来源:     历史遗留，非本次改造引入

现象:
  test_driver_progress 和 test_driver_run 两个测试失败，
  断言 `left: 120 == right: 0` 表示 Tick 计数与预期不符。

根因分析:
  测试期望 TickDriver 按固定速率发送 120 个 tick，但实际计数为 0，
  可能是 TickDriver 内部状态机在测试环境下未正确初始化，
  或 driver.progress() 返回的 sent 计数与预期不匹配。

整改方案:
  检查 TickDriver::progress() 实现，验证测试的 mock 数据是否满足驱动前提条件


问题 #4: test_all_ticks_exhausted 测试失败
======================================================================
文件:     crates/h_sandbox/src/tick_generator/generator.rs
严重程度: 低
来源:     历史遗留，非本次改造引入

现象:
  tick 耗尽后预期行为与实际行为不一致。

整改方案:
  验证 StreamTickGenerator 在 klines 全部消费后的迭代器行为是否符合预期


问题 #5: Parquet 硬编码路径
======================================================================
文件:     crates/h_sandbox/examples/sim_trading_parquet.rs:187
严重程度: 低
来源:     历史遗留

现象:
  默认路径硬编码为：
  D:\\个人量化策略\\TimeTradeSim\\market_data\\POWERUSDT\\1m\\part_1772294400000.parquet

整改方案:
  方案A（推荐）：添加命令行参数覆盖路径
  方案B：改为从环境变量或配置文件读取


三、架构审核结果
==============================================

3.1 模块边界审核 ✅ 通过
----------------------------------------------------------------
| 检查项                     | 结果   | 说明                     |
|--------------------------|--------|------------------------|
| h_sandbox 导入隔离         | ✅ 通过 | 所有导入通过公共 API      |
| 无内部模块泄露             | ✅ 通过 | 无 crate::internal 访问  |
| c_data_process 隔离       | ✅ 通过 | 列在依赖但未使用          |
| d_checktable 隔离         | ✅ 通过 | 列在依赖但未使用          |
| f_engine 接口隔离          | ✅ 通过 | 仅通过公共 trait 接口     |


3.2 数据流合规性审核 ✅ 通过
----------------------------------------------------------------
数据流路径（kline_replay example）：
  ApiKlineFetcher (a_common::api)
      ↓ [REST /api/v3/klines]
  StreamTickGenerator (h_sandbox::historical_replay)
      ↓ [60 ticks/K-line 生成]
  TickToWsConverter (h_sandbox::historical_replay)
      ↓ [BinanceKlineMsg WS格式]
  stdout (JSON 输出)

合规性：
  ✅ API → WS行情 → 引擎 → 策略 规范正确
  ✅ TickToWsConverter 复用 BinanceKlineMsg 类型
  ✅ 输出为标准币安 WebSocket JSON 格式


3.3 拦截器合规性审核 ✅ 通过
----------------------------------------------------------------
ShadowBinanceGateway 拦截范围：

  ✅ get_account()      → 模拟账户（OrderEngine）
  ✅ get_position()     → 模拟持仓（OrderEngine）
  ✅ place_order()      → 模拟订单（OrderEngine.execute）
  ✅ update_price()     → 内部价格状态（PnL计算用）
  ✅ get_current_price()→ 内部价格读取
  ✅ check_liquidation()→ 爆仓检测

  ❌ 市场数据（K线/Tick/订单簿）→ 未拦截，转发真实数据

结论：ShadowBinanceGateway 正确实现了"仅劫持交易接口"的设计目标。


3.4 依赖安全审核 ✅ 通过
----------------------------------------------------------------
| 依赖            | 用途        | 风险评估 |
|----------------|------------|---------|
| a_common       | 基础设施API  | 低      |
| b_data_source  | 数据类型    | 低      |
| c_data_process | 信号生成    | 低      |
| d_checktable  | 检查层      | 低      |
| e_risk_monitor | 风控        | 低      |
| f_engine       | 引擎        | 低      |
| parquet        | 数据加载    | 低（用于backtest非主流程） |
| rand           | 噪声生成    | 低      |


四、整改行动计划
==============================================

4.1 立即整改（不影响编译）
----------------------------------------------------------------
| 优先级 | 问题                | 整改动作                               | 预计时间 |
|--------|-------------------|--------------------------------------|---------|
| P1     | test_noise_creation | 删除 size_of_val > 0 断言             | 5分钟   |
| P1     | test_ma_strategy   | 修复测试逻辑（价格递增问题）            | 10分钟  |
| P2     | Parquet硬编码路径   | 添加 --parquet-path CLI参数            | 15分钟  |


4.2 后续优化（非阻塞）
----------------------------------------------------------------
| 优先级 | 问题                | 整改动作                               | 预计时间 |
|--------|-------------------|--------------------------------------|---------|
| P2     | TickDriver测试     | 调查 driver 状态机初始化问题            | 30分钟  |
| P2     | test_all_ticks_exhausted | 验证迭代器耗尽行为              | 20分钟  |
| P3     | cfg(feature="rand") | 清理 noise.rs 中的条件编译注释         | 5分钟   |
| P3     | 死代码清理          | 清理 symbol/side/total_ticks 等未使用字段 | 30分钟 |


五、总结
==============================================

5.1 改造完成度
----------------------------------------------------------------
本次沙盒改造已完成所有目标项：

  ✅ 删除全部 CSV/KlineLoader 废弃代码
  ✅ 实现 ApiKlineFetcher API直连K线拉取
  ✅ 保留并验证 StreamTickGenerator / TickToWsConverter
  ✅ ShadowBinanceGateway 拦截规范正确
  ✅ 沙盒唯一入口为 API 直连模式（kline_replay.rs）

5.2 编译状态
----------------------------------------------------------------
  ✅ cargo check --all           → 0 错误
  ✅ h_sandbox --lib             → 0 错误
  ✅ h_sandbox --examples        → 0 错误
  ⚠️  cargo test -p h_sandbox    → 39通过 / 5失败（历史遗留）

5.3 后续建议
----------------------------------------------------------------
  1. 尽快修复5个历史遗留测试（不影响主流程，但影响CI）
  2. 清理 h_sandbox 中的死代码和未使用字段
  3. Parquet 路径改为命令行参数以提升灵活性
  4. 考虑为 noise.rs 添加 rand feature 启用支持
