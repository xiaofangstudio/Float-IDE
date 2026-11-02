//! 作用域：变量 / 列表 / 字典在块、函数、类、try 里的可见性与可改性。
//! 期望值和 TS 两个引擎（F5 编译 / F6 解释）完全一致。

use float_lang::run_text;

fn out(src: &str) -> String {
    run_text(src).expect("应该能跑").trim().to_string()
}

fn err(src: &str) -> String {
    match run_text(src) {
        Ok(v) => panic!("本该报错，实际输出：{v}"),
        Err(e) => e.message,
    }
}

#[test]
fn for_变量活到循环外() {
    assert_eq!(out("for i in range(3)\n    print(i)\nprint(i)\n"), "0\n1\n2\n2");
    assert_eq!(out("fn f()\n    for i in range(3)\n        pass2 = i\n    return i\nprint(f())\n").len() > 0, true);
}

#[test]
fn 外层同名变量被循环复用() {
    assert_eq!(out("int i = 99\nfor i in range(2)\n    print(i)\nprint(i)\n"), "0\n1\n1");
}

#[test]
fn for_in_变量活到循环外() {
    assert_eq!(out("for x in [7, 8]\n    print(x)\nprint(x)\n"), "7\n8\n8");
}

#[test]
fn 嵌套同名循环() {
    assert_eq!(out("for i in range(2)\n    for i in range(2)\n        print(i)\nprint(i)\n"), "0\n1\n0\n1\n1");
}

#[test]
fn 块内声明在块外可见_和_if_while一致() {
    assert_eq!(out("if (true)\n    int a = 1\nprint(a)\n"), "1");
    assert_eq!(out("int n = 0\nwhile n < 1\n    int b = 5\n    n += 1\nprint(b)\n"), "5");
}

#[test]
fn 函数局部不泄漏() {
    let e = err("fn f()\n    int local = 1\nf()\nprint(local)\n");
    assert!(e.contains("没有定义"), "实际：{e}");
}

#[test]
fn 全局读写与遮蔽() {
    assert_eq!(out("int g = 1\nfn f()\n    g = 9\nf()\nprint(g)\n"), "9");
    assert_eq!(out("int g = 1\nfn f()\n    int g = 2\n    return g\nprint(f())\nprint(g)\n"), "2\n1");
}

#[test]
fn 列表按引用传_重新赋值不影响外面() {
    assert_eq!(out("fn addone(list xs)\n    xs.push(9)\nlist a = [1]\naddone(a)\nprint(a)\n"), "[1, 9]");
    assert_eq!(out("fn swap(list xs)\n    xs = [7]\nlist a = [1]\nswap(a)\nprint(a)\n"), "[1]");
}

#[test]
fn 字典字段可改() {
    assert_eq!(
        out("fn bump(dict d)\n    d[\"hp\"] = d[\"hp\"] + 1\ndict a = {\"hp\": 1}\nbump(a)\nprint(a[\"hp\"])\n"),
        "2"
    );
}

#[test]
fn 类里字段与方法局部同名() {
    assert_eq!(out("class C\n    int v = 5\n    fn get()\n        return v\nprint(C().get())\n"), "5");
    assert_eq!(out("class C\n    int v = 5\n    fn get()\n        int v = 9\n        return v\nprint(C().get())\n"), "9");
}
