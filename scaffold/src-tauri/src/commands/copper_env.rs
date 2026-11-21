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
    /// 编译器对外显示的名字，统一格式 `colang 1.9.2.exe`
    pub compiler_name: String,
    /// 编译器版本（从文件名解析，其次读 copper.json）
    pub compiler_version: String,
    /// 这个环境有没有带文档（docs/ 目录），IDE 的教程与「关于 Copper」都读这里
    pub docs: bool,
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

/// 手动指定的环境目录存哪儿（一行纯文本，改环境不用重新编译 IDE）
fn override_file() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return Some(PathBuf::from(appdata).join("Float").join("copper_home.txt"));
        }
    }
    #[cfg(not(windows))]
    {
        if let Ok(home) = std::env::var("HOME") {
            return Some(PathBuf::from(home).join(".config").join("float").join("copper_home.txt"));
        }
    }
    None
}

fn read_override() -> Option<PathBuf> {
    let p = override_file()?;
    let t = std::fs::read_to_string(p).ok()?;
    let t = t.trim();
    if t.is_empty() {
        None
    } else {
        Some(PathBuf::from(t))
    }
}

fn home_candidates() -> Vec<(PathBuf, &'static str)> {
    let mut out = Vec::new();
    if let Ok(p) = std::env::var("COPPER_HOME") {
        if !p.trim().is_empty() {
            out.push((PathBuf::from(p), "env"));
        }
    }
    // 手动指定（IDE 里选过的目录），排在用户级之前
    if let Some(p) = read_override() {
        out.push((p, "manual"));
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
    // 绿色版：只在开发构建里认，避免安装版升级后还去读旧的 `<exe>/copper_env`
    // （1.9.7 及更早的安装目录里残留着内置环境，会掩盖用户新装的环境）。
    #[cfg(debug_assertions)]
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

/// 一个目录是不是像一个 Copper 环境：至少要有 copper.json，或者 bin/ 下有编译器。
fn looks_like_env(dir: &Path) -> bool {
    if dir.join("copper.json").is_file() || dir.join("docs").join("tutorial.json").is_file() {
        return true;
    }
    if dir.join("bin").join(format!("colang{}", exe_suffix())).is_file() {
        return true;
    }
    let rd = match std::fs::read_dir(dir.join("bin")) {
        Ok(r) => r,
        Err(_) => return false,
    };
    rd.flatten()
        .any(|e| is_compiler_file(&e.file_name().to_string_lossy()))
}

/// 环境里的编译器路径（兜底用：正常路径是先 scan_compilers 挑最新版本）。
///   1. 多平台布局：`bin/<platform>/colang[.exe]`
///   2. 平铺布局：  `bin/colang[.exe]`
///   3. 老名字：    `bin/<platform>/copper[.exe]` / `bin/copper[.exe]`
fn compiler_in(home: &Path, platform: &str) -> PathBuf {
    for p in [
        home.join("bin").join(platform).join(format!("colang{}", exe_suffix())),
        home.join("bin").join(format!("colang{}", exe_suffix())),
        home.join("bin").join(platform).join(format!("copper{}", exe_suffix())),
        home.join("bin").join(format!("copper{}", exe_suffix())),
    ] {
        if p.is_file() {
            return p;
        }
    }
    home.join("bin").join(format!("colang{}", exe_suffix()))
}

fn compiler_self_in(home: &Path, platform: &str) -> PathBuf {
    for p in [
        home.join("bin").join(platform).join(format!("colang-self{}", exe_suffix())),
        home.join("bin").join(format!("colang-self{}", exe_suffix())),
        home.join("bin").join(platform).join(format!("copper-self{}", exe_suffix())),
        home.join("bin").join(format!("copper-self{}", exe_suffix())),
    ] {
        if p.is_file() {
            return p;
        }
    }
    home.join("bin").join(format!("colang-self{}", exe_suffix()))
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
            let has_c = match std::fs::read_dir(&p) {
                Ok(r) => r.flatten().any(|f| is_compiler_file(&f.file_name().to_string_lossy())),
                Err(_) => false,
            };
            if has_c {
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

fn read_version(home: &Path) -> String {    let p = home.join("copper.json");
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

/* ── 编译器命名与挑选 ────────────────────────────────────────────────
 * 统一格式：`colang <大>.<中>.<小>.exe`（例：`colang 1.9.2.exe`）。
 * 解释器不叫 copper，叫 colang —— 老的 copper.exe 只当兼容名兜底。
 * 目录里有多份时**优先最新的**：版本号大的赢；一样大再比修改时间。 */

/// 从文件名里抠版本号：colang 1.9.2.exe / colang-1.9.002 / colang2.0.0
pub fn parse_version(name: &str) -> Option<(u32, u32, u32)> {
    let lower = name.to_ascii_lowercase();
    if !lower.contains("colang") {
        if lower.starts_with("copper") {
            // copper.exe 这类旧名不带版本，靠 copper.json
            return None;
        }
        return None;
    }
    let digits: String = lower
        .chars()
        .map(|c| if c.is_ascii_digit() { c } else { ' ' })
        .collect();
    let nums: Vec<u32> = digits
        .split_whitespace()
        .filter_map(|s| s.parse::<u32>().ok())
        .collect();
    if nums.len() >= 3 {
        Some((nums[0], nums[1], nums[2]))
    } else if nums.len() == 2 {
        Some((nums[0], nums[1], 0))
    } else {
        None
    }
}

/// 对外显示名：有版本就写 `colang 1.9.2.exe`，没版本退回 `colang.exe`
pub fn display_name(version: &str) -> String {
    let v = version.trim();
    if v.is_empty() {
        format!("colang{}", exe_suffix())
    } else {
        format!("colang {}{}", v, exe_suffix())
    }
    }

/// 是不是编译器二进制（colang*/copper*，排除 -run / -embed）
pub fn is_compiler_file(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let is_bin = lower.ends_with(".exe") || !lower.contains('.');
    let is_runner = lower.contains("-run") || lower.contains("-embed");
    is_bin && !is_runner && (lower.starts_with("colang") || lower.starts_with("copper"))
}

fn is_self_file(name: &str) -> bool {
    name.to_ascii_lowercase().contains("-self")
}

/// 环境里所有编译器候选（带版本与修改时间），按"最新优先"排好
pub struct Candidate {
    pub path: PathBuf,
    pub name: String,
    pub version: String,
    pub self_hosted: bool,
    pub bytes: u64,
    mtime: u64,
}

pub fn scan_compilers(home: &Path, platform: &str) -> Vec<Candidate> {
    let mut dirs: Vec<PathBuf> = vec![home.join("bin").join(platform), home.join("bin")];
    let mut out: Vec<Candidate> = Vec::new();
    for (di, dir) in dirs.drain(..).enumerate() {
        let _ = di;
        let rd = match std::fs::read_dir(&dir) {
            Ok(r) => r,
            Err(_) => continue,
        };
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_file() {
                continue;
            }
            let name = p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            if !is_compiler_file(&name) {
                continue;
            }
            let meta = e.metadata().ok();
            let bytes = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            let mtime = meta
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let ver = parse_version(&name);
            out.push(Candidate {
                path: p,
                self_hosted: is_self_file(&name),
                name,
                version: ver
                    .map(|(a, b, c)| format!("{}.{}.{}", a, b, c))
                    .unwrap_or_default(),
                bytes,
                mtime,
            });
        }
    }
    out.sort_by(|a, b| {
        let key = |c: &Candidate| -> (u8, u32, u32, u32, u64) {
            let (x, y, z) = parse_version(&c.name).unwrap_or((0, 0, 0));
            (c.self_hosted as u8, u32::MAX - x, u32::MAX - y, u32::MAX - z, u64::MAX - c.mtime)
        };
        key(a).cmp(&key(b))
    });
    out
}

fn bound_compiler_file() -> Option<PathBuf> {
    override_file().map(|p| p.with_file_name("copper_compiler.txt"))
}

/// 用户在设置里手选的那个 colang exe（存的是绝对路径）
fn read_bound_compiler() -> Option<PathBuf> {
    let p = bound_compiler_file()?;
    let t = std::fs::read_to_string(p).ok()?;
    let t = t.trim();
    if t.is_empty() {
        return None;
    }
    let path = PathBuf::from(t);
    if path.is_file() {
        Some(path)
    } else {
        None
    }
}

/// 由编译器路径反推环境根：`<home>/bin/<平台>/colang 1.9.2.exe` → `<home>`
fn home_of(compiler: &Path) -> Option<PathBuf> {
    let bin = compiler.parent()?;
    if bin.file_name().map(|n| n.to_string_lossy().to_string()) == Some("bin".into()) {
        return bin.parent().map(|p| p.to_path_buf());
    }
    // bin/<平台>/x.exe → 上两层
    bin.parent()
        .filter(|p| p.file_name().map(|n| n.to_string_lossy().to_string()) == Some("bin".into()))
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .or_else(|| bin.parent().map(|p| p.to_path_buf()))
}

/// 解析当前生效的 Copper 环境。找不到就返回 found=false（不编造路径）。
pub fn resolve_env() -> CopperEnvInfo {
    let platform = platform_tag();

    /* 0. 用户在设置里手选了某个 colang exe：它优先于所有自动查找。
     *    "查找 Copper" 那个按钮绑的就是这里 —— 解释器叫 colang，不叫 copper。 */
    if let Some(bound) = read_bound_compiler() {
        let home = home_of(&bound).unwrap_or_else(|| bound.parent().map(|p| p.to_path_buf()).unwrap_or_default());
        let file_name = bound.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let ver = parse_version(&file_name)
            .map(|(a, b, c)| format!("{}.{}.{}", a, b, c))
            .unwrap_or_else(|| read_version(&home));
        let self_path = scan_compilers(&home, &platform)
            .into_iter()
            .find(|c| c.self_hosted)
            .map(|c| c.path.display().to_string())
            .unwrap_or_else(|| compiler_self_in(&home, &platform).display().to_string());
        return CopperEnvInfo {
            found: true,
            home: home.display().to_string(),
            compiler: bound.display().to_string(),
            compiler_self: self_path,
            runtime: runtime_in(&home).display().to_string(),
            platform,
            source: "manual-compiler".to_string(),
            version: read_version(&home),
            platforms: platforms_in(&home),
            compiler_name: display_name(&ver),
            compiler_version: ver,
            docs: home.join("docs").is_dir(),
        };
    }

    for (dir, src) in home_candidates() {
        if !looks_like_env(&dir) {
            continue;
        }
        let picked = scan_compilers(&dir, &platform).into_iter().find(|c| !c.self_hosted);
        let compiler = match &picked {
            Some(c) => c.path.clone(),
            None => compiler_in(&dir, &platform),
        };
        let ok = compiler.is_file();
        let env_ver = read_version(&dir);
        let ver = picked
            .as_ref()
            .map(|c| c.version.clone())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| env_ver.clone());
        let self_path = scan_compilers(&dir, &platform)
            .into_iter()
            .find(|c| c.self_hosted)
            .map(|c| c.path.display().to_string())
            .unwrap_or_else(|| compiler_self_in(&dir, &platform).display().to_string());
        if !ok {
            // 目录在但二进制缺：报 found=false，但把路径带回去，前端能显示"缺什么"
            return CopperEnvInfo {
                found: false,
                home: dir.display().to_string(),
                compiler: compiler.display().to_string(),
                compiler_self: self_path,
                runtime: runtime_in(&dir).display().to_string(),
                platform,
                source: src.to_string(),
                version: env_ver,
                platforms: platforms_in(&dir),
                compiler_name: display_name(&ver),
                compiler_version: ver,
                docs: dir.join("docs").is_dir(),
            };
        }
        return CopperEnvInfo {
            found: true,
            home: dir.display().to_string(),
            compiler: compiler.display().to_string(),
            compiler_self: self_path,
            runtime: runtime_in(&dir).display().to_string(),
            platform,
            source: src.to_string(),
            version: env_ver,
            platforms: platforms_in(&dir),
            compiler_name: display_name(&ver),
            compiler_version: ver,
            docs: dir.join("docs").is_dir(),
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

/// 内核 crate 目录（生成工程要 `float-lang = { path = ... }`）。
/// 顺序：COLANG_CRATE → 环境自带的 lib/float-lang → 开发机上的仓库路径。
fn crate_dir() -> Option<PathBuf> {
    if let Ok(c) = std::env::var("COLANG_CRATE") {
        let p = PathBuf::from(c);
        if p.join("Cargo.toml").is_file() {
            return Some(p);
        }
    }
    let env = resolve_env();
    if !env.home.is_empty() {
        let cand = PathBuf::from(&env.home).join("lib").join("float-lang");
        if cand.join("Cargo.toml").is_file() {
            return Some(cand);
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

/// 把 `.co` 编成原生 exe。
/// `out_dir` 为空时：产物放到源文件旁边的 `<stem>.exe`。
///
/// 主路径是**进程内内核 emitter**（和 F5 的 AOT 同一份代码），保证
/// 「VM 能跑的，导出的 exe 也能跑」；`use_self=true` 才强制走环境里的自举编译器，
/// 那条路的 emitter 能力面落后，只用于验证自举本身。
#[tauri::command]
pub fn copper_build(file: String, out_dir: Option<String>, use_self: Option<bool>) -> Result<CopperBuildResult, String> {
    let src = Path::new(&file);
    if !src.is_file() {
        return Err(format!("找不到源文件：{}", file));
    }
    let stem = src.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "app".into());
    let dest = match out_dir.clone() {
        Some(d) if !d.trim().is_empty() => {
            let dir = PathBuf::from(d);
            std::fs::create_dir_all(&dir).map_err(|e| format!("建输出目录失败：{}", e))?;
            dir.join(format!("{}{}", stem, exe_suffix()))
        }
        _ => src.parent().unwrap_or(Path::new(".")).join(format!("{}{}", stem, exe_suffix())),
    };
    let started = std::time::Instant::now();

    /* 1) 进程内构建（默认）：用当前内核的 emitter 生成 Rust 再编。
     * 不依赖环境里那个能力面落后的自举编译器。 */
    if !use_self.unwrap_or(false) {
        let text = std::fs::read_to_string(src).map_err(|e| format!("读源文件失败：{e}"))?;
        let key = format!("build-{}", aot_hash(src));
        match float_lang::aot::build_exe(src, &text, float_lang::MANIFEST_DIR, &key) {
            Ok(built) => {
                if let Some(parent) = dest.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                std::fs::copy(&built, &dest).map_err(|e| format!("复制产物失败：{e}"))?;
                return Ok(CopperBuildResult {
                    ok: true,
                    exe: dest.display().to_string(),
                    log: format!("内核 AOT（进程内 emitter）\n产物：{}", built.display()),
                    compiler: "float-lang（进程内）".into(),
                    millis: started.elapsed().as_millis() as u64,
                });
            }
            Err(e) => {
                return Ok(CopperBuildResult {
                    ok: false,
                    exe: String::new(),
                    log: format!("内核 AOT 失败：{e}"),
                    compiler: "float-lang（进程内）".into(),
                    millis: started.elapsed().as_millis() as u64,
                });
            }
        }
    }

    // 2) 显式要求自举编译器：走环境
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
    let cratep = crate_dir();
    let cargo = find_cargo().ok_or("找不到 cargo（可设 COLANG_CARGO 指定绝对路径）")?;
    let build_root = std::env::temp_dir().join("copper_build").join(format!("{}-{}", stem, std::process::id()));
    std::fs::create_dir_all(&build_root).map_err(|e| format!("建临时目录失败：{}", e))?;
    let mut cmd = Command::new(&compiler);
    cmd.env("COPPER_RT", &rt);
    if let Some(c) = &cratep {
        cmd.env("COLANG_CRATE", c);
    }
    let out = cmd
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
    // 产物落地（dest 已在函数开头按 out_dir 算好）
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

/// 环境里有哪些编译器，各自多大、什么版本（给"设置"面板显示用）
#[derive(Serialize)]
pub struct CopperCompilerInfo {
    pub path: String,
    pub bytes: u64,
    pub kind: String,
    /// 版本号，如 "1.9.2"；解析不出来就是空串
    pub version: String,
    /// 对外显示名，统一 `colang 1.9.2.exe`
    pub name: String,
    /// 是不是当前要用的那一个（列表里最新的）
    pub recommended: bool,
}

#[tauri::command]
pub fn copper_env_compilers() -> Vec<CopperCompilerInfo> {
    let env = resolve_env();
    let mut out: Vec<CopperCompilerInfo> = Vec::new();
    if env.found {
        let platform = platform_tag();
        let home = PathBuf::from(&env.home);
        for c in scan_compilers(&home, &platform) {
            let version = if c.version.is_empty() { env.compiler_version.clone() } else { c.version.clone() };
            out.push(CopperCompilerInfo {
                path: c.path.display().to_string(),
                bytes: c.bytes,
                kind: if c.self_hosted { "self".into() } else { "env".into() },
                name: display_name(&version),
                version,
                recommended: false,
            });
        }
    }
    if out.is_empty() {
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
                    version: env.compiler_version.clone(),
                    name: env.compiler_name.clone(),
                    recommended: false,
                });
            }
        }
    }
    /* 推荐 = 非自举里最新的那个；自举版排在后面只作参考 */
    if let Some(i) = out.iter().position(|c| c.kind == "env") {
        out[i].recommended = true;
    }
    out
}

/* ── 手选编译器 ─────────────────────────────────────────────────────
 * 用户可能只下载了一个 `colang 1.9.2.exe`，没有整包环境。
 * 设置页「查找 Copper」选完这个文件就绑上，下次启动照旧生效。 */

#[tauri::command]
pub fn copper_env_bind_compiler(path: String) -> Result<CopperEnvInfo, String> {
    let p = PathBuf::from(path.trim());
    if !p.is_file() {
        return Err(format!("文件不存在：{}", p.display()));
    }
    let name = p.file_name().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
    if !name.contains("colang") && !name.contains("copper") {
        return Err(format!(
            "这个文件不像 Copper 解释器：{name}。解释器叫 colang（例：colang 1.9.2.exe），不叫别的名字。"
        ));
    }
    let f = bound_compiler_file().ok_or("取不到配置目录")?;
    if let Some(parent) = f.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("建配置目录失败：{}", e))?;
    }
    let abs = p.canonicalize().unwrap_or(p.clone());
    std::fs::write(&f, abs.display().to_string()).map_err(|e| format!("写配置失败：{}", e))?;
    Ok(resolve_env())
}

/// 解除手选，回到自动查找
#[tauri::command]
pub fn copper_env_unbind_compiler() -> CopperEnvInfo {
    if let Some(f) = bound_compiler_file() {
        let _ = std::fs::remove_file(f);
    }
    resolve_env()
}

/* ── 环境自带的文档 ─────────────────────────────────────────────────
 * 教程和「关于 Copper」都属于环境：环境更新，内容跟着换。
 * 只读 `<home>/docs/` 下的文件，路径不许跳出这个目录。 */

#[derive(Serialize)]
pub struct CopperAsset {
    pub rel: String,
    pub bytes: u64,
}

#[tauri::command]
pub fn copper_env_assets() -> Vec<CopperAsset> {
    let env = resolve_env();
    let mut out = Vec::new();
    if !env.found {
        return out;
    }
    let docs = PathBuf::from(&env.home).join("docs");
    let rd = match std::fs::read_dir(&docs) {
        Ok(r) => r,
        Err(_) => return out,
    };
    for e in rd.flatten() {
        let p = e.path();
        let name = p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let lower = name.to_ascii_lowercase();
        if !p.is_file() || !(lower.ends_with(".md") || lower.ends_with(".json") || lower.ends_with(".txt")) {
            continue;
        }
        out.push(CopperAsset {
            rel: format!("docs/{}", name),
            bytes: e.metadata().map(|m| m.len()).unwrap_or(0),
        });
    }
    out.sort_by(|a, b| a.rel.cmp(&b.rel));
    out
}

/// 读环境文档：rel 形如 `docs/tutorial.json`，只允许 docs/ 下的相对路径
#[tauri::command]
pub fn copper_env_read_asset(rel: String) -> Result<String, String> {
    let env = resolve_env();
    if !env.found {
        return Err("没有找到 Copper 环境，读不到环境文档".into());
    }
    let clean = rel.trim().replace('\\', "/");
    if clean.contains("..") || !clean.starts_with("docs/") {
        return Err(format!("只允许读 docs/ 下的文件：{rel}"));
    }
    let path = PathBuf::from(&env.home).join(&clean);
    if !path.is_file() {
        return Err(format!("环境里没有这份文档：{clean}"));
    }
    std::fs::read_to_string(&path).map_err(|e| format!("读文档失败：{e}"))
}

/* ── 手动指定 / 更新环境包 ───────────────────────────────────────────
 * 用户可能把 Copper 装在任何地方，也可能只下载了一个 zip。这两件事都要在
 * IDE 里能做完（选目录 / 从 zip 铺开），不能要求先去改环境变量再重启。 */

/// 默认装到哪儿：Windows `%APPDATA%\Float\copper`，其它 `~/.copper`
fn default_home() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return Some(PathBuf::from(appdata).join("Float").join("copper"));
        }
    }
    #[cfg(not(windows))]
    {
        if let Ok(home) = std::env::var("HOME") {
            return Some(PathBuf::from(home).join(".copper"));
        }
    }
    None
}

/// 手动指定环境目录（会校验里面有没有 copper.json / 编译器）
#[tauri::command]
pub fn copper_env_set(home: String) -> Result<CopperEnvInfo, String> {
    let dir = PathBuf::from(home.trim());
    if !dir.is_dir() {
        return Err(format!("目录不存在：{}", dir.display()));
    }
    if !looks_like_env(&dir) {
        return Err(format!(
            "这个目录不像 Copper 环境（要有 copper.json，或 bin/<平台>/copper{}）",
            exe_suffix()
        ));
    }
    let f = override_file().ok_or("取不到配置目录")?;
    if let Some(parent) = f.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("建配置目录失败：{}", e))?;
    }
    std::fs::write(&f, dir.canonicalize().unwrap_or(dir.clone()).display().to_string())
        .map_err(|e| format!("写配置失败：{}", e))?;
    let info = resolve_env();
    Ok(info)
}

/// 清掉手动指定，回到自动查找
#[tauri::command]
pub fn copper_env_clear() -> CopperEnvInfo {
    if let Some(f) = override_file() {
        let _ = std::fs::remove_file(f);
    }
    resolve_env()
}

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<u64> {
    let mut n = 0u64;
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let src = e.path();
        let dst = to.join(e.file_name());
        if src.is_dir() {
            n += copy_tree(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst)?;
            n += 1;
        }
    }
    Ok(n)
}

/// 找一个目录里最新修改的 Copper 包（zip）
fn newest_package(dir: &Path) -> Option<PathBuf> {
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    let rd = std::fs::read_dir(dir).ok()?;
    for e in rd.flatten() {
        let p = e.path();
        if !p.is_file() {
            continue;
        }
        let name = p.file_name().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
        if !name.contains("copper") || !(name.ends_with(".zip")) {
            continue;
        }
        if let Ok(m) = e.metadata() {
            if let Ok(t) = m.modified() {
                if best.as_ref().map(|(bt, _)| t > *bt).unwrap_or(true) {
                    best = Some((t, p));
                }
            }
        }
    }
    best.map(|(_, p)| p)
}

/// 从下载的 zip 包装环境：解开 → 找到里面的环境根（含 copper.json 的那层）→ 铺到默认位置
#[tauri::command]
pub fn copper_env_install(zip: Option<String>) -> Result<CopperEnvInfo, String> {
    let pkg = match zip.map(|s| PathBuf::from(s.trim())).filter(|p| !p.as_os_str().is_empty()) {
        Some(p) => p,
        None => {
            let mut found = None;
            #[cfg(windows)]
            {
                if let Ok(profile) = std::env::var("USERPROFILE") {
                    found = newest_package(&PathBuf::from(&profile).join("Downloads"))
                        .or_else(|| newest_package(&PathBuf::from(&profile).join("Desktop")));
                }
            }
            #[cfg(not(windows))]
            {
                if let Ok(home) = std::env::var("HOME") {
                    found = newest_package(&PathBuf::from(&home).join("Downloads"))
                        .or_else(|| newest_package(&PathBuf::from(&home).join("Desktop")));
                }
            }
            found.ok_or("没在下载/桌面目录里找到 Copper 安装包（*.zip），请在参数里给绝对路径")?
        }
    };
    if !pkg.is_file() {
        return Err(format!("安装包不存在：{}", pkg.display()));
    }
    let tmp = std::env::temp_dir().join(format!("copper_pkg_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| format!("建临时目录失败：{}", e))?;

    // 解压：Windows 用自带的 Expand-Archive，其它平台试 unzip
    #[cfg(windows)]
    let st = Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &format!(
                "Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force",
                pkg.display(),
                tmp.display()
            ),
        ])
        .output();
    #[cfg(not(windows))]
    let st = Command::new("unzip")
        .args(["-o", "-q", pkg.to_string_lossy().as_ref(), "-d", tmp.to_string_lossy().as_ref()])
        .output();
    let st = st.map_err(|e| format!("解压失败：{}", e))?;
    if !st.status.success() {
        return Err(format!(
            "解压失败：{}",
            String::from_utf8_lossy(&st.stderr).trim()
        ));
    }

    // 找环境根：优先含 copper.json 的目录，其次含 bin/ 的
    fn find_root(dir: &Path, depth: u32) -> Option<PathBuf> {
        if dir.join("copper.json").is_file() {
            return Some(dir.to_path_buf());
        }
        if depth == 0 {
            return None;
        }
        if let Ok(rd) = std::fs::read_dir(dir) {
            let mut subs: Vec<_> = rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
            // 名字里带 copper 的先找
            subs.sort_by_key(|p| {
                let n = p.file_name().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
                if n.contains("copper") { 0 } else { 1 }
            });
            for s in subs {
                if let Some(r) = find_root(&s, depth - 1) {
                    return Some(r);
                }
            }
        }
        None
    }
    let root = find_root(&tmp, 2).ok_or("解开的包里没找到 copper.json（不是 Copper 安装包？）")?;

    let dest = default_home().ok_or("取不到默认安装目录")?;
    std::fs::create_dir_all(&dest).map_err(|e| format!("建安装目录失败：{}", e))?;
    copy_tree(&root, &dest).map_err(|e| format!("铺开环境失败：{}", e))?;
    let _ = std::fs::remove_dir_all(&tmp);

    // 装完直接生效：清掉旧的手动指定（默认位置本身就在查找链里）
    if let Some(f) = override_file() {
        let _ = std::fs::remove_file(f);
    }
    Ok(resolve_env())
}

/* ────────────────────────────────────────────────────────────────────────
 * AOT 运行：编成原生 exe 再跑（设置里 F5 可选 VM / AOT）
 *
 * 和 VM 运行的区别：
 *   - 编的是**磁盘上**的文件（未保存的改动先 Ctrl+S），产物按内容 hash 缓存，
 *     同一份源码第二次运行不再重编（首次 10~20 s，之后秒开）。
 *   - 交互：AOT 产物的 ask 本来读 stdin。IDE 设 COPPER_ASK_MARK=1，
 *     程序会在 stderr 打 `\x01ASK\x01<提示>`，后端翻成 float-input-request，
 *     前端答完走 copper_aot_input 写回子进程 stdin。
 * ──────────────────────────────────────────────────────────────────────── */

#[derive(serde::Serialize)]
pub struct AotRunResult {
    pub ok: bool,
    pub exe: String,
    pub millis: u64,
    pub code: i32,
    pub error: String,
    /// 这次是不是重新编译了（false = 命中缓存）
    pub compiled: bool,
    /// 编译耗时（0 = 命中缓存）
    pub compile_ms: u64,
}

static AOT_STDIN: std::sync::Mutex<Option<(String, std::process::ChildStdin)>> =
    std::sync::Mutex::new(None);
static AOT_CHILD: std::sync::Mutex<Option<(String, std::process::Child)>> =
    std::sync::Mutex::new(None);

/// 源文件内容 + 同目录 .co/.cw 的修改时间 → 缓存 hash（改库文件也会重编）
fn aot_hash(src: &Path) -> String {
    fn mix(h: &mut u64, bytes: &[u8]) {
        for b in bytes {
            *h ^= *b as u64;
            *h = h.wrapping_mul(0x100_0000_01b3);
        }
    }
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    mix(&mut h, src.display().to_string().as_bytes());
    if let Ok(s) = std::fs::read(src) {
        mix(&mut h, &s);
    }
    if let Some(dir) = src.parent() {
        if let Ok(rd) = std::fs::read_dir(dir) {
            let mut names: Vec<(String, u64)> = Vec::new();
            for e in rd.flatten() {
                let p = e.path();
                let ext = p.extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
                if ext != "co" && ext != "cw" {
                    continue;
                }
                let mt = e.metadata().and_then(|m| m.modified()).ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs()).unwrap_or(0);
                names.push((e.file_name().to_string_lossy().to_string(), mt));
            }
            names.sort();
            for (n, mt) in names {
                mix(&mut h, n.as_bytes());
                mix(&mut h, &mt.to_le_bytes());
            }
        }
    }
    format!("{:016x}", h)
}

#[tauri::command]
pub async fn copper_run_aot(
    app: tauri::AppHandle,
    file: String,
    force_rebuild: Option<bool>,
) -> Result<AotRunResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        aot_run_blocking(app, file, force_rebuild.unwrap_or(false))
    })
    .await
    .map_err(|e| format!("AOT 运行线程崩了：{}", e))?
}

fn aot_run_blocking(
    app: tauri::AppHandle,
    file: String,
    force: bool,
) -> Result<AotRunResult, String> {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};
    use tauri::Emitter;

    let src = PathBuf::from(&file);
    if !src.is_file() {
        return Err(format!("找不到源文件：{file}"));
    }
    let started = std::time::Instant::now();
    /* 用内核自己的 emitter 编，而不是环境里那个自举版 colang：
     * VM 能跑的语义这里全支持（wait until / time / ~ / ask …），
     * 环境只管提供文档与运行时，不再卡住 IDE 的能力边界。 */
    let hash = aot_hash(&src);
    let stem = src.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "app".into());
    let safe: String = stem.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect();
    let cache_root = std::env::temp_dir().join(format!("copper_ide_aot_{}", hash));
    let exe = cache_root.join(format!("{}{}", safe, exe_suffix()));

    let compile_ms;
    let mut compiled = false;
    if force || !exe.is_file() {
        let body = std::fs::read_to_string(&src).map_err(|e| format!("读源文件失败：{e}"))?;
        let t = std::time::Instant::now();
        let built = float_lang::aot::build_exe(&src, &body, float_lang::MANIFEST_DIR, &hash)
            .map_err(|m| format!("AOT 编译失败：\n{m}"))?;
        compile_ms = t.elapsed().as_millis() as u64;
        compiled = true;
        if built != exe && built.is_file() {
            std::fs::create_dir_all(&cache_root).ok();
            std::fs::copy(&built, &exe).map_err(|e| format!("复制产物失败：{e}"))?;
        }
    } else {
        compile_ms = 0;
    }
    if !exe.is_file() {
        return Err("AOT 编译完了但没找到产物".into());
    }

    let run_id = format!(
        "aot-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let mut cmd = Command::new(&exe);
    cmd.env("COPPER_ASK_MARK", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    crate::commands::hide_console_window(&mut cmd);
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("启动 AOT 产物失败：{e}"))?;

    if let Some(si) = child.stdin.take() {
        if let Ok(mut g) = AOT_STDIN.lock() {
            *g = Some((run_id.clone(), si));
        }
    }

    let out_id = run_id.clone();
    let out_app = app.clone();
    let stdout = child.stdout.take();
    let out_thread = std::thread::spawn(move || {
        if let Some(so) = stdout {
            for line in BufReader::new(so).lines().map_while(Result::ok) {
                let _ = out_app.emit("float-aot-line", serde_json::json!({
                    "runId": out_id, "kind": "out", "text": line,
                }));
            }
        }
    });

    let err_id = run_id.clone();
    let err_app = app.clone();
    let stderr = child.stderr.take();
    let err_thread = std::thread::spawn(move || {
        if let Some(se) = stderr {
            for line in BufReader::new(se).lines().map_while(Result::ok) {
                if let Some(prompt) = line.strip_prefix("\u{1}ASK\u{1}") {
                    let _ = err_app.emit("float-input-request", serde_json::json!({
                        "runId": err_id,
                        "prompt": prompt.replace("\\n", "\n"),
                    }));
                } else if !line.trim().is_empty() {
                    let _ = err_app.emit("float-aot-line", serde_json::json!({
                        "runId": err_id, "kind": "err", "text": line,
                    }));
                }
            }
        }
    });

    let pid_id = run_id.clone();
    // 子进程交给全局持有，红色停止按钮才能杀得掉；等待循环每次短暂锁一下
    if let Ok(mut g) = AOT_CHILD.lock() {
        *g = Some((run_id.clone(), child));
    }
    let code;
    loop {
        let mut finished: Option<i32> = None;
        if let Ok(mut g) = AOT_CHILD.lock() {
            match g.as_mut() {
                Some((id, c)) if id == &pid_id => match c.try_wait() {
                    Ok(Some(st)) => finished = Some(st.code().unwrap_or(0)),
                    Ok(None) => {}
                    Err(_) => finished = Some(-1),
                },
                // 被 copper_aot_stop 杀掉：表里已经清空
                _ => finished = Some(-1),
            }
            if finished.is_some() {
                *g = None;
            }
        }
        if let Some(c) = finished {
            code = c;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(60));
    }
    // 子进程退出：清掉输入通道，唤醒读线程（它们读到 EOF 会自己结束）
    if let Ok(mut g) = AOT_STDIN.lock() {
        if g.as_ref().map(|(id, _)| id == &pid_id).unwrap_or(false) {
            *g = None;
        }
    }
    let _ = out_thread.join();
    let _ = err_thread.join();

    Ok(AotRunResult {
        ok: code == 0,
        exe: exe.display().to_string(),
        millis: started.elapsed().as_millis() as u64,
        code,
        error: if code == 0 { String::new() } else { format!("程序退出码 {code}") },
        compiled,
        compile_ms,
    })
}

/// AOT 产物的 ask 回填：把答案写进子进程 stdin
#[tauri::command]
pub fn copper_aot_input(run_id: String, answer: String) -> bool {
    use std::io::Write;
    let mut g = match AOT_STDIN.lock() {
        Ok(g) => g,
        Err(_) => return false,
    };
    match g.as_mut() {
        Some((id, si)) if id == &run_id => {
            let _ = writeln!(si, "{}", answer);
            let _ = si.flush();
            true
        }
        _ => false,
    }
}

/// 停止 AOT 产物（红色停止按钮）
#[tauri::command]
pub fn copper_aot_stop() -> bool {
    if let Ok(mut g) = AOT_CHILD.lock() {
        if let Some((_, child)) = g.as_mut() {
            let _ = child.kill();
            *g = None;
            return true;
        }
    }
    false
}