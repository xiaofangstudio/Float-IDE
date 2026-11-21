//! 跑一个 .cw 脚本：`cargo run --release --example run -- 路径 [输入文件]`
//!
//! 输入文件按行给 `ask` 的答案；不给就用同名 .in 文件（存在的话）。
//! 编译 + 运行套在 64MB 大栈线程上：深递归、深嵌套解析都不爆栈
//! （和 IDE 的 lang_run、AOT 生成程序的大栈线程同一策略）。

use float_lang::Vm;
use std::env;
use std::fs;

#[path = "support/repl_ui.rs"]
mod repl_ui;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if matches!(
        args.first().map(|s| s.as_str()),
        Some("help") | Some("--help") | Some("-h")
    ) {
        let mut reader = repl_ui::InputReader::new();
        repl_ui::help_command(&mut reader, None);
        return;
    }
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

/// 这一行是否含“赋值号”（= / += / -= / *= / /= / %=），而不是比较号。
/// 只看字符串外、括号外的 `=`；`==`、`!=`、`<=`、`>=` 都算比较，不算赋值。
fn has_assignment_operator(line: &str) -> bool {
    let mut chars = line.chars().peekable();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut depth = 0usize;
    let mut prev = '\0';
    while let Some(c) = chars.next() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => quote = Some(c),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            '=' => {
                let next = chars.peek().copied();
                let comparison = next == Some('=') || matches!(prev, '!' | '<' | '>');
                if next == Some('=') {
                    chars.next();
                }
                if !comparison && depth == 0 {
                    return true;
                }
            }
            _ => {}
        }
        prev = c;
    }
    false
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
    // 赋值（= / += / -= ...）不算表达式；比较（== / != / <= / >=）仍然算表达式
    !has_assignment_operator(t)
}

/// 清洗终端送来的这一行：去掉 ANSI 控制序列、BOM/零宽字符、裸 CR 和 NUL，
/// 并剥掉可能被终端回显进来的 `>>>` 提示符。超长输出时提前键入，最容易把
/// 提示符/光标位置搞乱，这里先把非源码字符清掉再解析。
fn clean_repl_line(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut it = raw.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\u{1b}' {
            match it.peek().copied() {
                Some('[') => {
                    it.next();
                    // CSI：一直吃到 0x40..0x7e 的终止字符
                    while let Some(n) = it.next() {
                        if ('@'..='~').contains(&n) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    it.next();
                    // OSC：吃到 BEL 或 ESC \
                    while let Some(n) = it.next() {
                        if n == '\u{7}' {
                            break;
                        }
                        if n == '\u{1b}' && it.peek() == Some(&'\\') {
                            it.next();
                            break;
                        }
                    }
                }
                _ => {
                    it.next();
                }
            }
            continue;
        }
        if c == '\r' || c == '\n' || c == '\0' || c == '\u{feff}' || c == '\u{200b}'
            || c == '\u{200c}' || c == '\u{200d}'
        {
            continue;
        }
        if c.is_control() && c != '\t' {
            continue;
        }
        out.push(c);
    }
    // 如果提示符被错误地并进了输入，取最后一个 >>> 后面的内容
    if let Some(p) = out.rfind(">>>") {
        out = out[p + 3..].to_string();
    }
    out.trim_end().to_string()
}

/// `print(...)` 后面只剩残留数字或多余右括号时，返回可执行的有效前缀。
/// 这是给超长数字输出/终端粘行擦屁股，正常 `print(1) x` 不会命中（x 不是数字）。
fn print_call_prefix(line: &str) -> Option<String> {
    let s = line.trim_start();
    let rest = s.strip_prefix("print")?;
    if !rest.starts_with('(') {
        return None;
    }
    let open = s.find('(')?;
    let mut depth = 0usize;
    let mut in_str = false;
    let mut esc = false;
    let mut end = None;
    for (i, c) in s[open..].char_indices() {
        if in_str {
            if esc {
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '(' => depth += 1,
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    end = Some(open + i + c.len_utf8());
                    break;
                }
            }
            _ => {}
        }
    }
    let end = end?;
    let tail = s[end..].trim();
    if tail.is_empty() {
        return None;
    }
    let digit_count = tail.chars().filter(|c| c.is_ascii_digit()).count();
    let only_tail = tail
        .chars()
        .all(|c| c.is_ascii_digit() || c == ')' || c.is_whitespace());
    if only_tail && (digit_count >= 8 || tail.starts_with(')')) {
        Some(s[..end].to_string())
    } else {
        None
    }
}

fn repl() {
    let mut reader = repl_ui::InputReader::new();
    println!("Copper 交互模式（输入 help 看分章节帮助，输入 exit 退出）");
    let mut buf: Vec<String> = Vec::new();
    let mut printed: usize = 0;
    let mut at_line_start = true;
    loop {
        if reader.is_tty() {
            // 上一轮输出如果没以换行结束（超长数字被终端截断、或提前键入），
            // 先把提示符顶到行首，避免 "数字>>> print(...)" 粘成一行。
            if !at_line_start {
                println!();
            }
            let _ = std::io::Write::flush(&mut std::io::stdout());
        }
        let raw = match reader.read_line(">>> ", true) {
            Some(l) => l,
            None => {
                if reader.is_tty() {
                    println!();
                }
                break;
            }
        };
        let line = clean_repl_line(&raw);
        at_line_start = true;
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        match t {
            "exit" | "quit" | "退出" | "q" => break,
            "help" | "?" => {
                repl_ui::help_command(&mut reader, None);
                at_line_start = true;
                continue;
            }
            _ => {}
        }
        if let Some(arg) = t.strip_prefix("help ") {
            repl_ui::help_command(&mut reader, Some(arg));
            at_line_start = true;
            continue;
        }

        // 先按"表达式"试：包一层 print，值就直接显示出来（跟 python 手敲一样）；
        // 不像表达式（声明/赋值/控制流）就原样当语句跑。
        let attempts: Vec<String> = if looks_like_expr(t) {
            vec![format!("print({})", t), line.clone()]
        } else {
            vec![line.clone()]
        };
        let mut attempts = attempts;
        if let Some(prefix) = print_call_prefix(&line) {
            if !attempts.iter().any(|a| a == &prefix) {
                attempts.push(prefix);
            }
        }

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
                        at_line_start = out.ends_with('\n');
                        let _ = std::io::Write::flush(&mut std::io::stdout());
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
            at_line_start = true;
        }
    }
}

#[cfg(test)]
mod repl_line_tests {
    use super::*;

    #[test]
    fn comparisons_are_expressions() {
        for line in [
            "a == 1",
            "a != 1",
            "a <= 1",
            "a >= 1",
            "a==1",
            "f(a == 1)",
            "s == \"x=y\"",
        ] {
            assert!(looks_like_expr(line), "应当按表达式执行：{line}");
        }
    }

    #[test]
    fn assignments_are_statements() {
        for line in [
            "a = 1",
            "a=1",
            "a += 1",
            "a -= 1",
            "a *= 1",
            "a /= 1",
            "a %= 1",
            "a = f(x == 1)",
            "s = \"a=b\"",
            "int a",
        ] {
            assert!(!looks_like_expr(line), "应当按语句执行：{line}");
        }
    }
}
