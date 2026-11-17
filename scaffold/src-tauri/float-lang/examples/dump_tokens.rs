//! 词法对拍基准：把一份 .co 源码的 token 流按固定格式打出来。
//!
//! 用途：自举第 2a 步——Copper 写的 lexer（bootstrap/copper_lex.co）要和
//! Rust 引擎的 tokenizer 产出**逐 token 一致**的结果。两边跑同一份源码、
//! 各自输出本工具定义的格式，diff 为空 = 自举 lexer 语义完全对齐。
//!
//! 输出格式（每个 token 一行）：
//!   行号 列号 结列 类型 文本
//! 类型名：NUM STR IDENT KW OP PUNC EOF
//! 文本统一转义：\ → \\ 、换行 → \n、制表 → \t（数字串后缀带 @值）。
//! 数字 token 的文本后附 `@<f64 显示值>`（对拍 num 字段）；无 num 不带。
//! 词法报错行打 `ERR 行 列 消息` 后终止（两边都要能对上报错位置）。

use float_lang::token::{code_part, strip_block_comments, tokenize_line, TokKind};
use float_lang::value::fmt_float;
use std::io::Write;

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            other => out.push(other),
        }
    }
    out
}

fn kind_name(k: TokKind) -> &'static str {
    match k {
        TokKind::Num => "NUM",
        TokKind::Str => "STR",
        TokKind::Ident => "IDENT",
        TokKind::Kw => "KW",
        TokKind::Op => "OP",
        TokKind::Punc => "PUNC",
        TokKind::Eof => "EOF",
    }
}

fn main() {
    let path = match std::env::args().nth(1) {
        Some(p) => p,
        None => {
            eprintln!("用法：dump_tokens <源码.co>");
            std::process::exit(2);
        }
    };
    let src = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("读不了 {}: {}", path, e);
            std::process::exit(2);
        }
    };

    let stdout = std::io::stdout();
    let mut w = std::io::BufWriter::new(stdout.lock());

    // 块注释预处理（与 parser 主流程一致：先剥块注释再逐行）
    let scan = strip_block_comments(&src);
    for (l, c, msg) in &scan.warnings {
        let _ = writeln!(w, "WARN {} {} {}", l, c, esc(msg));
    }
    for (l, c, msg) in &scan.errors {
        let _ = writeln!(w, "ERR {} {} {}", l, c, esc(msg));
    }
    if !scan.errors.is_empty() {
        return;
    }

    for (idx, raw_line) in scan.text.lines().enumerate() {
        let line_no = (idx + 1) as u32;
        // 行首缩进宽度（空格数，全角空格算 1）
        let indent = raw_line
            .chars()
            .take_while(|c| c.is_whitespace())
            .count() as u32;
        let (code, comment) = match code_part(raw_line) {
            Ok(x) => x,
            Err(e) => {
                let _ = writeln!(w, "ERR {} 1 {}", line_no, esc(&e));
                return;
            }
        };
        if let Some(cm) = comment {
            let _ = writeln!(w, "CMT {} {} {}", line_no, indent + 1, esc(cm.trim()));
        }
        let code_trim = code.trim();
        if code_trim.is_empty() {
            continue;
        }
        // 起始列 = 行首缩进宽度（按字符数算——全角空格也算 1 列，与
        // tokenize_line 的 char 索引一致；不能用字节差，全角空白会错位）
        let skip = raw_line
            .chars()
            .take_while(|c| c.is_whitespace())
            .count() as u32;
        let start_col = skip + 1;
        match tokenize_line(code_trim, line_no, start_col) {
            Ok(toks) => {
                for t in toks {
                    let num_part = match t.num {
                        // 与 VM 的浮点显示完全一致（整数收敛 / 12 位有效数字）
                        Some(v) => format!("@{}", fmt_float(v)),
                        None => String::new(),
                    };
                    let _ = writeln!(
                        w,
                        "{} {} {} {} {}{}",
                        t.line, t.col, t.end_col, kind_name(t.kind), esc(&t.text), num_part
                    );
                    if t.kind == TokKind::Eof {
                        break;
                    }
                }
            }
            Err(e) => {
                let _ = writeln!(w, "ERR {} {} {}", e.line, e.col, esc(&e.message));
                return;
            }
        }
    }
}
