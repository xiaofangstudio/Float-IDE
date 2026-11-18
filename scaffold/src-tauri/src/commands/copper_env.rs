//! Copper 环境（toolchain environment）接入。
//!
//! 第 3 步的核心：IDE 不再"自带一套编译"，而是**读环境**——
//! 环境里放着一个已经编好的 Copper 编译器（`copper.exe`，由 `bootstrap/colang.co`
//! AOT 出来的那个）＋自举版（`copper-self.exe`）＋运行时前奏 `colang_rt.rs`。
//!
//! 解析顺序（第一个找到的赢）：
//!   1. 环境变量 `COPPER_HOME`（显式指定，最优先）
//!   2. 用户级安装位置：Windows `%APPDATA%\Float\copper`，macOS/Linux `~/.copper`
//!   3. 随 IDE 分发的绿色版目录：`<exe 所在目录>/copper_env`
//!
//! 环境不存在时返回 `found = false`，前端据此提示"先装 Copper 环境"，
//! 而**不是**静默退回内置实现（静默降级会让人以为在用环境，实际没在）。

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Serialize, Clone, Default)]
pub struct CopperEnvInfo {
    pub found: bool,
    /// 环境根目录
    pub home: String,
    /// 主编译器（AOT 出来的 colang.exe）
    pub compiler: String,
    /// 自举编译器（colang2.exe，Compalng 编 Compalng 的产物）
    pub compiler_self: String,
    /// 运行时前奏（发射代码要拼在最前面）
    pub runtime: String,
    /// 平台标识，例如 windows-x64 / macos-arm64 / linux-x64
    pub platform: String,
    /// 环境来源：env / user / bundled
    pub source: String,
    /// 环境里记录的版本号（copper.json 里写的）
    pub version: String,
    /// 环境里备好了二进制的平台列表（bin/ 下的子目录，如 windows-x64 / linux-x64）
    pub platforms: Vec<String>,
}

fn exe_suffix() -> &'static str {
    if cfg!(windows) {
        ".exe"
    } else {
        ""
    }
}

/// 当前平台标识。自举/跨平台铺垫都靠它找对应二进制。
pub fn platform_tag() -> String {
    let os = if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    };
    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x64"
    };
    format!("{}-{}", os, arch)
}

fn home_candidates() -> Vec<(PathBuf, &'static str)> {
    let mut out = Vec::new();
    if let Ok(p) = std::env::var("COPPER_HOME") {
        if !p.trim().is_empty() {
            out.push((PathBuf::from(p), "env"));
        }
    }
    // 用户级
    #[cfg(windows)]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            out.push((PathBuf::from(appdata).join("Float").join("copper"), "user"));
        }
    }
    #[cfg(not(windows))]
    {
        if let Ok(home) = std::env::var("HOME") {
            out.push((PathBuf::from(home).join(".copper"), "user"));
        }
    }
    // 绿色版：跟着 IDE 走
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            out.push((dir.join("copper_env"), "bundled"));
            // 开发时 exe 在 target/debug 或 target/release：往上找仓库根的 copper_env
            for up in 1..=4 {
                let mut anc = dir.to_path_buf();
                for _ in 0..up {
                    if !anc.pop() {
                        break;
                    }
                }
                let cand = anc.join("copper_env");
                if cand.join("copper.json").is_file() {
                    out.push((cand, "bundled"));
                    break;
                }
            }
        }
    }
    out
}

/// 一个目录是不是像一个 Copper 环境：至少要有 copper.json。
fn looks_like_env(dir: &Path) -> bool {
    dir.join("copper.json").is_file()
        || dir.join("bin").join(format!("copper{}", exe_suffix())).is_file()
        || compiler_in(dir, &platform_tag()).is_file()
}

/// 环境里的编译器路径。两种摆法都认，顺序如下：
///   1. 多平台布局：`bin/<platform>/copper[.exe]`（新，跨平台用这个）
///   2. 平铺布局：  `bin/copper[.exe]`（老环境照旧能用，不砸已有的安装）
fn compiler_in(home: &Path, platform: &str) -> PathBuf {
    let plat = home
        .join("bin")
        .join(platform)
        .join(format!("copper{}", exe_suffix()));
    if plat.is_file() {
        return plat;
    }
    home.join("bin").join(format!("copper{}", exe_suffix()))
}

fn compiler_self_in(home: &Path, platform: &str) -> PathBuf {
    let plat = home
        .join("bin")
        .join(platform)
        .join(format!("copper-self{}", exe_suffix()));
    if plat.is_file() {
        return plat;
    }
    home.join("bin").join(format!("copper-self{}", exe_suffix()))
}

/// 环境里到底备了哪几个平台的二进制（`bin/` 下的子目录名）。
/// 让"设置"面板能一眼看出这台机器能编什么、还缺什么。
fn platforms_in(home: &Path) -> Vec<String> {
    let bin = home.join("bin");
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&bin) {
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_dir() {
                continue;
            }
            let has = p.join(format!("copper{}", exe_suffix())).is_file()
                || p.join("copper").is_file()
                || p.join("copper.exe").is_file();
            if has {
                if let Some(name) = p.file_name() {
                    out.push(name.to_string_lossy().to_string());
                }
            }
        }
    }
    out.sort();
    out
}

fn runtime_in(home: &Path) -> PathBuf {
    home.join("lib").join("colang_rt.rs")
}

fn read_version(home: &Path) -> String {
    let p = home.join("copper.json");
    let text = match std::fs::read_to_string(&p) {
        Ok(t) => t,
        Err(_) => return String::new(),
    };
    // copper.json 是我们自己写的极简清单，只取 version 字段，不引 JSON 依赖
    for key in ["version", "版本"] {
        if let Some(i) = text.find(&format!("\"{}\"", key)) {
            let rest = &text[i + key.len() + 2..];
            if let Some(c) = rest.find(':') {
                let tail = rest[c + 1..].trim_start();
                if let Some(stripped) = tail.strip_prefix('"') {
                    if let Some(end) = stripped.find('"') {
                        return stripped[..end].to_string();
                    }
                }
            }
        }
    }
    String::new()
}

/// 解析当前生效的 Copper 环境。找不到就返回 found=false（不编造路径）。
pub fn resolve_env() -> CopperEnvInfo {
    let platform = platform_tag();
    for (dir, src) in home_candidates() {
        if !looks_like_env(&dir) {
            continue;
        }
        let compiler = compiler_in(&dir, &platform);
        let ok = compiler.is_file();
        if !ok {
            // 目录在但二进制缺：报 found=false，但把路径带回去，前端能显示"缺什么"
            return CopperEnvInfo {
                found: false,
                home: dir.display().to_string(),
                compiler: compiler.display().to_string(),
                compiler_self: compiler_self_in(&dir, &platform).display().to_string(),
                runtime: runtime_in(&dir).display().to_string(),
                platform,
                source: src.to_string(),
                version: read_version(&dir),
                platforms: platforms_in(&dir),
            };
        }
        return CopperEnvInfo {
            found: true,
            home: dir.display().to_string(),
            compiler: compiler.display().to_string(),
            compiler_self: compiler_self_in(&dir, &platform).display().to_string(),
            runtime: runtime_in(&dir).display().to_string(),
            platform,
            source: src.to_string(),
            version: read_version(&dir),
            platforms: platforms_in(&dir),
        };
    }
    CopperEnvInfo {
        found: false,
        platform,
        ..Default::default()
    }
}

#[tauri::command]
pub fn copper_env_info() -> CopperEnvInfo {
    resolve_env()
}

#[derive(Serialize)]
pub struct CopperBuildResult {
    pub ok: bool,
    /// 成功的产物路径
    pub exe: String,
    /// 编译器/rustc 的日志（失败时前端直接展示这一段）
    pub log: String,
    /// 用的是哪个编译器：env / self
    pub compiler: String,
    pub millis: u64,
}

fn find_cargo() -> Option<PathBuf> {
    if let Ok(c) = std::env::var("COLANG_CARGO") {
        let p = PathBuf::from(c);
        if p.is_file() {
            return Some(p);
        }
    }
    if let Ok(c) = std::env::var("CARGO") {
        let p = PathBuf::from(c);
        if p.is_file() {
            return Some(p);
        }
    }
    // 常见安装位置：不假设 PATH 里有 cargo（IDE 自己启动时经常没有）
    #[cfg(windows)]
    {
        if let Ok(home) = std::env::var("USERPROFILE") {
            let p = PathBuf::from(home).join(".cargo").join("bin").join("cargo.exe");
            if p.is_file() {
                return Some(p);
            }
        }
    }
    #[cfg(not(windows))]
    {
        if let Ok(home) = std::env::var("HOME") {
            let p = PathBuf::from(home).join(".cargo").join("bin").join("cargo");
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// 内核 crate 目录（生成工程要 `float-lang = { path = ... }`）
fn crate_dir() -> Option<PathBuf> {
    if let Ok(c) = std::env::var("COLANG_CRATE") {
        let p = PathBuf::from(c);
        if p.join("Cargo.toml").is_file() {
            return Some(p);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for up in 1..=5 {
                let mut anc = dir.to_path_buf();
                for _ in 0..up {
                    if !anc.pop() {
                        break;
                    }
                }
                let cand = anc.join("scaffold").join("src-tauri").join("float-lang");
                if cand.join("Cargo.toml").is_file() {
                    return Some(cand);
                }
            }
        }
    }
    None
}

/// 用环境里的编译器把 `.co` 编成原生 exe。
/// `out_dir` 为空时：产物放到源文件旁边的 `<stem>.exe`。
#[tauri::command]
pub fn copper_build(file: String, out_dir: Option<String>, use_self: Option<bool>) -> Result<CopperBuildResult, String> {
    let env = resolve_env();
    if !env.found {
        return Err(format!(
            "没有找到 Copper 环境（home={}）。装好环境或设置 COPPER_HOME 再试。",
            if env.home.is_empty() { "未设置" } else { &env.home }
        ));
    }
    let compiler = if use_self.unwrap_or(false) {
        if Path::new(&env.compiler_self).is_file() {
            env.compiler_self.clone()
        } else {
            env.compiler.clone()
        }
    } else {
        env.compiler.clone()
    };
    let rt = PathBuf::from(&env.runtime);
    if !rt.is_file() {
        return Err(format!("环境里缺运行时前奏：{}", rt.display()));
    }
    let cratep = crate_dir().ok_or("找不到 float-lang crate 目录（可设 COLANG_CRATE 指定）")?;
    let cargo = find_cargo().ok_or("找不到 cargo（可设 COLANG_CARGO 指定绝对路径）")?;
    let src = Path::new(&file);
    if !src.is_file() {
        return Err(format!("找不到源文件：{}", file));
    }
    let stem = src.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "app".into());
    let build_root = std::env::temp_dir().join("copper_build").join(format!("{}-{}", stem, std::process::id()));
    std::fs::create_dir_all(&build_root).map_err(|e| format!("建临时目录失败：{}", e))?;

    let started = std::time::Instant::now();
    let out = Command::new(&compiler)
        .env("COPPER_RT", &rt)
        .env("COLANG_CRATE", &cratep)
        .env("COLANG_OUT", &build_root)
        .env("COLANG_CARGO", &cargo)
        .env("COLANG_MODE", "build")
        .env("COLANG_TARGET", src.canonicalize().unwrap_or_else(|_| src.to_path_buf()))
        .output()
        .map_err(|e| format!("启动编译器失败：{}", e))?;
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let built = build_root.join("target").join("release").join(format!("app{}", exe_suffix()));
    if !built.is_file() {
        return Ok(CopperBuildResult {
            ok: false,
            exe: String::new(),
            log,
            compiler,
            millis: started.elapsed().as_millis() as u64,
        });
    }
    // 产物落地：默认靠近源文件
    let dest = match out_dir {
        Some(d) if !d.trim().is_empty() => {
            let dir = PathBuf::from(d);
            std::fs::create_dir_all(&dir).map_err(|e| format!("建输出目录失败：{}", e))?;
            dir.join(format!("{}{}", stem, exe_suffix()))
        }
        _ => src.parent().unwrap_or(Path::new(".")).join(format!("{}{}", stem, exe_suffix())),
    };
    if let Some(parent) = dest.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::copy(&built, &dest).map_err(|e| format!("复制产物失败：{}", e))?;
    Ok(CopperBuildResult {
        ok: true,
        exe: dest.display().to_string(),
        log,
        compiler,
        millis: started.elapsed().as_millis() as u64,
    })
}

/// 环境里有哪些编译器，各自多大（给"设置"面板显示用）
#[derive(Serialize)]
pub struct CopperCompilerInfo {
    pub path: String,
    pub bytes: u64,
    pub kind: String,
}

#[tauri::command]
pub fn copper_env_compilers() -> Vec<CopperCompilerInfo> {
    let env = resolve_env();
    let mut out = Vec::new();
    for (p, kind) in [(&env.compiler, "env"), (&env.compiler_self, "self")] {
        if p.is_empty() {
            continue;
        }
        let path = Path::new(p);
        if let Ok(m) = std::fs::metadata(path) {
            out.push(CopperCompilerInfo {
                path: p.clone(),
                bytes: m.len(),
                kind: kind.to_string(),
            });
        }
    }
    out
}
