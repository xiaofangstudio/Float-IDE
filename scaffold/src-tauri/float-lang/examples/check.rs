//! 静态检查：`cargo run --release --example check -- 文件.cw`
//!
//! 只跑词法 / 语法 / 语义分析，不执行脚本。给 IDE 的「问题」面板做参考。

use float_lang::{bytecode, parser, sema};
use std::env;
use std::fs;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("用法：check <脚本.cw>");
        return;
    }
    let mut total = 0usize;
    for path in &args {
        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("读不到 {}：{}", path, e);
                continue;
            }
        };
        println!("── {}", path);
        let prog = parser::parse_program(path, &src);
        if !prog.errors.is_empty() {
            for e in &prog.errors {
                println!("第 {} 行  错误：{}", e.span.line, e.message);
                total += 1;
            }
            continue;
        }
        let (chunk, _f, errs) = bytecode::compile(&prog);
        for e in &errs {
            println!("第 {} 行  错误：{}", e.span.line, e.message);
            total += 1;
        }
        let ds = sema::analyze(&prog);
        for d in &ds {
            println!("{}", d.render());
            total += 1;
        }
        if errs.is_empty() && ds.is_empty() {
            println!("没问题（{} 条指令）", chunk.code.len());
        }
    }
    if total > 0 {
        println!("\n共 {} 条", total);
    }
}
