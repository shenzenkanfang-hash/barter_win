//! 交易规则查看器
//!
//! 支持目录或单个文件，每0.5秒全量打印

use std::fs;
use std::path::Path;
use std::time::Duration;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let target = if args.len() > 1 { &args[1] } else { "E:/shm/backup/symbols_rules" };

    let path = Path::new(target);
    let is_file = path.is_file();

    if is_file {
        println!("交易规则查看器 (单文件模式)");
        println!("文件: {}", target);
        println!("按 Ctrl+C 退出\n");
    } else {
        println!("交易规则查看器 (目录模式)");
        println!("目录: {}", target);
        println!("按 Ctrl+C 退出\n");
    }

    if !path.exists() {
        eprintln!("路径不存在: {}", target);
        return;
    }

    loop {
        if is_file {
            let content = fs::read_to_string(path).unwrap_or_default();
            println!("=== {} ===\n{}", path.file_name().unwrap().to_string_lossy(), content);
        } else {
            let mut files: Vec<_> = match fs::read_dir(path) {
                Ok(e) => e.filter_map(|e| e.ok())
                    .filter(|e| e.path().extension().map_or(false, |ext| ext == "json"))
                    .collect(),
                Err(_) => {
                    std::thread::sleep(Duration::from_millis(500));
                    continue;
                }
            };
            files.sort_by_key(|e| e.file_name());

            for entry in &files {
                let filename = entry.file_name();
                let content = fs::read_to_string(entry.path()).unwrap_or_default();
                println!("=== {} ===\n{}", filename.to_string_lossy(), content);
            }
            println!("\n--- {} 个文件 ---\n", files.len());
        }

        std::thread::sleep(Duration::from_millis(500));
    }
}
