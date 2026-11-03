// 对拍：把 cases 文件里每段脚本分别用 VM 和 AOT 跑，输出必须一致。
// 用法：CASES=cases10.txt cargo run --release --example aot_parity

use float_lang::{aot, parser};
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let case_file = std::env::var("CASES").unwrap_or_else(|_| "cases.txt".to_string());
    let text = std::fs::read_to_string(format!("../../tests/parity/{}", case_file)).expect("读用例");

    let mut cases: Vec<(String, String)> = Vec::new();
    let mut cur: Option<(String, Vec<String>)> = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("### ") {
            if let Some((n, ls)) = cur.take() {
                cases.push((n, ls.join("\n") + "\n"));
            }
            cur = Some((rest.trim().to_string(), Vec::new()));
        } else if let Some((_, ls)) = cur.as_mut() {
            ls.push(line.to_string());
        }
    }
    if let Some((n, ls)) = cur.take() {
        cases.push((n, ls.join("\n") + "\n"));
    }

    let (mut same, mut fallback, mut bad) = (0, 0, 0);
    for (name, src) in &cases {
        let vm = float_lang::run_text(src);
        let prog = parser::parse_program("case.co", src);
        let rust = match aot::emit(&prog) {
            Ok(r) => r,
            Err(e) => {
                fallback += 1;
                println!("回退  {} —— {}", name, e.message);
                continue;
            }
        };
        match build_and_run(&rust) {
            Ok(out) => {
                let want = match &vm {
                    Ok(o) => o.clone(),
                    Err(e) => {
                        println!("❌ {}：VM 报错但 AOT 跑出了结果\n  VM: {}", name, e.message);
                        bad += 1;
                        continue;
                    }
                };
                if out.trim_end() == want.trim_end() {
                    same += 1;
                } else {
                    bad += 1;
                    println!("❌ {}\n  VM : {:?}\n  AOT: {:?}", name, want, out);
                }
            }
            Err(e) => {
                // AOT 编译失败算"回退"（上层会落回 VM），但要打出来看
                fallback += 1;
                println!("回退  {} —— 编译失败：{}", name, e.lines().take(3).collect::<Vec<_>>().join(" | "));
            }
        }
    }
    println!("AOT 对拍：一致 {} / 回退 {} / 不一致 {}", same, fallback, bad);
    if bad > 0 {
        std::process::exit(1);
    }
}

fn build_and_run(rust: &str) -> Result<String, String> {
    let dir = std::env::temp_dir().join("copper_aot_parity");
    std::fs::create_dir_all(dir.join("src")).map_err(|e| e.to_string())?;
    std::fs::write(
        dir.join("Cargo.toml"),
        format!(
            "[package]\nname = \"paritycase\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nfloat-lang = {{ path = {:?} }}\n\n[[bin]]\nname = \"paritycase\"\npath = \"src/main.rs\"\n\n[profile.release]\nopt-level = 3\nlto = true\ncodegen-units = 1\npanic = \"abort\"\n",
            env!("CARGO_MANIFEST_DIR")
        ),
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(dir.join("src").join("main.rs"), rust).map_err(|e| e.to_string())?;
    let out = Command::new("cargo")
        .args(["build", "--release", "-q"])
        .current_dir(&dir)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).to_string();
        let e: Vec<&str> = err.lines().filter(|l| l.starts_with("error")).collect();
        return Err(if e.is_empty() { err } else { e.join(" | ") });
    }
    let exe: PathBuf = dir.join("target/release/paritycase.exe");
    let run = Command::new(&exe).output().map_err(|e| e.to_string())?;
    if !run.status.success() {
        return Err(format!("运行出错：{}", String::from_utf8_lossy(&run.stderr)));
    }
    Ok(String::from_utf8_lossy(&run.stdout).to_string())
}
