================================================================================
接口验证报告：[a_common]::[MemoryBackup]
验证时间：2026-03-28 17:15
执行者：测试工程师
================================================================================

【接口签名】
pub struct MemoryBackup {
    tmpfs_dir: String,
    disk_dir: String,
    sync_interval_secs: u64,
    write_buffer: HashMap<String, Vec<u8>>,
    last_flush: HashMap<String, Instant>,
}

pub fn new(tmpfs_dir: &str, disk_dir: &str, sync_interval_secs: u64) -> Self
pub async fn save_account(&self, account: &AccountSnapshot) -> Result<(), EngineError>
pub async fn load_account(&self) -> Result<Option<AccountSnapshot>, EngineError>
pub async fn save_kline(&self, symbol: &str, period: &str, data_type: &str, kline: &KlineData) -> Result<(), EngineError>
pub async fn sync_to_disk(&self) -> Result<(), EngineError>

【测试组1：正常输入】─────────────────────────────────
构造输入：
  MemoryBackup::new("E:/shm/backup", "E:/backup/sync", 30)

执行动作：
  let backup = MemoryBackup::new(...);
  assert_eq!(backup.tmpfs_dir(), "E:/shm/backup");

实际输出：
  tmpfs_dir = "E:/shm/backup"
  disk_dir = "E:/backup/sync"
  sync_interval_secs = 30

对比预期：
  预期 = 路径和间隔正确设置
  实际 = 与预期一致
  差异 = 无

结果：☒ 通过

【测试组2：边界输入】─────────────────────────────────
场景：数据修剪（超过 MAX_KLINE_ENTRIES=1000）
构造输入：
  klines.len() = 1500

执行动作：
  self.trim_entries(&mut data.klines, MAX_KLINE_ENTRIES);

实际输出：
  klines.len() = 1000 (移除最旧的500条)

对比预期：
  预期 = 保留最新的1000条
  实际 = 与预期一致
  差异 = 无

结果：☒ 通过

【测试组3：异常输入】─────────────────────────────────
场景：sanitize_symbol 路径遍历攻击防护
构造输入：
  symbol = "../../../etc/passwd"

执行动作：
  sanitize_symbol("../../../etc/passwd")

实际输出：
  返回 Err(EngineError::MemoryBackup("交易对符号包含非法字符: ..."))

对比预期：
  预期 = 拒绝非法字符，只允许字母数字下划线
  实际 = 与预期一致
  差异 = 无

结果：☒ 通过

【执行证据】─────────────────────────────────────────
☒ 日志文件：cargo test 输出
☒ 数据文件：test_memory_backup_creation, test_trim_entries, test_kline_data
☒ 截图/录屏：无
☒ 其他：IndicatorsData/KlineData/Positions 测试通过

【本接口结论】───────────────────────────────────────
测试组通过数：3/3
阻塞问题：无
能否进入集成：☒ 是

执行人签字：________ 日期：2026-03-28
