//! HistoryStore - 历史分区实现
//!
//! 内存 + 磁盘同步存储已闭合K线和订单簿。

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use parking_lot::RwLock;

use crate::ws::kline_1m::ws::KlineData;
use super::store_trait::OrderBookData;

/// 历史分区：已闭合K线和订单簿
pub struct HistoryStore {
    klines: RwLock<HashMap<String, Vec<KlineData>>>,
    orderbooks: RwLock<HashMap<String, Vec<OrderBookData>>>,
    disk_path: PathBuf,
}

impl HistoryStore {
    pub fn new(disk_path: PathBuf) -> Self {
        // 确保目录存在
        if let Some(parent) = disk_path.parent() {
            fs::create_dir_all(parent).ok();
        }
        
        Self {
            klines: RwLock::new(HashMap::new()),
            orderbooks: RwLock::new(HashMap::new()),
            disk_path,
        }
    }

    /// 从磁盘加载历史数据
    pub fn load_from_disk(&self) {
        let kline_file = self.disk_path.join("klines.jsonl");
        if kline_file.exists() {
            if let Ok(file) = File::open(&kline_file) {
                let reader = BufReader::new(file);
                for line in reader.lines().flatten() {
                    if let Ok(kline) = serde_json::from_str::<KlineData>(&line) {
                        let symbol = kline.symbol.clone();
                        self.klines.write()
                            .entry(symbol.to_lowercase())
                            .or_insert_with(Vec::new)
                            .push(kline);
                    }
                }
                let count: usize = self.klines.read().values()
                    .map(|v: &Vec<KlineData>| v.len())
                    .sum();
                tracing::info!("从磁盘加载了 {} 条K线历史", count);
            }
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

    /// 追加订单簿到历史分区
    pub fn append_orderbook(&self, symbol: &str, orderbook: OrderBookData) {
        let symbol_lower = symbol.to_lowercase();
        self.orderbooks.write()
            .entry(symbol_lower.clone())
            .or_insert_with(Vec::new)
            .push(orderbook.clone());
        
        // 同步到磁盘
        self.sync_orderbook_to_disk(&symbol_lower, &orderbook);
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

    fn sync_orderbook_to_disk(&self, symbol: &str, orderbook: &OrderBookData) {
        let ob_file = self.disk_path.join(format!("{}_orderbooks.jsonl", symbol));
        
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&ob_file)
        {
            if let Ok(json) = serde_json::to_string(orderbook) {
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

    pub fn get_orderbooks(&self, symbol: &str) -> Vec<OrderBookData> {
        let symbol_lower = symbol.to_lowercase();
        self.orderbooks.read()
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
