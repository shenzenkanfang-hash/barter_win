//! 交易规则查看器
//!
//! 循环读取 symbols_rules/ 目录下的文件，每0.5秒打印变化

use std::fs;
use std::path::Path;
use std::time::Duration;
use std::collections::HashMap;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let base_dir = if args.len() > 1 {
        &args[1]
    } else {
        "E:/shm/backup/symbols_rules"
    };

    println!("交易规则查看器");
    println!("目录: {}", base_dir);
    println!("每 0.5 秒刷新一次，按 Ctrl+C 退出\n");

    let path = Path::new(base_dir);
    if !path.exists() {
        eprintln!("目录不存在: {}", base_dir);
        return;
    }

    let mut prev_content: HashMap<String, String> = HashMap::new();
    let mut count = 0;

    loop {
        // 读取所有文件
        let entries = match fs::read_dir(path) {
            Ok(e) => e.filter_map(|e| e.ok()).collect::<Vec<_>>(),
            Err(_) => {
                std::thread::sleep(Duration::from_millis(500));
                continue;
            }
        };

        let mut has_changes = false;

        for entry in entries {
            let filename = entry.file_name().to_string_lossy().to_string();
            let content = fs::read_to_string(entry.path()).unwrap_or_default();

            let is_new = !prev_content.contains_key(&filename);
            let changed = prev_content.get(&filename).map_or(true, |c| c != &content);

            if is_new || changed {
                has_changes = true;
                prev_content.insert(filename.clone(), content.clone());

                if is_new {
                    println!("[NEW] {}", filename);
                } else {
                    println!("[UPD] {}", filename);
                }
                println!("{}", content);
            }
        }

        if has_changes {
            count += 1;
            println!("--- 第 {} 轮刷新 ---", count);
        }

        std::thread::sleep(Duration::from_millis(500));
    }
}
