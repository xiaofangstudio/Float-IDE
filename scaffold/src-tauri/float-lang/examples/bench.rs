//! 性能基准：Rust VM vs V8（编译通道）vs CPython。
//!
//!   cargo run --release --example bench
//!
//! 三个用例刻意和 `scaffold/tests/bench.ts` / `bench.py` 完全一致，好横向比。

use float_lang::{bytecode, parser, Vm};
use std::time::Instant;

const FIB: &str = r#"
function fib(n)
    if (n < 2)
        return n
    return fib(n - 1) + fib(n - 2)
print(fib(30))
"#;

const LOOP: &str = r#"
int s = 0
for i in range(5000000)
    s += i
print(s)
"#;

const CONCAT: &str = r#"
str out = ""
for i in range(50000)
    out += "x"
print(len(out))
"#;

fn bench(name: &str, src: &str, repeat: usize) {
    // 编译一次
    let t0 = Instant::now();
    let prog = parser::parse_program("bench.cw", src);
    assert!(prog.errors.is_empty(), "{} 解析失败：{:?}", name, prog.errors.first());
    let (chunk, _f, errs) = bytecode::compile(&prog);
    assert!(errs.is_empty(), "{} 编译失败：{:?}", name, errs.first());
    let compile_ms = t0.elapsed().as_secs_f64() * 1000.0;

    // 预热一次
    let mut vm = Vm::new(chunk.clone());
    let warm = vm.run().expect("预热失败");
    let out_len = warm.output.trim().len();

    let mut best = f64::INFINITY;
    for _ in 0..repeat {
        let mut vm = Vm::new(chunk.clone());
        let t = Instant::now();
        let r = vm.run().expect("运行失败");
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        if ms < best {
            best = ms;
        }
        let _ = r;
    }

    println!(
        "{:<18} 编译 {:>6.2} ms   运行 {:>8.2} ms   合计 {:>8.2} ms   (输出长度 {})",
        name,
        compile_ms,
        best,
        compile_ms + best,
        out_len
    );
}

fn main() {
    println!("== float-lang 原生 VM（release）==\n");
    bench("fib(30) 递归", FIB, 5);
    bench("循环累加 500 万", LOOP, 5);
    bench("字符串拼接 5 万", CONCAT, 5);
    println!();
    println!("对照请跑 scaffold/tests/bench-compare.sh：它会 Python 与本 VM 交替跑多轮取最好成绩，");
    println!("笔记本会降频，一前一后跑出来的数字不可比。");
}
