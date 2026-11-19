// 集成测试：编译器命名与"取最新版本"的规则（不依赖 tauri 运行时）。
use floatide_lib::commands::copper_env::{display_name, is_compiler_file, parse_version, scan_compilers};

#[test]
fn version_from_name() {
    assert_eq!(parse_version("colang 1.9.2.exe"), Some((1, 9, 2)));
    assert_eq!(parse_version("colang-self 1.9.2.exe"), Some((1, 9, 2)));
    assert_eq!(parse_version("colang-2.0.0"), Some((2, 0, 0)));
    assert_eq!(parse_version("colang 1.9.002.exe"), Some((1, 9, 2)));
    assert_eq!(parse_version("copper.exe"), None);
}

#[test]
fn display_name_format() {
    assert_eq!(display_name("1.9.2"), "colang 1.9.2.exe");
    assert!(display_name("").starts_with("colang"));
}

#[test]
fn runners_are_not_compilers() {
    assert!(is_compiler_file("colang 1.9.2.exe"));
    assert!(is_compiler_file("colang-self 1.9.2.exe"));
    assert!(!is_compiler_file("copper-run.exe"));
    assert!(!is_compiler_file("colang-embed.exe"));
}

#[test]
fn scan_prefers_newest() {
    let home = std::env::temp_dir().join(format!("copper_scan_{}", std::process::id()));
    let bin = home.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    for n in ["colang 1.9.1.exe", "colang 1.9.2.exe", "colang 1.10.0.exe", "copper-run.exe"] {
        std::fs::write(bin.join(n), b"x").unwrap();
    }
    let list = scan_compilers(&home, "windows-x64");
    assert_eq!(list[0].name, "colang 1.10.0.exe", "应当优先最新版本");
    assert_eq!(list[0].version, "1.10.0");
    let _ = std::fs::remove_dir_all(&home);
}


#[test]
fn scan_real_installed_env() {
    let Some(home) = std::env::var("COPPER_TEST_HOME").ok() else { return };
    let home = std::path::PathBuf::from(home);
    let list = scan_compilers(&home, "windows-x64");
    for c in &list {
        println!("{}  v{}  self={} {}B  {}", c.name, c.version, c.self_hosted, c.bytes, c.path.display());
    }
    assert!(!list.is_empty(), "安装目录里应当扫到编译器");
    assert!(list[0].name.contains("colang"), "第一个候选应是 colang 开头，实际 {}", list[0].name);
    assert!(!list[0].self_hosted, "非自举版应排在前面");
}
