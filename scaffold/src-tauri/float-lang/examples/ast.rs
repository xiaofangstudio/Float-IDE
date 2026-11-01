//! 只看语法树：`cargo run --example ast -- 文件.cw`
use std::env;
use std::fs;
fn main() {
    let p = env::args().nth(1).expect("用法：ast <文件.cw>");
    let src = fs::read_to_string(&p).unwrap();
    let prog = float_lang::parser::parse_program(&p, &src);
    println!("错误 {} 条", prog.errors.len());
    for e in &prog.errors { println!("  第 {} 行：{}", e.span.line, e.message); }
    for s in &prog.body { println!("{:#?}", s.kind); }
}
