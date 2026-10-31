//! 纯调度成本测量：不同循环体里每条指令的平均代价。
use float_lang::{bytecode, parser, Vm};
use std::time::Instant;

const RANGE: &str = r#"
for i in range(20000000)
    x = i
"#;

const WHILE: &str = r#"
i = 0
while (i < 20000000)
    i++
"#;

const EMPTY_BODY: &str = r#"
for i in range(20000000)
    x = 1
"#;

fn bench(name: &str, src: &str, n: usize, per_iter: usize) {
    let prog = parser::parse_program("d.cw", src);
    let (chunk, _f, errs) = bytecode::compile(&prog);
    assert!(errs.is_empty(), "{:?}", errs.first());
    let _ = Vm::new(chunk.clone()).run().unwrap();
    let mut best = f64::INFINITY;
    for _ in 0..3 {
        let mut vm = Vm::new(chunk.clone());
        let t = Instant::now();
        let _ = vm.run().unwrap();
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        if ms < best { best = ms }
    }
    println!("{:<14} {:>8.1} ms   每圈 {:>5.2} ns   每条指令约 {:>4.2} ns",
        name, best, best * 1e6 / n as f64, best * 1e6 / (n * per_iter) as f64);
}

fn main() {
    bench("for range", RANGE, 20_000_000, 3);
    bench("while i++", WHILE, 20_000_000, 4);
    bench("for 常量赋值", EMPTY_BODY, 20_000_000, 3);
}
