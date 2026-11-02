/* 手动排查用：把一个真实工程喂给内核，模拟 ask 输入，打印输出与错误。
 * 用法：cargo run --release --example run_game -- <工程目录> <入口.cw> <输入1> <输入2> ...
 * 内核在等输入时会自动喂 inputs 里的下一行（用完了给空串）。 */
use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("用法: run_game <工程目录> <入口.cw> [输入...]");
        std::process::exit(2);
    }
    let base = std::path::PathBuf::from(&args[1]);
    let entry = args[2].clone();
    let inputs: Vec<String> = args[3..].to_vec();
    let src = std::fs::read_to_string(base.join(&entry)).expect("读入口文件");

    let t = std::time::Instant::now();
    let r = float_lang::api::run_interactive(&entry, &src, Some(&base), inputs, Default::default());
    for l in r.output.lines() {
        if !l.trim().is_empty() {
            println!("{l}");
        }
    }
    if let Some(e) = &r.error {
        // 走 Display：普通行带「第 N 行：」，第 27 行会变成「喵～第 27 行瘸了…」
        println!("\n[运行出错] {}", e);
    } else if r.need_input.is_some() {
        println!("\n[停在提问：{}]", r.need_input.unwrap());
    } else if r.outcome.is_some() {
        println!("\n[正常结束]");
    }
    let _ = std::io::stdout().flush();
    println!("[耗时 {:?}]", t.elapsed());
}
