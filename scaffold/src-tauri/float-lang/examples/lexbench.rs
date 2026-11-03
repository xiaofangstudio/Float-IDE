// 词法 vs 语法各自的耗时（定位大文件解析热点）
use std::time::Instant;

fn main() {
    let path = std::env::args().nth(1).expect("用法");
    let src = std::fs::read_to_string(&path).expect("读文件");
    let line = src.lines().max_by_key(|l| l.len()).unwrap_or("").to_string();
    let t0 = Instant::now();
    let toks = float_lang::token::tokenize_line(&line, 2, 1).expect("词法");
    let t_lex = t0.elapsed();
    println!("词法：{:?}（{} 个 token，{} 字符）", t_lex, toks.len(), line.chars().count());
    let t1 = Instant::now();
    let prog = float_lang::parser::parse_program(&path, &src);
    println!("整体解析：{:?}（{} 条语句）", t1.elapsed(), prog.body.len());
}
