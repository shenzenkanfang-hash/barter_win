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

问题 #1: MaCrossStrategy 预热逻辑缺陷 ✅ 已修复
======================================================================
文件:     crates/h_sandbox/src/backtest/strategy.rs:194
严重程度: 中
来源:     历史遗留，非本次改造引入

根因:     on_tick() 中 condition < slow_period 应为 <=

修复内容:
  - 策略: `if self.prices.len() <= self.slow_period as usize` (原 <)
  - 测试: 改为循环验证 20 个 tick 不 panic，验证策略在预热后正常产生信号

验证: test_ma_strategy ✅ 通过


问题 #2: GaussianNoise 空结构体导致测试失败 ✅ 已修复
======================================================================
文件:     crates/h_sandbox/src/historical_replay/noise.rs:107
严重程度: 低
来源:     历史遗留

根因:     空结构体大小为 0，断言 `size_of_val > 0` 永远失败

修复内容:
  - 删除错误的 size_of_val 断言
  - 改为 `let _ = noise;` 验证构造成功即可

验证: test_noise_creation ✅ 通过


问题 #3: TickDriver progress() 状态计算错误 ✅ 已修复
======================================================================
文件:     crates/h_sandbox/src/tick_generator/driver.rs
严重程度: 中
来源:     历史遗留

根因:
  progress() 使用公式 `sent = total_ticks - remaining_in_current_kline()`
  但 `remaining_in_current_kline()` 在预加载状态下返回 0（错误），
  导致 `sent = 120 - 0 = 120`（应 0）。

修复内容:
  - TickGenerator 新增 `total_klines` 字段跟踪初始总数
  - 新增 `exhausted_kline_count()` / `current_tick_index()` / `current_kline_is_none()`
  - 重写 progress(): `sent = exhausted*60 + tick_index`
  - 修复 remaining_in_current_kline()：预加载状态返回 TICKS_PER_1M
  - 导出 pub const TICKS_PER_1M

验证: test_driver_progress ✅ 通过, test_driver_run ✅ 通过


问题 #4: test_all_ticks_exhausted is_exhausted() 判断错误 ✅ 已修复
======================================================================
文件:     crates/h_sandbox/src/tick_generator/generator.rs
严重程度: 低
来源:     历史遗留

根因:     is_exhausted() 只检查 `current_kline.is_none() && klines.is_empty()`
  但 next_tick() 的终止条件是 `price_path.pop_front()?`
  当最后一根K线耗尽时，current_kline 仍为 Some（数据残留），
  导致 is_exhausted() 返回 false 而 next_tick() 已返回 None。

修复内容:
  - is_exhausted() 改为检查 `price_path.is_empty() && klines.is_empty()`

验证: test_all_ticks_exhausted ✅ 通过


问题 #5: Parquet 硬编码路径 ⚠️ 待处理
======================================================================
文件:     crates/h_sandbox/examples/sim_trading_parquet.rs:187
严重程度: 低
来源:     历史遗留

现象:
  默认路径硬编码为：
  D:\\个人量化策略\\TimeTradeSim\\market_data\\POWERUSDT\\1m\\part_1772294400000.parquet

整改方案:
  方案A（推荐）：添加 --parquet-path CLI 参数
  方案B：从环境变量读取


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

4.1 立即整改（已完成）
----------------------------------------------------------------
| 优先级 | 问题                | 整改动作                               | 状态   |
|--------|-------------------|--------------------------------------|---------|
| P1     | test_noise_creation | 删除 size_of_val > 0 断言             | ✅ 完成 |
| P1     | test_ma_strategy   | 修复预热逻辑 off-by-one (condition < → <=) | ✅ 完成 |
| P1     | TickDriver测试     | 重写 progress() + 添加 total_klines 跟踪  | ✅ 完成 |
| P1     | test_all_ticks_exhausted | 修复 is_exhausted() 判断条件      | ✅ 完成 |


4.2 后续优化（非阻塞）
----------------------------------------------------------------
| 优先级 | 问题                | 整改动作                               | 预计时间 |
|--------|-------------------|--------------------------------------|---------|
| P2     | Parquet硬编码路径   | 添加 --parquet-path CLI参数            | 15分钟  |
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

5.2 编译和测试状态
----------------------------------------------------------------
  ✅ cargo check --all           → 0 错误
  ✅ h_sandbox --lib             → 0 错误
  ✅ h_sandbox --examples        → 0 错误
  ✅ cargo test -p h_sandbox    → 44通过 / 0失败

5.3 提交记录
----------------------------------------------------------------
  3fb5d6a - 删除 CSV 废弃代码
  4a0cf07 - API K线回放系统完善
  f4b669e - 修复历史遗留测试 + generator 状态计算 bug

5.4 后续建议
----------------------------------------------------------------
  1. Parquet 路径改为命令行参数以提升灵活性
  2. 清理 h_sandbox 中的死代码和未使用字段
  3. 考虑为 noise.rs 添加 rand feature 启用支持
