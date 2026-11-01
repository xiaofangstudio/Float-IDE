// 对拍用：把 cases 文件里每段脚本用 Rust 内核跑一遍，结果写 rust.json
// 用法：CASES=cases2.txt cargo run --release --example parity
use std::io::Write;

fn main() {
    let case_file = std::env::var("CASES").unwrap_or_else(|_| "cases.txt".to_string());
    let text = std::fs::read_to_string(format!("../../tests/parity/{}", case_file)).expect("读用例");
    let mut out_json = String::from("{\n");
    let mut first = true;
    let mut flush = |cur: &mut Option<(String, Vec<String>)>, out_json: &mut String, first: &mut bool| {
        if let Some((name, lines)) = cur.take() {
            let src = lines.join("\n") + "\n";
            let (out, err) = match float_lang::run_text(&src) {
                Ok(o) => (o.lines().map(|s| s.to_string()).collect::<Vec<_>>(), vec![]),
                Err(e) => (vec![], vec![e.message]),
            };
            if !*first {
                out_json.push_str(",\n");
            }
            *first = false;
            out_json.push_str(&format!("  {:?}: {{\"out\": {:?}, \"err\": {:?}}}", name, out, err));
        }
    };
    let mut cur: Option<(String, Vec<String>)> = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("### ") {
            flush(&mut cur, &mut out_json, &mut first);
            cur = Some((rest.trim().to_string(), Vec::new()));
        } else if let Some((_, lines)) = cur.as_mut() {
            lines.push(line.to_string());
        }
    }
    flush(&mut cur, &mut out_json, &mut first);
    out_json.push_str("\n}\n");
    let mut f = std::fs::File::create("../../tests/parity/rust.json").expect("写文件");
    f.write_all(out_json.as_bytes()).unwrap();
    println!("Rust 侧完成：{}", case_file);
}
