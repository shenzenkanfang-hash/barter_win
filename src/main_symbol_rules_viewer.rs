//! 交易规则查看器
//!
//! 支持目录或单个文件，一次性打印所有内容

use std::fs;
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let target = if args.len() > 1 { &args[1] } else { "E:/shm/backup/symbols_rules" };

    let path = Path::new(target);
    let is_file = path.is_file();

    if !path.exists() {
        eprintln!("路径不存在: {}", target);
        return;
    }

    if is_file {
        let content = fs::read_to_string(path).unwrap_or_default();
        println!("{}", content);
    } else {
        let mut files: Vec<_> = match fs::read_dir(path) {
            Ok(e) => e.filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map_or(false, |ext| ext == "json"))
                .collect(),
            Err(e) => {
                eprintln!("读取目录失败: {}", e);
                return;
            }
        };
        files.sort_by_key(|e| e.file_name());

        for entry in &files {
            let content = fs::read_to_string(entry.path()).unwrap_or_default();
            println!("{}", content);
        }
    }
}
