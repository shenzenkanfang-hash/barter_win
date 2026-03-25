沙盒分片缓存架构设计
==============================================
Author: 软件架构师
Created: 2026-03-26
Status: pending-review
Project: barter-rs 量化交易系统
==============================================

一、背景与目标
==============================================

在 V1.1 API 直连 K线回放的基础上，新增本地分片缓存机制：

1. 首次运行：API 拉取 → 实时分片写入磁盘
2. 二次运行：直接加载本地分片 → 零 API 调用
3. 保留 API 直连兜底能力（数据缺失或强制刷新）

二、缓存目录规范
==============================================

缓存根目录（Windows 固定路径）：

    D:/sandbox_cache/{symbol}/{interval}/part_{起始时间戳_ms}.csv

示例：

    D:/sandbox_cache/BTCUSDT/1m/part_1704067200000.csv
    D:/sandbox_cache/BTCUSDT/1m/part_1706745600000.csv
    D:/sandbox_cache/ETHUSDT/1m/part_1704067200000.csv

每分片文件包含 50000 条 K线数据。

三、CSV 文件格式
==============================================

列定义（与 b_data_source::KLine 对齐）：

    symbol,period,open,high,low,close,volume,timestamp
    BTCUSDT,1m,50000,50100,49900,50050,100,1704067200000
    BTCUSDT,1m,50050,50200,50000,50150,120,1704067260000

说明：
- timestamp: 毫秒级 Unix 时间戳
- period: K线周期（1m / 5m / 15m / 1h / 1d）
- 无表头单独文件，每行一条 K线
- 文件名编码起始时间戳，无需额外索引文件

四、运行逻辑（优先级）
==============================================

┌─────────────────────────────────────────────────┐
│  ① 扫描本地分片 CSV                             │
│      │                                          │
│      ├─ 存在且覆盖完整时间范围                   │
│      │         ↓                                │
│      │  流式读取（不占内存）                    │
│      │                                          │
│      └─ 不存在 / 数据缺失                       │
│                ↓                                 │
│  ② 币安 API 流式拉取 K线                       │
│                ↓                                 │
│  ③ 实时分片写入磁盘（50000条/片）               │
│                ↓                                 │
│  ④ StreamTickGenerator 消费                    │
└─────────────────────────────────────────────────┘

五、核心模块
==============================================

5.1 ShardCache 分片缓存管理器
----------------------------------------------------------------
位置: crates/h_sandbox/src/shard_cache.rs（新增）

职责：
- 分片扫描（按时间范围查找本地文件）
- 分片流式读取（迭代器，不占内存）
- 分片写入（追加模式，写满自动封片）

接口设计：

    pub struct ShardCache {
        cache_root: PathBuf,
        shard_size: usize,  // 默认 50000
    }

    impl ShardCache {
        /// 扫描指定时间范围内的本地分片（按时间排序）
        pub fn find_shards(
            &self,
            symbol: &str,
            interval: &str,
            start_ms: i64,
            end_ms: i64,
        ) -> Vec<ShardFile>;

        /// 流式读取指定分片文件（返回 KLine 迭代器）
        pub fn read_shard(
            &self,
            path: &Path,
        ) -> ShardReader;

        /// 创建新分片写入器（追加模式）
        pub fn write_shard(
            &self,
            symbol: &str,
            interval: &str,
            start_ms: i64,
        ) -> ShardWriter;

        /// 获取缓存根目录
        pub fn cache_root(&self) -> &Path;
    }

    /// 分片文件元数据
    pub struct ShardFile {
        pub path: PathBuf,
        pub start_ms: i64,  // 从文件名解析
        pub end_ms: i64,    // start_ms + shard_size * 60000
    }

5.2 ShardReader 流式读取器
----------------------------------------------------------------
职责：逐行读取 CSV 分片，返回 KLine 迭代器

    pub struct ShardReader { /* 内部缓冲 */ }

    impl Iterator for ShardReader {
        type Item = Result<KLine, ShardReadError>;
    }

特点：
- 逐行读取，不一次性加载整个文件
- 每批读取 N 行（可配置，默认 1000）
- 自动处理 CSV 解析错误（跳过坏行）

5.3 ShardWriter 分片写入器
----------------------------------------------------------------
职责：接收 K线数据，按分片大小写入文件

    pub struct ShardWriter {
        current_path: PathBuf,
        count: usize,
        shard_size: usize,
    }

    impl ShardWriter {
        /// 写入一条 K线
        pub fn write(&mut self, kline: &KLine) -> Result<(), ShardWriteError>;

        /// 强制封片（通常由内部自动调用）
        pub fn finish(self) -> Result<ShardFile, ShardWriteError>;
    }

自动封片规则：
- 写入第 50001 条时，自动 finish 当前分片，创建新分片
- 文件名使用第一条 K线的 timestamp 作为起始时间戳

5.4 历史回放入口改造
----------------------------------------------------------------
改造文件: crates/h_sandbox/src/historical_replay/replay_controller.rs

改造内容：
- run_with_klines() 保持不变（接收 Vec<KLine>）
- 新增 run_with_cache() 方法，集成缓存逻辑

    /// 运行回放（缓存优先）
    pub fn run_with_cache(
        &mut self,
        symbol: &str,
        interval: &str,
        start_ms: i64,
        end_ms: i64,
    ) -> Result<(), ReplayError> {
        // ① 尝试本地分片
        let shards = self.shard_cache.find_shards(symbol, interval, start_ms, end_ms)?;
        if shards覆盖完整 {
            // ② 流式读取本地分片
            let klines = self.shard_cache.stream_klines(shards);
            return self.run_with_klines(klines, symbol);
        }
        // ③ API 拉取 + 写入缓存
        let writer = self.shard_cache.write_shard(symbol, interval, start_ms);
        let klines = self.api_fetcher.fetch_stream(start_ms, end_ms, |kline| {
            writer.write(&kline)?;
            Ok(kline)
        })?;
        self.run_with_klines(klines, symbol)
    }

六、Example 改造
==============================================

改造文件: crates/h_sandbox/examples/kline_replay.rs

6.1 CLI 参数新增
----------------------------------------------------------------
    #[derive(Parser)]
    struct Args {
        // ... 现有参数 ...
        /// 禁用本地缓存，强制 API 直连
        #[arg(long, default_value = "false")]
        no_cache: bool,

        /// 缓存根目录（默认 D:/sandbox_cache）
        #[arg(long)]
        cache_dir: Option<PathBuf>,
    }

6.2 缓存优先加载逻辑
----------------------------------------------------------------
    let cache_root = args.cache_dir.unwrap_or_else(|| PathBuf::from("D:/sandbox_cache"));
    let cache = ShardCache::new(cache_root);

    let symbol = &args.symbol;
    let start_ms = /* 解析 */;
    let end_ms = /* 解析 */;

    if args.no_cache {
        // 强制 API 直连
        let klines = fetcher.fetch_all().await?;
        run_replay(klines);
    } else {
        // 缓存优先
        let shards = cache.find_shards(symbol, "1m", start_ms, end_ms)?;
        if !shards.is_empty() {
            info!("使用本地缓存: {} 个分片", shards.len());
            let klines = cache.stream_klines(shards);
            run_replay(klines);
        } else {
            info!("本地缓存未命中，拉取 API...");
            let klines = fetcher.fetch_all().await?;
            run_replay(klines);
        }
    }

七、技术细节
==============================================

7.1 CSV 读写库
----------------------------------------------------------------
使用 rust-csv crate（workspace 已声明）：

    csv = "1.1"  # workspace 已有

h_sandbox 的 Cargo.toml 需添加：

    csv = { workspace = true }

7.2 时间范围覆盖判断
----------------------------------------------------------------
分片覆盖条件：

    shard.start_ms <= requested_start_ms
    && shard.end_ms >= requested_end_ms

若多个分片中间有空洞（Gap），视为不完整，触发 API 补数。

7.3 并发安全
----------------------------------------------------------------
- ShardWriter：单线程独占写入，不涉及并发
- ShardCache.find_shards()：只读操作，无锁
- ShardCache 与 ApiKlineFetcher 可并行初始化

八、测试策略
==============================================

| 测试 | 验证内容 |
|------|----------|
| test_shard_write_50000_auto_flush | 写入 50001 条 → 2 个分片文件 |
| test_shard_scan_time_range | 时间范围查找正确，无遗漏 |
| test_shard_reader_streaming | 逐行读取，不占用大量内存 |
| test_cache_hit_miss | 首次 miss，二次 hit |
| test_cache_gap_fallback | 中间有空洞时触发 API 补数 |

九、待确认事项
==============================================

无。所有细节已确认。

十、交付物
==============================================

| 文件 | 说明 |
|------|------|
| crates/h_sandbox/src/shard_cache.rs | 新增分片缓存管理器 |
| crates/h_sandbox/src/config.rs | CacheConfig 配置 |
| crates/h_sandbox/examples/kline_replay.rs | 改造：集成缓存逻辑 |
| docs/superpowers/specs/2026-03-26-sandbox-shard-cache-design.md | 本文档 |
