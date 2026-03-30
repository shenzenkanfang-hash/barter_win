# Phase 4: SharedStore 序列号完善 - Context

**Gathered:** 2026-03-30
**Status:** Ready for execution

<domain>
## Phase Boundary

实现 `SharedStore::get_since()` 增量读取方法，完善序列号机制，支持增量同步场景。
</domain>

<decisions>
## Implementation Decisions

### get_since 方法设计
- `fn get_since(&self, key: &K, min_seq: u64) -> Vec<VersionedData<V>>` — 返回 version >= min_seq 的所有条目
- 返回值按 version 升序排列（历史顺序）
- key 不存在时返回空 Vec（非 None）
- 支持软删除后的增量读取

### 存储结构变更
- `data: HashMap<K, VersionedData<V>>` → `data: HashMap<K, Vec<VersionedData<V>>>`
- 每个 key 保存完整版本历史，支持任意断点续传
- `get()` 只返回最新版本（Vec.last()）
- `write()` 追加而非替换
- `delete()` 追加软删除标记版本

### 向后兼容
- `get()`, `version()`, `contains()`, `keys()`, `len()` 行为不变
- `delete()` 软删除语义不变（追加标记版本）
- `global_version()` 每次写操作递增（不变）
</decisions>

<reusable>
## Reusable Assets
- `crates/b_data_source/src/shared_store.rs`: SharedStore trait + SharedStoreImpl
- `crates/b_data_source/src/lib.rs`: Re-exports
</reusable>
