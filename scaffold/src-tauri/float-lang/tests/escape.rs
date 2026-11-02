//! 逃逸 / 列表扩容 / free 错地址：Rust 内核必须和 TS 引擎同语义。

use float_lang::run_text;

fn ok(src: &str) -> String {
    run_text(src).expect("应该能跑").trim().to_string()
}

fn err(src: &str) -> String {
    match run_text(src) {
        Ok(v) => panic!("本该报错，实际输出：{v}"),
        Err(e) => e.message,
    }
}

#[test]
fn 返回局部变量地址_解引用要报悬空() {
    let e = err("fn bad()\n    int y = 1\n    return &y\nptr p = bad()\nprint(*p)\n");
    assert!(e.contains("作用域") || e.contains("悬空"), "实际：{e}");
}

#[test]
fn 返回全局地址没问题() {
    assert_eq!(ok("int g = 9\nfn f()\n    return &g\nptr p = f()\nprint(*p)\n"), "9");
}

#[test]
fn 列表_push_之后_元素指针仍然有效() {
    assert_eq!(ok("list xs = [1, 2]\nptr p = &xs[0]\nxs.push(3)\nprint(*p)\nprint(len(xs))\n"), "1\n3");
    assert_eq!(ok("list xs = [1, 2]\nptr p = &xs[0]\nxs.push(3)\n*p = 99\nprint(xs)\n"), "[99, 2, 3]");
}

#[test]
fn free_栈地址要报错() {
    let e = err("int x = 1\nptr p = &x\nfree(p)\n");
    assert!(e.contains("不是 alloc") || e.contains("不能 free"), "实际：{e}");
}

#[test]
fn free_列表元素地址要报错() {
    let e = err("list xs = [1]\nptr p = &xs[0]\nfree(p)\n");
    assert!(e.contains("不是 alloc") || e.contains("不能 free"), "实际：{e}");
}

#[test]
fn 列表_pop_掉被指元素后访问要越界报错() {
    let e = err("list xs = [1, 2]\nptr p = &xs[1]\nxs.pop()\nprint(*p)\n");
    assert!(e.contains("越界"), "实际：{e}");
}

#[test]
fn 列表整体重新赋值后_指针仍指旧列表() {
    // 指针绑定的是"当时那个列表对象"，不是变量名
    assert_eq!(ok("list xs = [1, 2]\nptr p = &xs[0]\nxs = [7, 8]\nprint(*p)\nprint(xs)\n"), "1\n[7, 8]");
}
