//! HistoryStore - 历史分区实现
//!
//! 内存 + 磁盘同步存储已闭合K线和订单簿

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::fs::OpenOptions;
use parking_lot::RwLock;

use crate::ws::kline_1m::KlineData;
use super::store_trait::OrderBookData;

/// 历史分区：已闭合K线和订单簿
pub struct HistoryStore {
    klines: RwLock<HashMap<String, Vec<KlineData>>>,
    orderbooks: RwLock<HashMap<String, Vec<OrderBookData>>>,
    disk_path: PathBuf,
}

impl HistoryStore {
    pub fn new(disk_path: PathBuf) -> Self {
        if let Some(parent) = disk_path.parent() {
            fs::create_dir_all(parent).ok();
        }

        Self {
            klines: RwLock::new(HashMap::new()),
            orderbooks: RwLock::new(HashMap::new()),
            disk_path,
        }
    }

    /// 追加K线到历史分区
    pub fn append_kline(&self, symbol: &str, kline: KlineData) {
        let symbol_lower = symbol.to_lowercase();
        self.klines.write()
            .entry(symbol_lower.clone())
            .or_insert_with(Vec::new)
            .push(kline.clone());

        // 同步到磁盘
        self.sync_kline_to_disk(&symbol_lower, &kline);
    }

    fn sync_kline_to_disk(&self, symbol: &str, kline: &KlineData) {
        let kline_file = self.disk_path.join(format!("{}_klines.jsonl", symbol));

        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&kline_file)
        {
            if let Ok(json) = serde_json::to_string(kline) {
                let _ = writeln!(file, "{}", json);
                let _ = file.flush();
            }
        }
    }

    pub fn get_klines(&self, symbol: &str) -> Vec<KlineData> {
        let symbol_lower = symbol.to_lowercase();
        self.klines.read()
            .get(&symbol_lower)
            .cloned()
            .unwrap_or_default()
    }

    pub fn get_all(&self) -> HashMap<String, Vec<KlineData>> {
        self.klines.read().clone()
    }

    pub fn clear(&self) {
        self.klines.write().clear();
        self.orderbooks.write().clear();
    }
}
