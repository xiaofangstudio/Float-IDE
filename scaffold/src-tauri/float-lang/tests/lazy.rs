//! lazy 参数（by-name）与 AND2/OR2 短路：Rust VM 专属能力。
//! TS 兼容引擎不认识 lazy 语法，所以这些用例不进 parity，只在这里守门。

use float_lang::run_text;

fn out(src: &str) -> String {
    run_text(src).expect("应该能跑").trim().to_string()
}

#[test]
fn 短路_第二个参数不求值() {
    let src = r#"
fn bool AND2(bool x, lazy bool y)
    if x
        if y
            return true
    return false
function side(tag, v)
    print("eval", tag)
    return v
print(AND2(false, side("A", true)))
print(AND2(true, side("B", true)))
"#;
    // A 不该被求值；B 求值在第二次 AND2 里，先于它的返回值打印
    assert_eq!(out(src), "false\neval B\ntrue");
}

#[test]
fn or2_短路() {
    let src = r#"
fn bool OR2(bool x, lazy bool y)
    if x
        return true
    if y
        return true
    return false
function side(tag, v)
    print("eval", tag)
    return v
print(OR2(true, side("A", false)))
print(OR2(false, side("B", false)))
"#;
    assert_eq!(out(src), "true\neval B\nfalse");
}

#[test]
fn by_name_每次读取重新求值() {
    let src = r#"
int n = 0
function tick()
    n += 1
    return n
function twice(lazy x)
    return x + x
print(twice(tick()))
print(n)
"#;
    // tick 被调用两次：1 + 2 = 3，n 最终是 2
    assert_eq!(out(src), "3\n2");
}

#[test]
fn 用不到就不求值() {
    let src = r#"
int n = 0
function tick()
    n += 1
    return n
function choose(lazy x, c)
    if c
        return x
    return 0
print(choose(tick(), false))
print(n)
"#;
    assert_eq!(out(src), "0\n0");
}

#[test]
fn lazy_参数捕获调用处变量() {
    let src = r#"
function later(lazy x)
    return x
function make()
    int v = 7
    return later(v + 1)
print(make())
"#;
    assert_eq!(out(src), "8");
}

#[test]
fn lazy_参数不能赋值() {
    let src = r#"
function bad(lazy x)
    x = 5
    return x
print(bad(1))
"#;
    assert!(run_text(src).is_err(), "给 lazy 参数赋值应该报编译错误");
}
