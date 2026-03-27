# MarketDataStore 统一存储接口设计

## 1. 背景

当前 WS 数据直接写入内存，缺乏统一抽象：
- 难以切换模拟/真实 WS 行为
- 难以测试特定数据场景
- 历史K线存储逻辑分散

## 2. 目标

提供统一的 `MarketDataStore` 接口：
- 统一数据存储，WS 和模拟器共用
- 实时分区 + 历史分区分离
- 内存 + 磁盘同步
- 波动率实时计算

## 3. 核心接口

```rust
pub trait MarketDataStore: Send + Sync {
    // ========== 写入 ==========
    /// 写入K线数据
    /// - 每次调用触发波动率计算
    /// - is_closed=true 时同时写入历史分区
    fn write_kline(&self, symbol: &str, kline: KlineData, is_closed: bool);
    
    /// 写入订单簿
    fn write_orderbook(&self, symbol: &str, orderbook: OrderBookData);
    
    // ========== 查询 ==========
    /// 获取当前K线（实时分区）
    fn get_current_kline(&self, symbol: &str) -> Option<KlineData>;
    
    /// 获取订单簿
    fn get_orderbook(&self, symbol: &str) -> Option<OrderBookData>;
    
    /// 获取历史K线（历史分区）
    fn get_history_klines(&self, symbol: &str) -> Vec<KlineData>;
    
    /// 获取历史订单簿
    fn get_history_orderbooks(&self, symbol: &str) -> Vec<OrderBookData>;
}
```

## 4. 模块结构

```
crates/b_data_source/src/store/
├── mod.rs           # 模块导出
├── trait.rs         # MarketDataStore trait 定义
├── memory_store.rs  # 实时分区（内存）
├── history_store.rs # 历史分区（内存+磁盘同步）
├── volatility.rs    # 波动率计算
└── impl.rs          # 默认实现
```

## 5. 组件设计

### 5.1 MemoryStore（实时分区）

```rust
pub struct MemoryStore {
    klines: RwLock<HashMap<String, KlineData>>,        // symbol → 当前K线
    orderbooks: RwLock<HashMap<String, OrderBookData>>, // symbol → 当前订单簿
}
```

### 5.2 HistoryStore（历史分区）

```rust
pub struct HistoryStore {
    klines: RwLock<HashMap<String, Vec<KlineData>>>,        // symbol → 闭合K线
    orderbooks: RwLock<HashMap<String, Vec<OrderBookData>>>, // symbol → 订单簿
    disk_path: PathBuf,  // 磁盘同步路径
}
```

### 5.3 DefaultImpl（默认实现）

```rust
pub struct MarketDataStoreImpl {
    memory: Arc<MemoryStore>,
    history: Arc<HistoryStore>,
    volatility: Arc<VolatilityManager>,
}
```

## 6. 数据流

```
WS 数据
    │
    ▼
write_kline(symbol, kline, is_closed)
    │
    ├─► 1. memory.klines[symbol] = kline  (实时分区)
    │
    ├─► 2. volatility.update(symbol, kline)  (每次都计算)
    │
    └─► 3. if is_closed {
           history.klines[symbol].push(kline)  (历史分区)
           history.sync_to_disk()  (磁盘同步)
         }

write_orderbook(symbol, book)
    │
    └─► memory.orderbooks[symbol] = book  (实时分区)
```

## 7. 波动率计算

```rust
fn write_kline(&self, symbol: &str, kline: KlineData, is_closed: bool) {
    // 1. 写入实时分区
    self.memory.write_kline(symbol, kline.clone());
    
    // 2. 触发波动率计算（每次都计算，不管是否闭合）
    self.volatility.update(symbol, &kline);
    
    if is_closed {
        // 3. 闭合时写入历史分区
        self.history.append_kline(symbol, kline.clone());
    }
}
```

## 8. 启动恢复

```rust
impl MarketDataStore for MarketDataStoreImpl {
    fn new() -> Self {
        let memory = MemoryStore::new();
        let history = HistoryStore::load_from_disk();
        
        // 从历史分区恢复实时分区最新K线
        for (symbol, klines) in history.get_all() {
            if let Some(last) = klines.last() {
                memory.write_kline(symbol, last.clone());
            }
        }
        
        Self { memory, history, volatility }
    }
}
```

## 9. WS 集成

```rust
// WS 处理回调
fn on_kline(&self, data: KlineData) {
    self.store.write_kline(&data.symbol, data, data.is_closed);
}

fn on_orderbook(&self, data: OrderBookData) {
    self.store.write_orderbook(&data.symbol, data);
}
```

## 10. 模拟器集成

```rust
// 模拟器直接注入测试数据
fn inject_kline(&self, kline: KlineData) {
    self.store.write_kline(&kline.symbol, kline, true);
}
```

## 11. 文件组织

| 文件 | 职责 |
|------|------|
| `trait.rs` | 接口定义 |
| `memory_store.rs` | 实时分区实现 |
| `history_store.rs` | 历史分区实现 + 磁盘同步 |
| `volatility.rs` | 波动率计算 |
| `impl.rs` | DefaultImpl |
| `mod.rs` | 模块导出 |

## 12. 测试策略

- 单元测试：MemoryStore, HistoryStore, VolatilityManager
- 集成测试：MarketDataStoreImpl 完整流程
- 模拟器测试：注入特定数据验证存储
