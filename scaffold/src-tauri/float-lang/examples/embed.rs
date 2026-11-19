//! 把脚本焊进一个 exe：`embed <脚本.co> <输出.exe>`
//!
//! 没有 Rust 工具链时 `co build` 走这条路：拿 COVM 解释器做主体，
//! 把脚本源码追加在 exe 尾部（末尾 24 字节是 16 字节魔数 + 8 字节长度）。
//! 解释器启动时读自己的尾巴，有魔数就当成脚本跑——产物是单文件、双击即跑、零外部依赖。
//!
//! 有 cargo 时仍走真正的 AOT（Rust 编译），速度快几十倍。

use std::fs;
use std::io::Write;

const MAGIC: &[u8; 16] = b"COPPEREMBEDv1\0\0\0";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("用法：embed <脚本.co> <输出.exe>");
        std::process::exit(2);
    }
    let script = &args[0];
    let out = &args[1];

    let src = match fs::read(script) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("读不到 {}：{}", script, e);
            std::process::exit(1);
        }
    };

    // 解释器本体：优先环境变量，其次同目录 / 同目录 windows-x64
    let runner = match std::env::var_os("COPPER_RUN") {
        Some(p) => std::path::PathBuf::from(p),
        None => {
            let exe = std::env::current_exe().unwrap_or_default();
            let dir = exe.parent().unwrap_or(std::path::Path::new(".")).to_path_buf();
            let a = dir.join("windows-x64").join("copper-run.exe");
            if a.is_file() {
                a
            } else {
                let b = dir.join("copper-run.exe");
                if b.is_file() {
                    b
                } else {
                    eprintln!("找不到 copper-run.exe（设 COPPER_RUN 指过去）");
                    std::process::exit(1);
                }
            }
        }
    };

    let mut body = match fs::read(&runner) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("读不到解释器 {}：{}", runner.display(), e);
            std::process::exit(1);
        }
    };

    body.extend_from_slice(&src);
    body.extend_from_slice(MAGIC);
    body.extend_from_slice(&(src.len() as u64).to_le_bytes());

    match fs::File::create(out).and_then(|f| {
        let mut f = f;
        f.write_all(&body)?;
        f.flush()
    }) {
        Ok(_) => {
            println!("[embed] {} -> {}（COVM 便携版，{} KB）", script, out, body.len() / 1024);
        }
        Err(e) => {
            eprintln!("写不出 {}：{}", out, e);
            std::process::exit(1);
        }
    }
}
