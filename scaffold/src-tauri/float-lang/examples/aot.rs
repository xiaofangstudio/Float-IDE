//! AOT 演示台：把 Copper 脚本编译成真正的本机 exe。
//!
//!   cargo run --release --example aot -- 脚本.co [-o 输出.exe] [--run]
//!
//! 流程：源码 →（import 展开 / 宏展开）→ AST → Rust 源码 → cargo --release → 原生 exe。
//! 有 AOT 暂不支持的特性（指针 / ask / wait …）时，自动改用 VM 跑，**结果照常给出**：
//! AOT 永远是快路径，VM 永远是兜底，绝不出"既没编译也没运行"的局面。

use float_lang::{api, aot, parser};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

/// 清掉以前留下的 AOT 构建目录。
///
/// 为什么要有这个：每编译一个脚本就要在临时目录建一个完整 cargo 工程（5MB 上下），
/// 目录名按脚本名区分，于是跑几十个脚本就攒出几百 MB。用户既不知道、也不会去清。
/// 所以这个工具自己扫：**共享缓存和 2 小时内动过的留着**（那是增量编译要用的），
/// 其余 `copper_aot_*` 全部删掉。删失败不算错（可能正被别的进程用着）。
fn cleanup_stale_build_dirs() {
    let tmp = std::env::temp_dir();
    let rd = match std::fs::read_dir(&tmp) {
        Ok(rd) => rd,
        Err(_) => return,
    };
    let now = std::time::SystemTime::now();
    let mut removed = 0usize;
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if !name.starts_with("copper_aot_") || name == "copper_aot_cache" {
            continue; // 共享 target 缓存要留：留着才有增量编译
        }
        let path = e.path();
        if !path.is_dir() {
            continue;
        }
        // 2 小时内动过的留着：可能正是这一轮要用的目录
        let fresh = e
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| now.duration_since(t).ok())
            .map(|d| d.as_secs() < 2 * 3600)
            .unwrap_or(false);
        if fresh {
            continue;
        }
        if std::fs::remove_dir_all(&path).is_ok() {
            removed += 1;
        }
    }
    if removed > 0 {
        println!("清掉 {} 个旧的 AOT 构建目录（临时文件不留在磁盘上）", removed);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("用法：aot <脚本.co> [-o 输出.exe] [--run]");
        std::process::exit(2);
    }
    let script = PathBuf::from(&args[1]);
    let mut out: Option<PathBuf> = None;
    let mut run_after = false;
    let mut show_rust = false;
    let mut bench: Option<usize> = None;
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "-o" => {
                i += 1;
                out = Some(PathBuf::from(&args[i]));
            }
            "--run" => run_after = true,
            "--show" => show_rust = true,
            "--bench" => {
                i += 1;
                bench = args.get(i).and_then(|x| x.parse().ok()).or(Some(3));
            }
            other => {
                eprintln!("不认识的参数：{}", other);
                std::process::exit(2);
            }
        }
        i += 1;
    }

    // 先把自己以前留下的垃圾扫一遍（见函数注释：每个脚本一个 5MB cargo 工程，
    // 跑几十次就是几百 MB，用户根本不知道这些东西存在）
    cleanup_stale_build_dirs();

    let src = match std::fs::read_to_string(&script) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("读不到 {}：{}", script.display(), e);
            std::process::exit(2);
        }
    };
    let base = script.parent().map(|p| p.to_path_buf());
    let file = script.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "main.co".into());

    let t0 = Instant::now();

    // 1) 预处理（import 展开 / 宏展开，与 VM 同一条管线）
    let prepared = match api::prepare(&file, &src, base.as_deref()) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("预处理失败：{}", e.message);
            std::process::exit(1);
        }
    };

    // 2) 解析
    let prog = parser::parse_program(&file, &prepared.source);
    if let Some(e) = prog.errors.first() {
        eprintln!("语法有错：{}", e.message);
        std::process::exit(1);
    }

    // 3) 翻译成 Rust：小脚本走单文件（最快），大脚本走分块（增量编译，内存可控）
    let mut split: Option<Vec<aot::SplitFile>> = None;
    let rust_src = match aot::emit(&prog) {
        Ok(s) => s,
        Err(e) if e.message.contains("脚本太大") => {
            let stem = script
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "copper_program".into());
            let safe: String = stem.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect();
            match aot::emit_split(&prog, &safe, env!("CARGO_MANIFEST_DIR")) {
                Ok(files) => {
                    println!("分块编译：{} 个文件（{:.0} 个函数），改一个函数只重编一块", files.len(), 0.0);
                    split = Some(files);
                    String::new()
                }
                Err(e2) => {
                    eprintln!("{}", e2.message);
                    eprintln!("── 回落 VM 运行，结果不变 ──");
                    run_vm(&script, &src, base.as_deref());
                    return;
                }
            }
        }
        Err(e) => {
            eprintln!("{}", e.message);
            eprintln!("── 回落 VM 运行，结果不变 ──");
            run_vm(&script, &src, base.as_deref());
            return;
        }
    };
    if split.is_none() {
        println!("AST → Rust 源码：{} 行（{}）", rust_src.lines().count(), fmt_ms(t0.elapsed()));
        if show_rust {
            print!("{}", rust_src);
            return;
        }
    }

    // 4) cargo 编译成原生 exe
    let t1 = Instant::now();
    let built = match &split {
        Some(files) => build_split(files, &script),
        None => build_native(&rust_src, &script),
    };
    let out_exe = match built {
        Ok(p) => p,
        Err(e) => {
            eprintln!("本机编译失败：{}", e);
            eprintln!("── 回落 VM 运行，结果不变 ──");
            run_vm(&script, &src, base.as_deref());
            return;
        }
    };
    println!("rustc 编译完成：{}（{}）", out_exe.display(), fmt_ms(t1.elapsed()));

    if let Some(o) = out {
        if let Some(parent) = o.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::copy(&out_exe, &o) {
            Ok(_) => println!("已输出：{}", o.display()),
            Err(e) => eprintln!("复制产物失败：{}", e),
        }
    }

    if let Some(n) = bench {
        // 启动开销用"空程序"量出来再扣掉，否则 Windows 上光进程启动就 70ms，数字没法看
        let baseline = compile_baseline().unwrap_or_default();
        let base = bench_exe(&baseline, n.max(3)).unwrap_or(0.0);
        let me = bench_exe(&out_exe, n.max(3)).unwrap_or(0.0);
        println!("── 基准（取 {} 次最好成绩）──", n.max(3));
        println!("启动开销（空程序）: {:.1} ms", base);
        println!("本机程序总耗时      : {:.1} ms", me);
        println!("扣除启动后的计算时间: {:.1} ms", (me - base).max(0.0));
        return;
    }

    if run_after {
        let t2 = Instant::now();
        let st = Command::new(&out_exe).status();
        println!("── 本机执行用时 {} ──", fmt_ms(t2.elapsed()));
        if let Ok(st) = st {
            std::process::exit(st.code().unwrap_or(1));
        }
    }
}

fn run_vm(script: &Path, src: &str, base: Option<&Path>) {
    match float_lang::api::run_from(&script.to_string_lossy(), src, base) {
        Ok(outcome) => print!("{}", outcome.output),
        Err(e) => eprintln!("出错：{}", e.message),
    }
}

fn build_native(rust_src: &str, script: &Path) -> Result<PathBuf, String> {
    let stem = script
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "copper_program".into());
    let safe_stem: String = stem.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect();
    // 同一个脚本固定用同一个构建目录：第二次起是增量编译（1~2 秒），不再每次从零编
    let dir = std::env::temp_dir().join(format!("copper_aot_{}", safe_stem));
    std::fs::create_dir_all(dir.join("src")).map_err(|e| e.to_string())?;

    std::fs::write(
        dir.join("Cargo.toml"),
        format!(
            "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nfloat-lang = {{ path = {:?} }}\n\n[[bin]]\nname = \"{}\"\npath = \"src/main.rs\"\n\n[profile.release]\nopt-level = 3\nlto = true\ncodegen-units = 1\npanic = \"abort\"\n",
            safe_stem,
            env!("CARGO_MANIFEST_DIR"),
            safe_stem
        ),
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(dir.join("src").join("main.rs"), rust_src).map_err(|e| e.to_string())?;

    let st = Command::new("cargo")
        .arg("build")
        .arg("--release")
        .current_dir(&dir)
        .output()
        .map_err(|e| format!("cargo 起不来（{}）。AOT 需要本机装 Rust 工具链。", e))?;
    if !st.status.success() {
        return Err(format!("cargo build 失败：\n{}", String::from_utf8_lossy(&st.stderr)));
    }
    let exe = dir.join("target").join("release").join(format!("{}.exe", safe_stem));
    if !exe.exists() {
        return Err("编译完了但找不到产物".into());
    }
    Ok(exe)
}

/// 跑 n 次，取最快的一次（墙钟）
fn bench_exe(exe: &Path, n: usize) -> Option<f64> {
    let mut best = f64::MAX;
    for _ in 0..n {
        let t = Instant::now();
        let out = Command::new(exe).output().ok()?;
        if !out.status.success() {
            return None;
        }
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        if ms < best {
            best = ms;
        }
    }
    if best == f64::MAX { None } else { Some(best) }
}

/// 编译一个"什么也不干"的 Copper 程序，用来量启动开销
fn compile_baseline() -> Option<PathBuf> {
    let dir = std::env::temp_dir().join("copper_aot_baseline");
    let src = "print(0)\n";
    let prog = parser::parse_program("baseline.co", src);
    let rust = aot::emit(&prog).ok()?;
    let fake = PathBuf::from("baseline.co");
    build_native(&rust, &fake).ok().map(|_| dir.join("target/release/baseline.exe"))
}

/// 分块构建：把 workspace 的文件写进构建目录（内容没变就不动，保住 cargo 缓存），然后编译
fn build_split(files: &[aot::SplitFile], script: &Path) -> Result<PathBuf, String> {
    let stem = script
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "copper_program".into());
    let safe_stem: String = stem.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect();
    let dir = std::env::temp_dir().join(format!("copper_aot_{}_split", safe_stem));

    let mut written = 0usize;
    for f in files {
        let path = dir.join(&f.rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        // 只写变了内容的文件：mtime 不动，cargo 就不用重编那一块（这就是增量）
        let same = std::fs::read_to_string(&path).map(|old| old == f.content).unwrap_or(false);
        if !same {
            std::fs::write(&path, &f.content).map_err(|e| e.to_string())?;
            written += 1;
        }
    }
    println!("分块写入：{} 个文件里改了 {} 个", files.len(), written);

    // 共享 target 目录：所有分块工程共用一份编译缓存，float-lang 这类不变的依赖只编一次
    let shared_target = std::env::temp_dir().join("copper_aot_cache");
    /* -j 2：分块工程有十几个 crate，默认可并行到 CPU 核数，
     * 每个 rustc 都要几百 MB，并行一多就把内存打满开始 swap（实测整机卡死过）。
     * 限制成 2 个并行，总时间反而更短。 */
    let st = Command::new("cargo")
        .args(["build", "--release", "-j", "2"])
        .env("CARGO_TARGET_DIR", &shared_target)
        .current_dir(&dir)
        .output()
        .map_err(|e| format!("cargo 起不来（{}）。AOT 需要本机装 Rust 工具链。", e))?;
    if !st.status.success() {
        let err = String::from_utf8_lossy(&st.stderr).to_string();
        let e: Vec<&str> = err.lines().filter(|l| l.starts_with("error")).collect();
        return Err(if e.is_empty() { err } else { e.join(" | ") });
    }
    let exe = shared_target.join("release").join(format!("{}.exe", safe_stem));
    if !exe.exists() {
        return Err("编译完了但找不到产物".into());
    }
    Ok(exe)
}

fn fmt_ms(d: std::time::Duration) -> String {
    format!("{:.1} ms", d.as_secs_f64() * 1000.0)
}
