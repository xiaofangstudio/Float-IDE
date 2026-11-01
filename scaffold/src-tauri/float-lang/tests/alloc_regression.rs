//! 手动内存（alloc / free）回归测试。
//!
//! 这一组是"退出时崩进程"那个真 bug 的守门员：操作数栈以前用
//! `Vec<Value>` + 裸 `ptr::read`，被搬走的格子析构时会被 drop 第二次，
//! Rc 计数下溢把堆写坏 —— debug 下看不出来，release 跑完退出直接崩。
//! 现在栈只析构 [0, sp) 这段活值，这组用例负责确认"跑完还能好好退出"。

use float_lang::run_text;

#[test]
fn alloc_then_free_runs_clean() {
    let o = run_text("ptr p = alloc(4)\np[0] = 1\nprint(p[0])\nfree(p)\n").unwrap();
    assert_eq!(o, "1\n");
}

#[test]
fn alloc_holding_strings_and_lists() {
    // 指针里放引用类型：Rc 的增减最容易暴露双重释放
    let src = r#"
ptr p = alloc(4)
p[0] = "你好"
p[1] = [1, 2, 3]
p[2] = {"a": 1}
print(p[0])
free(p)
"#;
    let r = run_text(src);
    assert!(r.is_ok(), "{:?}", r.err().map(|e| e.message));
}

#[test]
fn alloc_leak_errors() {
    let r = run_text("ptr p = alloc(4)\np[0] = 7\nprint(p[0])\n");
    assert!(r.is_err());
}

#[test]
fn many_allocs_with_values() {
    let mut src = String::new();
    for i in 0..200 {
        src.push_str(&format!("ptr p{} = alloc(8)\np{}[0] = join(\"x\", {})\nfree(p{})\n", i, i, i, i));
    }
    src.push_str("print(\"ok\")\n");
    assert_eq!(run_text(&src).unwrap(), "ok\n");
}

#[test]
fn alloc_in_loop_with_function() {
    let src = r#"
fn 用一次(n)
    ptr p = alloc(2)
    p[0] = n * 2
    int r = p[0]
    free(p)
    return r

int s = 0
for i in range(50)
    s += 用一次(i)
print(s)
"#;
    assert_eq!(run_text(src).unwrap(), "2450\n");
}
