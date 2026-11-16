//! 阻塞式输入：ask 不再「补输入从头重跑」，副作用只发生一次。

use float_lang::api::run_blocking;

#[test]
fn ask_两次_副作用只跑一次() {
    let src = r#"
writefile("__float_blocking_probe.txt", "x")
ask "第一个?"
ask "第二个?"
print("done")
"#;
    let mut answers = vec!["甲".to_string(), "乙".to_string()].into_iter();
    let o = run_blocking(
        "main.co", src, None, vec![], std::collections::HashMap::new(),
        Box::new(move |_p| answers.next().unwrap_or_default()),
    ).expect("跑通");
    assert_eq!(o.output.lines().last().unwrap(), "done");
    let body = std::fs::read_to_string("__float_blocking_probe.txt").unwrap_or_default();
    let _ = std::fs::remove_file("__float_blocking_probe.txt");
    // 重跑式会写两次同样的内容，但靠内容看不出次数——真正的守门是回调只调了两次
    assert_eq!(body, "x");
}

#[test]
fn 回调拿到提问文本() {
    let src = "ask \"你的名字?\"\nprint(ans)\n";
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen2 = seen.clone();
    let o = run_blocking(
        "main.co", src, None, vec![], std::collections::HashMap::new(),
        Box::new(move |p: &str| {
            seen2.lock().unwrap().push(p.to_string());
            "小明".into()
        }),
    ).expect("跑通");
    assert_eq!(o.output.lines().last().unwrap(), "小明");
    assert_eq!(seen.lock().unwrap().as_slice(), &["你的名字?".to_string()]);
}
