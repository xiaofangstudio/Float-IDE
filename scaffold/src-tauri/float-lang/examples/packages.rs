//! 脚本包机制演示 / 命令行：
//!
//!   packages demo                     自走一遍：装 1.0.0 → 装 1.1.0 → 切回 1.0.0
//!   packages scan   <工程根>
//!   packages install <工程根> <包目录>
//!   packages switch <工程根> <名字> <版本>

use float_lang::packages;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() || args[0] == "demo" {
        demo();
        return;
    }
    match args[0].as_str() {
        "scan" => {
            let root = PathBuf::from(&args[1]);
            match packages::scan_packages(&root) {
                Ok(list) => print_list(&list),
                Err(e) => eprintln!("扫描失败：{}", e),
            }
        }
        "install" => {
            let root = PathBuf::from(&args[1]);
            let src = PathBuf::from(&args[2]);
            match packages::install_package(&root, &src) {
                Ok(p) => println!("已装并启用：{}@{}（{}）", p.name, p.version, p.entry_path().display()),
                Err(e) => eprintln!("装不上：{}", e),
            }
        }
        "switch" => {
            let root = PathBuf::from(&args[1]);
            match packages::switch_to(&root, &args[2], &args[3]) {
                Ok(()) => println!("已切换到 {}@{}", args[2], args[3]),
                Err(e) => eprintln!("切换失败：{}", e),
            }
        }
        other => eprintln!("不认识的命令：{}", other),
    }
}

fn print_list(list: &[packages::PackageInfo]) {
    if list.is_empty() {
        println!("（没有检测到脚本包）");
        return;
    }
    for p in list {
        println!(
            "{} {}@{}{}  ← {}",
            if p.active { "●" } else { "○" },
            p.name,
            p.version,
            if p.title.is_empty() { String::new() } else { format!("  「{}」", p.title) },
            p.entry_path().display()
        );
    }
}

fn write_pkg(dir: &Path, name: &str, version: &str, body: &str) {
    fs::create_dir_all(dir).unwrap();
    fs::write(
        dir.join("manifest.json"),
        format!(
            "{{\n  \"name\": \"{}\",\n  \"version\": \"{}\",\n  \"title\": \"示例脚本包\",\n  \"entry\": \"main.cw\"\n}}\n",
            name, version
        ),
    )
    .unwrap();
    fs::write(dir.join("main.cw"), body).unwrap();
}

fn demo() {
    let base = env::temp_dir().join(format!("float-pkg-demo-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let project = base.join("我的工程");
    fs::create_dir_all(&project).unwrap();

    // 模拟：用户把一包脚本拖进工程根目录
    let drop1 = base.join("drop").join("float-games-1.0.0");
    write_pkg(&drop1, "float-games", "1.0.0", "print(\"v1 的游戏\")\n");
    println!("① 拖进一包：{}", drop1.display());
    let p = packages::install_package(&project, &drop1).unwrap();
    println!("   检测到并装好：{}@{} → {}", p.name, p.version, p.entry_path().display());

    // 又拖进来一版新的
    let drop2 = base.join("drop").join("float-games-1.1.0");
    write_pkg(&drop2, "float-games", "1.1.0", "print(\"v2 的游戏（新版）\")\n");
    println!("\n② 再拖进新版：{}", drop2.display());
    let p = packages::install_package(&project, &drop2).unwrap();
    println!("   自动切到新版：{}@{}", p.name, p.version);

    println!("\n③ 现在工程里的包：");
    print_list(&packages::scan_packages(&project).unwrap());

    // 后悔了，切回旧版
    packages::switch_to(&project, "float-games", "1.0.0").unwrap();
    println!("\n④ 一键切回 1.0.0：");
    print_list(&packages::scan_packages(&project).unwrap());

    let active = packages::active_package(&project).unwrap();
    let body = fs::read_to_string(active.entry_path()).unwrap();
    println!("\n⑤ 当前启用版本的脚本内容：{}", body.trim());
    println!("\n（演示目录：{}）", project.display());
}
