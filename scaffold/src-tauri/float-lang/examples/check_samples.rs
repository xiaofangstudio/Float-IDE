// 把所有示例脚本过一遍静态检查（编译到字节码），确认注释迁移没写坏
use std::path::Path;

fn main() {
    let dir = Path::new("../../../language/samples");
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .expect("读 samples 目录")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "co" || x == "cw").unwrap_or(false))
        .collect();
    files.sort();
    let mut bad = 0;
    for f in &files {
        let src = std::fs::read_to_string(f).unwrap();
        let name = f.file_name().unwrap().to_string_lossy().to_string();
        match float_lang::api::compile_source(&name, &src, Some(dir)) {
            Ok(_) => {}
            Err(e) => {
                bad += 1;
                println!("✗ {} 第 {} 行：{}", name, e.span.line, e.message);
            }
        }
    }
    println!("检查 {} 个示例，失败 {}", files.len(), bad);
}
