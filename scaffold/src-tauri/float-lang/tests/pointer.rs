//! 真指针（& / * / 指针运算）：Rust 内核（F5）必须和 TS 解释器同语义。

use float_lang::run_text;

fn out(src: &str) -> String {
    run_text(src).expect("应该能跑").trim().to_string()
}

#[test]
fn 取地址与解引用() {
    assert_eq!(out("int x = 10\nptr p = &x\nprint(*p)\n"), "10");
}

#[test]
fn 通过指针写回变量() {
    assert_eq!(out("int x = 10\nptr p = &x\n*p = 42\nprint(x)\n"), "42");
    assert_eq!(out("int n = 1\nptr q = &n\n*q += 5\nprint(n)\n"), "6");
}

#[test]
fn 列表元素的地址() {
    assert_eq!(
        out("list xs = [1, 2, 3]\nptr p = &xs[1]\nprint(*p)\n*p = 9\nprint(xs[1])\n"),
        "2\n9"
    );
}

#[test]
fn 指针运算() {
    let src = "list ys = [10, 20, 30]\nptr a = &ys[0]\nprint(*(a + 1))\nprint(*(a + 2))\n\
               ptr b = a + 2\nprint(*(b - 1))\nprint(b - a)\n";
    assert_eq!(out(src), "20\n30\n20\n2");
}

#[test]
fn 相对下标() {
    let src = "list ys = [10, 20, 30]\nptr c = &ys[0]\nprint(c[1])\nptr d = c + 2\nprint(d[-1])\n";
    assert_eq!(out(src), "20\n20");
}

#[test]
fn alloc_老用法不回归() {
    let src = "ptr p = alloc(3)\np[0] = 7\np[1] = p[0] * 2\nprint(p[0], p[1], len(p))\nfree(p)\n";
    assert_eq!(out(src), "7 14 3");
}

#[test]
fn 遍历指针从当前位置开始() {
    let src = "ptr e = alloc(3)\ne[0] = 1\ne[1] = 2\ne[2] = 3\nint s = 0\nfor v in e\n    s += v\n\
               print(s)\nptr f = e + 1\nint s2 = 0\nfor v in f\n    s2 += v\nprint(s2)\nfree(e)\n";
    assert_eq!(out(src), "6\n5");
}

#[test]
fn 函数参数用指针修改调用方() {
    let src = "fn bump(ptr p)\n    *p = *p + 1\n\nint z = 41\nbump(&z)\nprint(z)\n";
    assert_eq!(out(src), "42");
}

#[test]
fn 悬空指针给中文报错() {
    let e = run_text("fn f()\n    int x = 1\n    return &x\n\nptr p = f()\nprint(*p)\n").unwrap_err();
    assert!(e.message.contains("悬空") || e.message.contains("作用域"), "实际：{}", e.message);
}

#[test]
fn 越界不踩内存() {
    let e = run_text("ptr p = alloc(2)\nprint(*(p + 5))\nfree(p)\n").unwrap_err();
    assert!(e.message.contains("越界"), "实际：{}", e.message);
}

#[test]
fn 非指针解引用要点名() {
    let e = run_text("int x = 3\nprint(*x)\n").unwrap_err();
    assert!(e.message.contains("指针"), "实际：{}", e.message);
}

#[test]
fn free_列表地址要被拒绝() {
    let e = run_text("list xs = [1]\nptr p = &xs[0]\nfree(p)\n").unwrap_err();
    assert!(e.message.contains("不是 alloc") || e.message.contains("不能 free"), "实际：{}", e.message);
}

#[test]
fn mod_关键字() {
    assert_eq!(out("print(7 mod 3)\nprint(10 mod 4)\n"), "1\n2");
}

#[test]
fn if_链中间夹注释不断链() {
    let src = "int k = 2\nif k == 1\n    print(\"one\")\n-- 插一句注释\nelif k == 2\n    print(\"two\")\nelse\n    print(\"other\")\n";
    assert_eq!(out(src), "two");
}

#[test]
fn 多级指针_前缀两个星号就是解引用两次() {
    assert_eq!(out("int x = 7\nptr p = &x\nptr pp = &p\nprint(**pp)\n"), "7");
    assert_eq!(out("int x = 7\nptr p = &x\nptr pp = &p\n**pp = 9\nprint(x)\n"), "9");
    // 幂运算不能被这层改动带坏：右结合、指数对
    assert_eq!(out("print(2 ** 10)\nprint(2 ** 3 ** 2)\n"), "1024\n512");
}
