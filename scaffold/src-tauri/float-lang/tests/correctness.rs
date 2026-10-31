//! 内核正确性排查：专门盯历史上出过问题的地方（for 循环变量、作用域、别名、
//! 提前 return 的状态清理……）。每个用例都对应一个真实踩过的坑或语义约定。

use float_lang::api::{run_source, run_text};

fn out(src: &str) -> String {
    match run_text(src) {
        Ok(s) => s,
        Err(e) => panic!("运行失败：{}\n源码：\n{}", e.message, src),
    }
}

/// 断言输出（按行比较，忽略首尾空行）
fn assert_out(src: &str, want: &[&str]) {
    let got = out(src);
    let got_lines: Vec<&str> = got.lines().collect();
    assert_eq!(got_lines, want, "\n实际输出：\n{}\n源码：\n{}", got, src);
}

/* ─────────────── for 循环变量：历史上"循环里用不了 i"的重灾区 ─────────────── */

#[test]
fn for_循环变量在体内可读() {
    assert_out(
        "for i in range(3)\n    print(i)\n",
        &["0", "1", "2"],
    );
}

#[test]
fn for_循环变量在体内可改且不改迭代() {
    // 改 i 只改这个变量，迭代进度由循环状态自己维护
    assert_out(
        "for i in range(3)\n    print(i)\n    i = i + 100\n",
        &["0", "1", "2"],
    );
}

#[test]
fn for_循环结束后变量还在() {
    assert_out("for i in range(5)\n    x = i\nprint(i)\n", &["4"]);
}

#[test]
fn 嵌套循环同名变量互不干扰() {
    assert_out(
        "for i in range(2)\n    for i in range(2)\n        print(i)\n",
        &["0", "1", "0", "1"],
    );
}

#[test]
fn 嵌套循环内外层变量都能用() {
    assert_out(
        "for i in range(2)\n    for j in range(3)\n        print(i, j)\n",
        &["0 0", "0 1", "0 2", "1 0", "1 1", "1 2"],
    );
}

#[test]
fn for_里_continue_跳过本轮() {
    assert_out(
        "for i in range(5)\n    if (i == 2)\n        continue\n    print(i)\n",
        &["0", "1", "3", "4"],
    );
}

#[test]
fn for_里_break_跳出() {
    assert_out(
        "for i in range(100)\n    if (i == 3)\n        break\n    print(i)\n",
        &["0", "1", "2"],
    );
}

#[test]
fn 内层_break_不影响外层() {
    assert_out(
        "for i in range(3)\n    for j in range(100)\n        if (j == 1)\n            break\n        print(i, j)\n",
        &["0 0", "1 0", "2 0"],
    );
}

#[test]
fn for_里套_for_in_里_break() {
    assert_out(
        "for i in range(2)\n    for x in [1, 2, 3]\n        if (x == 2)\n            break\n        print(i, x)\n",
        &["0 1", "1 1"],
    );
}

#[test]
fn 含端点循环_步长为一() {
    assert_out("for i = 1 to 3\n    print(i)\n", &["1", "2", "3"]);
}

#[test]
fn 含端点循环_步长为二不越界() {
    // 1,3,5,7,9 —— 不能把 11 也算进来
    assert_out(
        "for i = 1 to 10 step 2\n    print(i)\n",
        &["1", "3", "5", "7", "9"],
    );
}

#[test]
fn 含端点循环_负步长() {
    // 10,7,4,1 —— 不能把 -2 也算进来
    assert_out(
        "for i = 10 to 1 step -3\n    print(i)\n",
        &["10", "7", "4", "1"],
    );
}

#[test]
fn 负步长不含端点() {
    assert_out(
        "for i in range(3, 0, -1)\n    print(i)\n",
        &["3", "2", "1"],
    );
}

#[test]
fn 浮点步长循环() {
    assert_out(
        "for i = 0.0 to 1.0 step 0.5\n    print(i)\n",
        &["0", "0.5", "1"],
    );
}

#[test]
fn 空区间一圈都不跑() {
    assert_out("for i in range(0)\n    print(i)\nprint(\"完\")\n", &["完"]);
}

/* ─────────────── 函数帧清理：循环里 return / 递归里的循环 ─────────────── */

#[test]
fn 函数里_for_循环后正常返回() {
    assert_out(
        "function sum_to(n)\n    int s = 0\n    for i in range(n)\n        s += i\n    return s\nprint(sum_to(5))\n",
        &["10"],
    );
}

#[test]
fn 循环里提前_return_能收干净状态() {
    assert_out(
        "function first_gt(n)\n    for i in range(100)\n        if (i > n)\n            return i\n    return -1\nprint(first_gt(3))\nprint(first_gt(3))\n",
        &["4", "4"],
    );
}

#[test]
fn 递归函数里的_for_不串状态() {
    // 每层递归都有自己的循环状态，不能互相覆写
    assert_out(
        // f(3) = (0+1+2) + f(2) = 3 + (0+1) + f(1) = 3 + 1 + 0 + f(0) = 4
        "function f(n)\n    int s = 0\n    for i in range(n)\n        s += i\n    if (n > 0)\n        s += f(n - 1)\n    return s\nprint(f(3))\n",
        &["4"],
    );
}

#[test]
fn 递归里_for_in_也不串状态() {
    assert_out(
        // g(2) = (1+2) + g(1) = 3 + 3 + g(0) = 9
        "function g(n)\n    int s = 0\n    for x in [1, 2]\n        s += x\n    if (n > 0)\n        s += g(n - 1)\n    return s\nprint(g(2))\n",
        &["9"],
    );
}

#[test]
fn 从_for_in_里提前_return() {
    assert_out(
        "function find(xs, target)\n    for x in xs\n        if (x == target)\n            return \"有\"\n    return \"无\"\nprint(find([1,2,3], 2))\nprint(find([1,2,3], 9))\n",
        &["有", "无"],
    );
}

/* ─────────────── 字符串就地追加：别名不能被污染 ─────────────── */

#[test]
fn 字符串追加不污染别名() {
    assert_out(
        "str a = \"x\"\nstr b = a\na += \"y\"\nprint(a)\nprint(b)\n",
        &["xy", "x"],
    );
}

#[test]
fn 字符串追加多轮() {
    assert_out(
        "str s = \"\"\nfor i in range(4)\n    s += \"ab\"\nprint(len(s))\nprint(s)\n",
        &["8", "abababab"],
    );
}

#[test]
fn 字符串拼接会转数字() {
    assert_out("print(\"x\" + 1)\nprint(1 + \"x\")\n", &["x1", "1x"]);
}

#[test]
fn 字符串追加到列表里的副本() {
    assert_out(
        "str s = \"a\"\nlist l = [s]\ns += \"b\"\nprint(l)\nprint(s)\n",
        &["[\"a\"]", "ab"],
    );
}

/* ─────────────── 复合赋值 / 自增 ─────────────── */

#[test]
fn 复合赋值各种类型() {
    assert_out(
        "int a = 10\na += 5\na -= 3\na *= 2\na /= 4\nprint(a)\nfloat f = 1.0\nf += 0.5\nprint(f)\n",
        &["6", "1.5"],
    );
}

#[test]
fn 对一个变量反复取余() {
    assert_out("int a = 17\na %= 5\nprint(a)\n", &["2"]);
}

#[test]
fn 自增自减() {
    assert_out("int i = 0\ni++\ni++\ni--\nprint(i)\n", &["1"]);
}

#[test]
fn 自增作用在浮点上() {
    assert_out("float f = 0.5\nf++\nprint(f)\n", &["1.5"]);
}

/* ─────────────── 作用域 ─────────────── */

#[test]
fn 函数内局部变量不外泄() {
    assert_out(
        "function f()\n    int hidden = 42\n    return hidden\nprint(f())\nprint(hidden)\n",
        &["42", "null"],
    );
}

#[test]
fn 函数能读写全局() {
    assert_out(
        "int g = 1\nfunction bump()\n    g += 1\n    return g\nprint(bump())\nprint(bump())\nprint(g)\n",
        &["2", "3", "3"],
    );
}

#[test]
fn 同名局部遮蔽全局但不改全局() {
    assert_out(
        "int x = 1\nfunction f()\n    int x = 99\n    return x\nprint(f())\nprint(x)\n",
        &["99", "1"],
    );
}

/* ─────────────── 表达式 / 优先级 ─────────────── */

#[test]
fn 运算符优先级() {
    assert_out(
        "print(1 + 2 * 3)\nprint((1 + 2) * 3)\nprint(2 ** 3 ** 2)\nprint(1 < 2 && 3 > 4)\nprint(1 < 2 || 3 > 4)\n",
        &["7", "9", "512", "false", "true"],
    );
}

#[test]
fn 位运算优先级低于比较() {
    assert_out("print(1 + 2 & 3)\nprint(6 & 3)\nprint(1 << 3)\n", &["3", "2", "8"]);
}

#[test]
fn 逻辑短路真的不执行右边() {
    assert_out(
        "function boom()\n    print(\"不该到这里\")\n    return true\nprint(false && boom())\nprint(true || boom())\n",
        &["false", "true"],
    );
}

#[test]
fn 负数与取负() {
    assert_out("print(-3 + 1)\nprint(-(2 * 3))\nint a = 5\nprint(-a)\n", &["-2", "-6", "-5"]);
}

/* ─────────────── 容器 ─────────────── */

#[test]
fn 列表增删改查() {
    assert_out(
        "list xs = [3, 1, 2]\nxs.push(9)\nprint(xs)\nxs.sort()\nprint(xs)\nprint(xs[0])\nprint(len(xs))\nprint(xs.sum())\n",
        &["[3, 1, 2, 9]", "[1, 2, 3, 9]", "1", "4", "15"],
    );
}

#[test]
fn 解构赋值() {
    assert_out(
        "function pair()\n    return 1, 2\nint a, int b = pair()\nprint(a)\nprint(b)\n",
        &["1", "2"],
    );
}

#[test]
fn 字典遍历与取值() {
    assert_out(
        "dict d = {\"a\": 1, \"b\": 2}\nprint(d[\"a\"])\nprint(d.has(\"b\"))\nfor k, v in d\n    print(k, v)\n",
        &["1", "true", "a 1", "b 2"],
    );
}

/* ─────────────── 错误路径 ─────────────── */

#[test]
fn 除零会报错而不是崩() {
    let e = run_source("print(1 / 0)\n").unwrap_err();
    assert!(e.message.contains("除数为 0"), "实际：{}", e.message);
}

#[test]
fn 步长为零会报错() {
    let e = run_source("for i = 1 to 3 step 0\n    print(i)\n").unwrap_err();
    assert!(e.message.contains("步长"), "实际：{}", e.message);
}

#[test]
fn 下标越界会报错() {
    let e = run_source("list xs = [1]\nprint(xs[5])\n").unwrap_err();
    assert!(e.message.contains("越界"), "实际：{}", e.message);
}

#[test]
fn 死循环会被步数上限拦住() {
    let prog = "while (true)\n    x = 1\n";
    let mut vm_ok = false;
    match run_source(prog) {
        Ok(_) => vm_ok = true,
        Err(e) => assert!(e.message.contains("死循环") || e.message.contains("步数"), "实际：{}", e.message),
    }
    assert!(!vm_ok, "死循环不该正常结束");
}

#[test]
fn 深度递归被拦住且不崩进程() {
    let e = run_source("function f(n)\n    return f(n + 1)\nprint(f(0))\n").unwrap_err();
    assert!(e.message.contains("递归太深"), "实际：{}", e.message);
}

/* ─────────────── 指针与手动内存 ─────────────── */

#[test]
fn 申请指针并读写() {
    assert_out(
        "ptr p = alloc(3)\np[0] = 10\np[1] = p[0] * 2\nprint(p[0], p[1], len(p))\nfree(p)\n",
        &["10 20 3"],
    );
}

#[test]
fn 指针和自动内存可以混着用() {
    assert_out(
        "list xs = [1, 2]\nptr p = alloc(2)\np[0] = xs\np[1] = \"袋子\"\nprint(p[0], p[1], xs)\nfree(p)\n",
        &["[1, 2] 袋子 [1, 2]"],
    );
}

#[test]
fn 指针能当集合遍历() {
    assert_out(
        "ptr p = alloc(3)\nfor i in range(3)\n    p[i] = i\nint s = 0\nfor v in p\n    s += v\nprint(s)\nfree(p)\n",
        &["3"],
    );
}

#[test]
fn 指针传参改的是同一块() {
    assert_out(
        "fn fill(p, v)\n    for i in range(len(p))\n        p[i] = v\nptr p = alloc(2)\nfill(p, 7)\nprint(p[0], p[1])\nfree(p)\n",
        &["7 7"],
    );
}

#[test]
fn 释放后再读会给出中文错误() {
    let e = run_source("ptr p = alloc(2)\nfree(p)\nprint(p[0])\n").unwrap_err();
    assert!(e.message.contains("free") && e.message.contains("不能再读"), "实际：{}", e.message);
}

#[test]
fn 重复释放会报错() {
    let e = run_source("ptr p = alloc(2)\nfree(p)\nfree(p)\n").unwrap_err();
    assert!(e.message.contains("已经 free 过"), "实际：{}", e.message);
}

#[test]
fn 指针越界会报错() {
    let e = run_source("ptr p = alloc(2)\nprint(p[5])\n").unwrap_err();
    assert!(e.message.contains("越界"), "实际：{}", e.message);
}

#[test]
fn 忘了释放就不让跑完() {
    // 手动内存按 Rust 的规矩：申请了必须还，漏一个就不算正常结束
    let e = run_source("ptr t = alloc(10)\nt[0] = 1\nprint(\"跑完\")\n").unwrap_err();
    assert!(e.message.contains("还没有 free"), "实际：{}", e.message);
    assert!(e.message.contains("第 1 行"), "要指出是哪一行申请的：{}", e.message);
}

#[test]
fn 循环里申请又释放是合法的() {
    assert_out(
        "for i in range(100)\n    ptr t = alloc(10)\n    t[0] = i\n    free(t)\nprint(\"跑完\")\n",
        &["跑完"],
    );
}

#[test]
fn 漏了好几块会一次性列出来() {
    let e = run_source("ptr a = alloc(1)\nptr b = alloc(2)\n").unwrap_err();
    assert!(e.message.contains("2 块"), "实际：{}", e.message);
}

/* ─────────────── 默认参数与中文报错 ─────────────── */

#[test]
fn 默认参数能引用前面的参数() {
    assert_out(
        "fn area(w, h = w)\n    return w * h\nprint(area(3, 4))\nprint(area(5))\n",
        &["12", "25"],
    );
}

#[test]
fn 报错带行号() {
    let e = run_source("int a = 1\nint b = 0\nprint(a / b)\n").unwrap_err();
    assert!(e.message.contains("第 3 行"), "实际：{}", e.message);
}

#[test]
fn 报错带中文调用链() {
    let e = run_source(
        "fn inner(xs)\n    return xs[9]\nfn outer()\n    return inner([1])\nprint(outer())\n",
    )
    .unwrap_err();
    assert!(e.message.contains("调用链"), "实际：{}", e.message);
    assert!(e.message.contains("inner") && e.message.contains("outer"), "实际：{}", e.message);
    assert!(!e.message.contains("Traceback"), "不该出现英文 Traceback：{}", e.message);
}
