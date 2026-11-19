//! 跑一个 .cw 脚本：`cargo run --release --example run -- 路径 [输入文件]`
//!
//! 输入文件按行给 `ask` 的答案；不给就用同名 .in 文件（存在的话）。
//! 编译 + 运行套在 64MB 大栈线程上：深递归、深嵌套解析都不爆栈
//! （和 IDE 的 lang_run、AOT 生成程序的大栈线程同一策略）。

use float_lang::Vm;
use std::env;
use std::fs;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    // 没给脚本参数：看看自己身上是否焊着脚本（co build 在无 cargo 时这样产出的 exe）。
    if args.is_empty() {
        match read_embedded() {
            Some(src) => {
                run_source("<embedded>".to_string(), src, None);
                return;
            }
            None => {
                repl();
                return;
            }
        }
    }
    if args[0] == "--repl" {
        repl();
        return;
    }
    let path = args[0].clone();
    let src = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("读不到 {}：{}", path, e);
            std::process::exit(1);
        }
    };
    let input_path = if args.len() > 1 {
        Some(args[1].clone())
    } else {
        // 同名 .in：按扩展名换，不写死 ".cw"——以前只换 .cw，
        // 跑 .co 脚本时"猜出的 .in 路径"就是脚本自己，整份源码被当成
        // ask 的答案喂进去（第一行注释成了输入）。任何后缀都要按名字找 .in。
        let guess = std::path::Path::new(&path)
            .with_extension("in")
            .to_string_lossy()
            .into_owned();
        if guess != path && fs::metadata(&guess).is_ok() {
            Some(guess)
        } else {
            None
        }
    };
    run_source(path, src, input_path);
}

/// 读自己尾巴上焊的脚本：末尾 24 字节 = 16 字节魔数 + 8 字节长度。
fn read_embedded() -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    const MAGIC: &[u8; 16] = b"COPPEREMBEDv1\0\0\0";
    let exe = env::current_exe().ok()?;
    let mut f = fs::File::open(exe).ok()?;
    let len = f.metadata().ok()?.len();
    if len < 24 {
        return None;
    }
    f.seek(SeekFrom::End(-24)).ok()?;
    let mut tail = [0u8; 24];
    f.read_exact(&mut tail).ok()?;
    if &tail[0..16] != MAGIC {
        return None;
    }
    let n = u64::from_le_bytes([
        tail[16], tail[17], tail[18], tail[19], tail[20], tail[21], tail[22], tail[23],
    ]);
    if n == 0 || n + 24 > len {
        return None;
    }
    f.seek(SeekFrom::Start(len - 24 - n)).ok()?;
    let mut buf = vec![0u8; n as usize];
    f.read_exact(&mut buf).ok()?;
    String::from_utf8(buf).ok()
}

fn run_source(path: String, src: String, input_path: Option<String>) {
    let inputs: Vec<String> = match input_path {
        Some(p) => fs::read_to_string(&p)
            .unwrap_or_default()
            .lines()
            .map(|l| l.to_string())
            .collect(),
        None => Vec::new(),
    };

    let code = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || -> i32 {
            // 走 api::compile_source：import 的展开、行号归属都在里面处理
            let base = std::path::Path::new(&path).parent();
            let chunk = match float_lang::api::compile_source(&path, &src, base) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("编译错误：{}", e);
                    return 1;
                }
            };
            let mut vm = Vm::new(chunk);
            // 没给 .in 文件时，ask 直接读 stdin —— 命令行跑交互脚本必须这样，
            // 否则 ask 永远拿到空串（和 AOT 版行为也不一致）。
            if inputs.is_empty() {
                vm.set_stdin_mode(true);
            }
            vm.set_inputs(inputs);
            let t = std::time::Instant::now();
            match vm.run() {
                Ok(o) => {
                    print!("{}", o.output);
                    if !o.reports.is_empty() {
                        println!("\n── 报告 ──");
                        for r in &o.reports {
                            for line in r.lines() {
                                println!("{}", line);
                            }
                        }
                    }
                    // 计时只做诊断，默认不打扰输出（命令行要像 python 那样干净）。
            // 想看得设环境变量 COPPER_TIMING=1。
            if std::env::var_os("COPPER_TIMING").is_some() {
                eprintln!("—— 用时 {:.2} ms", t.elapsed().as_secs_f64() * 1000.0);
            }
                    0
                }
                Err(e) => {
                    print!("{}", vm.take_output());
                    eprintln!("\n运行出错：{}", e.message);
                    1
                }
            }
        })
        .unwrap()
        .join()
        .unwrap_or(2);
    let _ = std::io::Write::flush(&mut std::io::stdout());
    std::process::exit(code);
}

/* ── 交互模式（`co` 不带脚本，或者 `copper-run --repl`）───────────────
 * 语义：把输入过的行攒成一段源码，每次整段重编译重跑，只打印"这次比上次多出来
 * 的输出"。所以变量、函数、类是跨行留得住的（跟 python 手敲的感觉一致），
 * 而前面那些行的 print 不会重复刷屏。
 * 编译/运行失败的那行不会进缓冲——写错了不污染已有的状态。 */

fn run_capture(src: &str) -> Result<String, String> {
    // RunOutcome 里有 Rc，不是 Send：线程里只把 output/错误消息（String）带出来
    let owned = src.to_string();
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || match float_lang::api::run_from("<repl>", &owned, None) {
            Ok(o) => Ok(o.output),
            Err(e) => Err(e.message),
        })
        .unwrap()
        .join()
        .unwrap_or_else(|_| Err("交互执行崩了（panic）".to_string()))
}

/// 像不像一个"表达式"：没赋值、没块、没声明关键字——那就试着当 print 的参数
fn looks_like_expr(line: &str) -> bool {
    let t = line.trim();
    if t.is_empty() {
        return false;
    }
    let first = t.split_whitespace().next().unwrap_or("");
    const STMT_HEADS: &[&str] = &[
        "int", "float", "str", "bool", "list", "dict", "fn", "class", "if", "elif", "else",
        "while", "for", "repeat", "return", "break", "continue", "import", "print", "ask",
        "try", "catch", "throw", "data", "let", "var", "const", "报告", "输出",
    ];
    if STMT_HEADS.contains(&first) {
        return false;
    }
    // 赋值（= / += / -= ...）不算表达式
    !t.contains('=')
}

fn repl() {
    use std::io::{BufRead, IsTerminal, Write};
    let tty = std::io::stdin().is_terminal();
    println!("Copper 交互模式（输入 exit 退出，输入 help 看提示）");
    let mut buf: Vec<String> = Vec::new();
    let mut printed: usize = 0;
    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();
    loop {
        if tty {
            print!(">>> ");
            let _ = std::io::stdout().flush();
        }
        let raw = match lines.next() {
            Some(Ok(l)) => l,
            _ => {
                if tty {
                    println!();
                }
                break;
            }
        };
        let line = raw.trim_end_matches(['\r', '\n']).to_string();
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        match t {
            "exit" | "quit" | "退出" | "q" => break,
            "help" | "?" => {
                println!("  help      这些提示");
                println!("  exit      退出");
                println!("  变量/函数/类 跨行留着；表达式会自动打印结果");
                println!("  print(...) 自己打印；写错的行不会留在状态里");
                continue;
            }
            _ => {}
        }

        // 先按"表达式"试：包一层 print，值就直接显示出来（跟 python 手敲一样）；
        // 不像表达式（声明/赋值/控制流）就原样当语句跑。
        let attempts: Vec<String> = if looks_like_expr(t) {
            vec![format!("print({})", t), line.clone()]
        } else {
            vec![line.clone()]
        };

        let mut done = false;
        let mut last_err = String::new();
        for (i, stmt) in attempts.iter().enumerate() {
            let mut cand: Vec<String> = buf.clone();
            cand.push(stmt.clone());
            let src = cand.join("\n");
            match run_capture(&src) {
                Ok(out) => {
                    if out.len() > printed {
                        print!("{}", &out[printed..]);
                        let _ = std::io::stdout().flush();
                    }
                    printed = out.len();
                    buf = cand;
                    done = true;
                    break;
                }
                Err(e) => last_err = e,
            }
            // 第一个（print 包裹）失败时继续试原样语句；原样也失败就报错
            let _ = i;
        }
        if !done {
            println!("出错：{}", last_err);
        }
    }
}
