//! 「Rust 内核追平 TS 兼容引擎」的守门测试。
//!
//! 这里每一个用例都对应 TS 引擎里能跑、而 Rust 内核以前跑不了（或者行为不一样）的东西：
//! range 内置、列表/字典/字符串方法、wait until、if out、time read/reset、
//! 以及 push/insert/join/remove 的返回值对齐。

use float_lang::run_text;

fn out(src: &str) -> String {
    match run_text(src) {
        Ok(s) => s,
        Err(e) => panic!("运行失败：{}", e),
    }
}

/* ───────── range ───────── */

#[test]
fn range_三种写法() {
    assert_eq!(out("print(range(5))"), "[0, 1, 2, 3, 4]\n");
    assert_eq!(out("print(range(2, 5))"), "[2, 3, 4]\n");
    assert_eq!(out("print(range(0, 10, 3))"), "[0, 3, 6, 9]\n");
    assert_eq!(out("print(range(5, 0, -2))"), "[5, 3, 1]\n");
    // 全是整数就产出整数，不能变成 0.0 1.0
    assert_eq!(out("print(type(range(3)[0]))"), "int\n");
    assert_eq!(out("print(range(0))"), "[]\n");
}

#[test]
fn range_能做列表运算() {
    assert_eq!(out("list xs = range(4)\nprint(xs.sum())"), "6\n");
    assert_eq!(out("list xs = range(3)\nxs.push(99)\nprint(len(xs))"), "4\n");
    assert_eq!(out("print(len(range(1, 1000)))"), "999\n");
}

#[test]
fn range_的错误说得清楚() {
    let e = match run_text("print(range(1, 5, 0))") {
        Err(e) => e.message,
        Ok(_) => panic!("步长 0 该报错"),
    };
    assert!(e.contains("步长"), "{}", e);
    let e = match run_text("print(range(1, 2, 3, 4))") {
        Err(e) => e.message,
        Ok(_) => panic!("参数太多该报错"),
    };
    assert!(e.contains("三个参数"), "{}", e);
}

/* ───────── 方法：TS 有、Rust 以前没有的 ───────── */

#[test]
fn 列表的_add_clear() {
    assert_eq!(out("list xs = [1]\nprint(xs.add(2))\nprint(xs)"), "2\n[1, 2]\n");
    // clear 返回 0，清空列表
    assert_eq!(out("list xs = [1, 2, 3]\nprint(xs.clear())\nprint(len(xs))"), "0\n0\n");
}

#[test]
fn 字典的_items_set() {
    assert_eq!(
        out("dict d = {\"a\": 1}\nd.set(\"b\", 2)\nprint(d[\"b\"])"),
        "2\n"
    );
    // items() 给 [[键, 值], …]
    assert_eq!(
        out("dict d = {\"a\": 1}\nfor kv in d.items()\n    print(kv[0], kv[1])"),
        "a 1\n"
    );
    assert_eq!(out("dict d = {\"a\": 1}\nprint(len(d.items()))"), "1\n");
}

#[test]
fn 字符串的_includes_substring() {
    assert_eq!(out("print(\"hello\".includes(\"ell\"))"), "true\n");
    assert_eq!(out("print(\"hello\".substring(1, 3))"), "ell\n");
}

/* ───────── 返回值对齐 TS ───────── */

#[test]
fn push_insert_返回新长度() {
    assert_eq!(out("list xs = []\nprint(xs.push(1))\nprint(xs.push(2))"), "1\n2\n");
    assert_eq!(out("list xs = [1, 3]\nprint(xs.insert(1, 2))\nprint(xs)"), "3\n[1, 2, 3]\n");
}

#[test]
fn join_默认空串() {
    assert_eq!(out("list xs = [\"a\", \"b\"]\nprint(xs.join())"), "ab\n");
    assert_eq!(out("list xs = [\"a\", \"b\"]\nprint(xs.join(\"-\"))"), "a-b\n");
}

#[test]
fn 字典_remove_返回旧值() {
    assert_eq!(
        out("dict d = {\"a\": 7}\nprint(d.remove(\"a\"))\nprint(d.has(\"a\"))"),
        "7\nfalse\n"
    );
}

/* ───────── wait until ───────── */

#[test]
fn wait_until_条件成立才往下走() {
    let src = r#"
int i = 0
wait until (i >= 3)
    i++
print(i)
"#;
    assert_eq!(out(src), "3\n");
}

#[test]
fn wait_until_里的_break_能提前出来() {
    let src = r#"
int i = 0
wait until (i >= 100)
    i++
    if (i == 2)
        break
print(i)
"#;
    assert_eq!(out(src), "2\n");
}

#[test]
fn wait_until_条件一开始就成立就一次也不跑() {
    let src = r#"
int i = 0
wait until (true)
    i = 999
print(i)
"#;
    assert_eq!(out(src), "0\n");
}

/* ───────── if out ───────── */

#[test]
fn if_out_条件成立就跳出循环() {
    let src = r#"
for i in range(10)
    if out (i == 3)
    print(i)
print("done")
"#;
    // i == 3 那一圈第一句就跳出去了，所以只打出 0 1 2
    assert_eq!(out(src), "0\n1\n2\ndone\n");
}

#[test]
fn if_out_在_while_里也行() {
    let src = r#"
int i = 0
while (true)
    i++
    if out (i > 2)
print(i)
"#;
    assert_eq!(out(src), "3\n");
}

/* ───────── time read / time reset ───────── */

#[test]
fn time_read_给出秒数并写进__last_time() {
    let out = out("time reset\ntime read\nprint(type(__last_time))\nprint(__last_time >= 0)");
    assert_eq!(out, "float\ntrue\n");
}

#[test]
fn time_reset_之后读数归零() {
    // 先跑点活，再 reset，读数应该回到接近 0
    let src = r#"
int s = 0
for i in range(20000)
    s += i
time reset
time read
print(__last_time < 1.0)
"#;
    assert_eq!(out(src), "true\n");
}

#[test]
fn time_写错会报错() {
    let e = match run_text("time fly") {
        Err(e) => e.message,
        Ok(_) => panic!("该报错"),
    };
    assert!(e.contains("time read") || e.contains("time reset"), "{}", e);
}

/* ───────── 字符串插值（TS 有、Rust 以前没有） ───────── */

#[test]
fn 字符串插值() {
    assert_eq!(out("int x = 7\nprint(\"值是 {x} 结束\")"), "值是 7 结束\n");
    assert_eq!(out("int x = 3\nprint(\"a{x}b{x}c\")"), "a3b3c\n");
    // 插值里可以写表达式
    assert_eq!(out("print(\"价格 {1 + 2} 元\")"), "价格 3 元\n");
    // \{ \} 是转义，不插值
    assert_eq!(out("print(\"花括号 \\{x\\} 不插值\")"), "花括号 {x} 不插值\n");
    // 下标表达式也认（字符串里的引号不会把插值截断）
    assert_eq!(out("dict d = {\"k\": 5}\nprint(\"值 {d[\\\"k\\\"]}\")"), "值 5\n");
    // 没有花括号的字符串不受影响
    assert_eq!(out("print(\"纯文本\")"), "纯文本\n");
}

/* ───────── 三元表达式与整除 ───────── */

#[test]
fn 三元表达式() {
    assert_eq!(out("int a = 3\nprint(a > 0 ? \"正\" : \"负\")"), "正\n");
    assert_eq!(out("int a = 3\nprint(a > 10 ? \"大\" : \"小\")"), "小\n");
    // 右结合：嵌套三元
    assert_eq!(
        out("int n = 0\nprint(n < 0 ? \"负\" : n == 0 ? \"零\" : \"正\")"),
        "零\n"
    );
    // 只求值被选中的那一支（另一支抛错也不会触发）
    assert_eq!(
        out("fn 炸()\n    throw \"不该被求值\"\n    return 1\nprint(true ? \"安全\" : 炸())"),
        "安全\n"
    );
    // 三元里可以放函数调用
    assert_eq!(
        out("fn twice(x)\n    return x * 2\nprint(1 > 0 ? twice(3) : twice(4))"),
        "6\n"
    );
}

#[test]
fn 三元缺冒号会报错() {
    let e = match run_text("print(1 > 0 ? \"a\" \"b\")") {
        Err(e) => e.message,
        Ok(_) => panic!("该报错"),
    };
    assert!(e.contains("冒号"), "{}", e);
}

#[test]
fn 整除() {
    assert_eq!(out("print(idiv(7, 2))"), "3\n");
    // 向下取整（和 Python 的 // 一致）
    assert_eq!(out("print(idiv(-7, 2))"), "-4\n");
    assert_eq!(out("print(idiv(8, 4))"), "2\n");
    // 带小数：结果还是向下取整
    assert_eq!(out("print(idiv(7.5, 2))"), "3\n");
    let e = match run_text("print(idiv(1, 0))") {
        Err(e) => e.message,
        Ok(_) => panic!("除 0 该报错"),
    };
    assert!(e.contains("idiv"), "{}", e);
}

// ───────── 注释：斜杠星到行尾 ─────────

#[test]
fn 斜杠星是整行注释() {
    assert_eq!(
        out("/* 这是注释\nint a = 1\n/* 又一行注释\nprint(a)"),
        "1\n"
    );
    // 缩进的注释
    assert_eq!(
        out("if (true)\n    /* 块里的注释\n    print(\"ok\")"),
        "ok\n"
    );
    // 注释行不影响行号
    let e = match run_text("/* 注释\n/* 注释\nprint(1 / 0)") {
        Err(e) => e,
        Ok(_) => panic!("该报错"),
    };
    assert_eq!(e.span.line, 3);
}

#[test]
fn 两个减号不影响自减() {
    assert_eq!(out("int i = 3\ni--\nprint(i)"), "2\n");
    assert_eq!(out("int i = 0\ni--\nprint(i)"), "-1\n");
    assert_eq!(out("int i = 3\ni -= 1\nprint(i)"), "2\n");
    // 行首写 `--i`（不是合法语法）：必须明确报错，不能被当成注释静默跳过
    let e = match run_text("int i = 3\n--i\nprint(i)") {
        Err(e) => e.message,
        Ok(_) => panic!("--i 不合法，应该报错而不是当注释"),
    };
    assert!(e.contains("--"), "{}", e);
}

#[test]
fn 行尾注释用斜杠星() {
    assert_eq!(out("int a = 1 /* 行尾注释\nprint(a)"), "1\n");
    // 字符串里的 // 和 -- 都不算注释/运算符
    assert_eq!(out("print(\"http://a.b\")"), "http://a.b\n");
    assert_eq!(out("print(\"a--b\")"), "a--b\n");
    // 老的 -- 注释：报错并提示改成 /*
    let m = match run_text("int a = 1 -- 老式注释\nprint(a)") {
        Ok(_) => panic!("老 -- 注释该报错"),
        Err(e) => e.message,
    };
    assert!(m.contains("/*"), "报错要提示新写法：{}", m);
}

#[test]
fn 两个斜杠是整除运算符() {
    assert_eq!(out("print(7 // 2)"), "3\n");
    assert_eq!(out("print(-7 // 2)"), "-4\n");
    assert_eq!(out("print(7 // -2)"), "-4\n");
    assert_eq!(out("print(7.5 // 2)"), "3\n");
    // 优先级和 * / % 一样
    assert_eq!(out("print(1 + 7 // 2)"), "4\n");
    assert_eq!(out("print(7 // 2 * 2)"), "6\n");
    // 和函数写法等价
    assert_eq!(out("print(idiv(7, 2), 7 // 2)"), "3 3\n");
    let e = match run_text("print(1 // 0)") {
        Err(e) => e.message,
        Ok(_) => panic!("除 0 该报错"),
    };
    assert!(e.contains("整除"), "{}", e);
}
