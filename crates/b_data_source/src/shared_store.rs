//! SharedStore - 带版本号机制的共享数据存储
//!
//! 提供通用共享数据存储接口，支持版本号追踪和数据变更通知。
//!
//! # 与 MarketDataStore 的区别
//! - MarketDataStore: 专注市场数据（K线/订单簿/波动率）
//! - SharedStore: 通用共享数据，可存储任意类型数据，带版本号

use std::sync::Arc;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use serde::{Deserialize, Serialize};
use std::hash::Hash;

/// 版本号追踪器
#[derive(Debug, Clone)]
pub struct StoreVersion {
    /// 当前版本号
    version: u64,
    /// 上次更新时间戳（毫秒）
    last_update_ms: i64,
}

impl StoreVersion {
    pub fn new() -> Self {
        Self {
            version: 0,
            last_update_ms: 0,
        }
    }

    /// 获取当前版本
    pub fn get(&self) -> u64 {
        self.version
    }

    /// 获取上次更新时间戳
    pub fn last_update_ms(&self) -> i64 {
        self.last_update_ms
    }

    /// 增加版本号
    pub fn incr(&mut self, timestamp_ms: i64) -> u64 {
        self.version += 1;
        self.last_update_ms = timestamp_ms;
        self.version
    }
}

impl Default for StoreVersion {
    fn default() -> Self {
        Self::new()
    }
}

/// 版本化数据条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionedData<T> {
    /// 数据内容
    pub data: T,
    /// 版本号
    pub version: u64,
    /// 更新时间戳
    pub timestamp_ms: i64,
}

impl<T> VersionedData<T> {
    pub fn new(data: T, version: u64, timestamp_ms: i64) -> Self {
        Self { data, version, timestamp_ms }
    }
}

/// SharedStore 变更事件
#[derive(Debug, Clone)]
pub struct SharedStoreEvent<T> {
    /// 变更的key
    pub key: String,
    /// 变更后的数据
    pub data: VersionedData<T>,
    /// 变更类型
    pub change_type: ChangeType,
}

/// 变更类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeType {
    /// 新增
    Insert,
    /// 更新
    Update,
    /// 删除
    Delete,
}

/// SharedStore trait - 带版本号的共享数据存储接口
///
/// 提供线程安全的共享数据存储，支持版本追踪和变更通知。
pub trait SharedStore<K, V>: Send + Sync
where
    K: Eq + Hash + Clone + Send + Sync,
    V: Clone + Send + Sync,
{
    /// 获取数据（不存在返回 None）
    fn get(&self, key: &K) -> Option<VersionedData<V>>;

    /// 获取数据版本号（不存在返回 0）
    fn version(&self, key: &K) -> u64;

    /// 检查 key 是否存在
    fn contains(&self, key: &K) -> bool;

    /// 写入数据（自动增加版本号）
    fn write(&self, key: K, value: V, timestamp_ms: i64);

    /// 删除数据
    fn delete(&self, key: &K, timestamp_ms: i64);

    /// 获取所有 keys
    fn keys(&self) -> Vec<K>;

    /// 获取数据总数
    fn len(&self) -> usize;

    /// 是否为空
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 获取全局版本号（所有 key 的最大版本号）
    fn global_version(&self) -> u64;

    /// 检查指定 key 是否为最新（版本号 >= 指定版本）
    fn is_current(&self, key: &K, min_version: u64) -> bool;
}

/// SharedStoreImpl - SharedStore 默认实现
///
/// 使用 HashMap 存储数据，支持线程安全访问。
#[derive(Debug)]
pub struct SharedStoreImpl<K, V>
where
    K: Eq + Hash + Clone + Send + Sync,
    V: Clone + Send + Sync,
{
    /// 数据存储
    data: RwLock<HashMap<K, VersionedData<V>>>,
    /// 全局版本号
    global_version: AtomicU64,
    /// 各 key 的版本号
    key_versions: RwLock<HashMap<K, u64>>,
}

impl<K, V> SharedStoreImpl<K, V>
where
    K: Eq + Hash + Clone + Send + Sync,
    V: Clone + Send + Sync,
{
    pub fn new() -> Self {
        Self {
            data: RwLock::new(HashMap::new()),
            global_version: AtomicU64::new(0),
            key_versions: RwLock::new(HashMap::new()),
        }
    }
}

impl<K, V> Default for SharedStoreImpl<K, V>
where
    K: Eq + Hash + Clone + Send + Sync,
    V: Clone + Send + Sync,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V> SharedStore<K, V> for SharedStoreImpl<K, V>
where
    K: Eq + Hash + Clone + Send + Sync,
    V: Clone + Send + Sync,
{
    fn get(&self, key: &K) -> Option<VersionedData<V>> {
        self.data.read().get(key).cloned()
    }

    fn version(&self, key: &K) -> u64 {
        self.key_versions.read().get(key).copied().unwrap_or(0)
    }

    fn contains(&self, key: &K) -> bool {
        self.data.read().contains_key(key)
    }

    fn write(&self, key: K, value: V, timestamp_ms: i64) {
        let mut data = self.data.write();
        let mut key_versions = self.key_versions.write();

        let new_version = key_versions.get(&key).copied().unwrap_or(0) + 1;

        data.insert(key.clone(), VersionedData::new(value, new_version, timestamp_ms));
        key_versions.insert(key, new_version);

        // 全局版本号：每次写入操作递增
        self.global_version.fetch_add(1, Ordering::SeqCst);
    }

    fn delete(&self, key: &K, timestamp_ms: i64) {
        let mut data = self.data.write();
        let mut key_versions = self.key_versions.write();

        // 先获取现有数据（克隆一份）
        let existing_data = data.get(key).cloned();

        if let Some(existing) = existing_data {
            // 软删除：保留数据但标记版本
            let delete_version = existing.version + 1;
            data.insert(key.clone(), VersionedData::new(
                existing.data,
                delete_version,
                timestamp_ms,
            ));
            key_versions.insert(key.clone(), delete_version);
        }
    }

    fn keys(&self) -> Vec<K> {
        self.data.read().keys().cloned().collect()
    }

    fn len(&self) -> usize {
        self.data.read().len()
    }

    fn global_version(&self) -> u64 {
        self.global_version.load(Ordering::SeqCst)
    }

    fn is_current(&self, key: &K, min_version: u64) -> bool {
        self.version(key) >= min_version
    }
}

/// Arc 包装的 SharedStore 方便共享引用
pub type SharedStoreRef<K, V> = Arc<dyn SharedStore<K, V>>;

/// 创建新的 SharedStore 实例
pub fn create_shared_store<K, V>() -> Arc<SharedStoreImpl<K, V>>
where
    K: Eq + Hash + Clone + Send + Sync,
    V: Clone + Send + Sync,
{
    Arc::new(SharedStoreImpl::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_store_version() {
        let mut ver = StoreVersion::new();
        assert_eq!(ver.get(), 0);

        let v1 = ver.incr(1000);
        assert_eq!(v1, 1);
        assert_eq!(ver.last_update_ms(), 1000);

        let v2 = ver.incr(2000);
        assert_eq!(v2, 2);
        assert_eq!(ver.last_update_ms(), 2000);
    }

    #[test]
    fn test_shared_store_basic() {
        let store: Arc<SharedStoreImpl<String, i32>> = create_shared_store();

        // 写入
        store.write("a".to_string(), 100, 1000);
        assert_eq!(store.get(&"a".to_string()).unwrap().data, 100);
        assert_eq!(store.version(&"a".to_string()), 1);

        // 更新
        store.write("a".to_string(), 200, 2000);
        assert_eq!(store.get(&"a".to_string()).unwrap().data, 200);
        assert_eq!(store.version(&"a".to_string()), 2);

        // 全局版本
        assert_eq!(store.global_version(), 2);

        // 多次更新版本递增
        store.write("b".to_string(), 300, 3000);
        assert_eq!(store.global_version(), 3);
    }

    #[test]
    fn test_shared_store_is_current() {
        let store: Arc<SharedStoreImpl<String, i32>> = create_shared_store();

        store.write("a".to_string(), 100, 1000);

        assert!(store.is_current(&"a".to_string(), 1));
        assert!(!store.is_current(&"a".to_string(), 2));
        assert!(!store.is_current(&"b".to_string(), 1)); // 不存在的 key
    }

    #[test]
    fn test_shared_store_delete() {
        let store: Arc<SharedStoreImpl<String, i32>> = create_shared_store();

        store.write("a".to_string(), 100, 1000);
        assert!(store.contains(&"a".to_string()));

        store.delete(&"a".to_string(), 2000);

        // 删除后版本仍存在
        assert!(store.contains(&"a".to_string()));
        assert!(store.version(&"a".to_string()) > 1);
    }
}
