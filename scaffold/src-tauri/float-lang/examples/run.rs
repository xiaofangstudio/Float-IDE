//! 跑一个 .cw 脚本：`cargo run --release --example run -- 路径 [输入文件]`
//!
//! 输入文件按行给 `ask` 的答案；不给就用同名 .in 文件（存在的话）。

use float_lang::{bytecode, parser, Vm};
use std::env;
use std::fs;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("用法：run <脚本.cw> [输入文件]");
        std::process::exit(2);
    }
    let path = &args[0];
    let src = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("读不到 {}：{}", path, e);
            std::process::exit(1);
        }
    };
    let input_path = if args.len() > 1 {
        Some(args[1].clone())
    } else {
        let guess = path.replace(".cw", ".in");
        if fs::metadata(&guess).is_ok() {
            Some(guess)
        } else {
            None
        }
    };
    let inputs: Vec<String> = match input_path {
        Some(p) => fs::read_to_string(&p)
            .unwrap_or_default()
            .lines()
            .map(|l| l.to_string())
            .collect(),
        None => Vec::new(),
    };

    let prog = parser::parse_program(path, &src);
    if let Some(e) = prog.errors.first() {
        eprintln!("语法错误（第 {} 行）：{}", e.span.line, e.message);
        std::process::exit(1);
    }
    let (chunk, _f, errs) = bytecode::compile(&prog);
    if let Some(e) = errs.first() {
        eprintln!("编译错误：{}", e.message);
        std::process::exit(1);
    }
    let mut vm = Vm::new(chunk);
    vm.set_inputs(inputs);
    let t = std::time::Instant::now();
    match vm.run() {
        Ok(o) => {
            print!("{}", o.output);
            eprintln!("—— 用时 {:.2} ms", t.elapsed().as_secs_f64() * 1000.0);
        }
        Err(e) => {
            print!("{}", vm.take_output());
            eprintln!("\n运行出错：{}", e.message);
            std::process::exit(1);
        }
    }
}
