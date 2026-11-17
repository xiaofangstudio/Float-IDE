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
    if args.is_empty() {
        eprintln!("用法：run <脚本.cw> [输入文件]");
        std::process::exit(2);
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
                    eprintln!("—— 用时 {:.2} ms", t.elapsed().as_secs_f64() * 1000.0);
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
