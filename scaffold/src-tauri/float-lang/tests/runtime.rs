//! 端到端运行测试：源码进去，输出出来。
//!
//! 这些用例同时充当「Rust 版语义说明书」——以后改 VM 时，
//! 一旦打破了哪条既有语义，这里会立刻红。

use float_lang::run_text;

fn out(src: &str) -> String {
    match run_text(src) {
        Ok(s) => s,
        Err(e) => panic!("运行失败：{}", e),
    }
}

fn err_of(src: &str) -> String {
    match run_text(src) {
        Ok(s) => panic!("本该报错，却跑出了：{}", s),
        Err(e) => e.message,
    }
}

#[test]
fn 打印与算术() {
    assert_eq!(out("print(1 + 2)"), "3\n");
    assert_eq!(out("print(10 - 3 * 2)"), "4\n");
    assert_eq!(out("print((10 - 3) * 2)"), "14\n");
    assert_eq!(out("print(7 / 2)"), "3.5\n");
    assert_eq!(out("print(8 / 2)"), "4\n");
    assert_eq!(out("print(7 % 3)"), "1\n");
    assert_eq!(out("print(2 ** 10)"), "1024\n");
    assert_eq!(out("print(-5)"), "-5\n");
}

#[test]
fn 整数与浮点分开() {
    assert_eq!(out("print(type(1))"), "int\n");
    assert_eq!(out("print(type(1.5))"), "float\n");
    // 浮点长尾收敛，不打 0.30000000000000004
    assert_eq!(out("print(0.1 + 0.2)"), "0.3\n");
    // 整数运算不溢出成浮点
    assert_eq!(out("print(1000000 * 1000000)"), "1000000000000\n");
}

#[test]
fn 字符串拼接与比较() {
    assert_eq!(out(r#"print("ab" + "cd")"#), "abcd\n");
    assert_eq!(out(r#"print("n=" + 42)"#), "n=42\n");
    assert_eq!(out(r#"print("abc" < "abd")"#), "true\n");
    assert_eq!(out(r#"print(len("中文abc"))"#), "5\n");
}

#[test]
fn 变量声明与复合赋值() {
    assert_eq!(out("int a = 1\na += 2\na *= 3\nprint(a)"), "9\n");
    assert_eq!(out("var x = 10\nx -= 4\nprint(x)"), "6\n");
    assert_eq!(out("int n = 1\nn++\nn++\nprint(n)"), "3\n");
}

#[test]
fn 中文标识符() {
    assert_eq!(out("int 血量 = 100\n血量 -= 30\nprint(血量)"), "70\n");
    assert_eq!(
        out("function 打招呼(名字)\n    return \"你好，\" + 名字\nprint(打招呼(\"小方\"))"),
        "你好，小方\n"
    );
}

#[test]
fn 条件分支与_elif_else() {
    let src = "int n = 5
if (n > 10)
    print(\"大\")
elif (n > 3)
    print(\"中\")
else
    print(\"小\")";
    assert_eq!(out(src), "中\n");
}

#[test]
fn 花括号写法的_else_合并() {
    // 行尾 { 会被剥掉，} else { 要能并回上一个 if
    let src = "int n = 1
if (n == 1) {
    print(\"one\")
} else {
    print(\"other\")
}";
    assert_eq!(out(src), "one\n");
}

#[test]
fn while_与_break_continue() {
    let src = "int i = 0
int s = 0
while (i < 10)
    i++
    if (i % 2 == 0)
        continue
    if (i > 7)
        break
    s += i
print(s)";
    // 1 + 3 + 5 + 7 = 16
    assert_eq!(out(src), "16\n");
}

#[test]
fn for_range_不含终点() {
    let src = "int s = 0
for i in range(5)
    s += i
print(s)";
    assert_eq!(out(src), "10\n");
    assert_eq!(out("for i in range(2, 5)\n    print(i)"), "2\n3\n4\n");
    assert_eq!(out("for i in range(0, 6, 2)\n    print(i)"), "0\n2\n4\n");
}

#[test]
fn for_to_含两端() {
    assert_eq!(out("for i = 1 to 3\n    print(i)"), "1\n2\n3\n");
    assert_eq!(out("for i = 5 to 1 step -2\n    print(i)"), "5\n3\n1\n");
}

#[test]
fn 遍历列表与字符串() {
    assert_eq!(out("for x in [1, 2, 3]\n    print(x)"), "1\n2\n3\n");
    assert_eq!(out("for c in \"ab\"\n    print(c)"), "a\nb\n");
}

#[test]
fn 遍历字典拿键() {
    let src = "dict d = {\"a\": 1, \"b\": 2}
for k in d
    print(k)";
    assert_eq!(out(src), "a\nb\n");
}

#[test]
fn 遍历字典拿键值() {
    let src = "dict d = {\"a\": 1, \"b\": 2}
for k, v in d
    print(k, v)";
    assert_eq!(out(src), "a 1\nb 2\n");
}

#[test]
fn repeat_循环() {
    assert_eq!(out("int n = 0\nrepeat(3)\n    n += 1\nprint(n)"), "3\n");
}

#[test]
fn 函数与递归() {
    let src = "function fib(n)
    if (n < 2)
        return n
    return fib(n - 1) + fib(n - 2)
print(fib(20))";
    assert_eq!(out(src), "6765\n");
}

#[test]
fn 函数默认参数() {
    let src = "function greet(name, greeting = \"你好\")
    return greeting + \"，\" + name
print(greet(\"小方\"))
print(greet(\"小方\", \"早\"))";
    assert_eq!(out(src), "你好，小方\n早，小方\n");
}

#[test]
fn 函数多返回值() {
    let src = "function pair()
    return 1, 2
a, b = pair()
print(a, b)";
    assert_eq!(out(src), "1 2\n");
}

#[test]
fn 列表字面量与下标() {
    let src = "list xs = [10, 20, 30]
print(xs[0])
print(xs[2])
xs[1] = 99
print(xs)";
    assert_eq!(out(src), "10\n30\n[10, 99, 30]\n");
}

#[test]
fn 列表方法与内建函数() {
    assert_eq!(out("list xs = [3, 1, 2]\nxs.sort()\nprint(xs)"), "[1, 2, 3]\n");
    assert_eq!(out("list xs = [1]\nxs.push(2)\nprint(xs)"), "[1, 2]\n");
    assert_eq!(out("print([1, 2, 3].sum())"), "6\n");
    assert_eq!(out("print([1, 2].join(\"-\"))"), "1-2\n");
    assert_eq!(out("print(\"a,b,c\".split(\",\"))"), r#"["a", "b", "c"]"#.to_string() + "\n");
}

#[test]
fn 字典操作() {
    let src = "dict d = {}
d[\"k\"] = 1
print(d[\"k\"])
print(d.has(\"k\"))
print(d.keys())";
    assert_eq!(out(src), "1\ntrue\n[\"k\"]\n");
}

#[test]
fn 逻辑短路() {
    assert_eq!(out("print(1 < 2 && 2 < 3)"), "true\n");
    assert_eq!(out("print(false && unknown_thing)"), "false\n");
    assert_eq!(out("print(true || unknown_thing)"), "true\n");
}

#[test]
fn 位运算() {
    assert_eq!(out("print(6 & 3)"), "2\n");
    assert_eq!(out("print(6 | 3)"), "7\n");
    assert_eq!(out("print(6 ^ 3)"), "5\n");
    assert_eq!(out("print(1 << 4)"), "16\n");
}

#[test]
fn 除零报错() {
    assert!(err_of("print(1 / 0)").contains("除数为 0"));
}

#[test]
fn 无限递归被拦住() {
    let e = err_of("function f(n)\n    return f(n + 1)\nprint(f(0))");
    assert!(e.contains("递归太深"), "实际是：{}", e);
}

#[test]
fn 下标越界报错() {
    assert!(err_of("list xs = [1]\nprint(xs[5])").contains("越界"));
}

#[test]
fn 语法错误会被报出来() {
    assert!(run_text("if (\n    print(1)").is_err());
    assert!(run_text("function\n    print(1)").is_err());
}

#[test]
fn 空程序不炸() {
    assert_eq!(out(""), "");
    assert_eq!(out("-- 只有注释\n"), "");
    assert_eq!(out("-- 注释\n\n-- 又一行\n"), "");
}

#[test]
fn 嵌套函数体里的局部变量互不干扰() {
    let src = "function f(a)
    int t = a * 2
    return t
function g(a)
    int t = a + 100
    return t
print(f(1), g(1))";
    assert_eq!(out(src), "2 101\n");
}

#[test]
fn 循环里重复声明同一变量() {
    let src = "for i in range(3)
    int k = i * 10
    print(k)";
    assert_eq!(out(src), "0\n10\n20\n");
}

/* ── 空变量 + 汇编式算术（add / sub / mul / div / mod，空格分隔） ── */

#[test]
fn 空变量按类型给中性初值() {
    assert_eq!(out("int c\nprint(c)"), "0\n");
    assert_eq!(out("float f\nprint(f)"), "0\n");
    assert_eq!(out("str s\nprint(\"[\" + s + \"]\")"), "[]\n");
    assert_eq!(out("bool b\nprint(b)"), "false\n");
    assert_eq!(out("list xs\nprint(len(xs))"), "0\n");
}

#[test]
fn 空变量能直接累加() {
    let src = "int 总数\nfor i = 1 to 4\n    总数 += i\nprint(总数)";
    assert_eq!(out(src), "10\n");
}

#[test]
fn 汇编式算术两个数() {
    let src = "int a = 2\nint b = 3\nint c\nc = add a b\nprint(c)";
    assert_eq!(out(src), "5\n");
}

#[test]
fn 汇编式算术能接多个数() {
    assert_eq!(out("print(add 1 2 3 4)"), "10\n");
    assert_eq!(out("print(sub 100 1 2 3)"), "94\n");
    assert_eq!(out("print(mul 2 3 4)"), "24\n");
    assert_eq!(out("print(div 100 5 2)"), "10\n");
    assert_eq!(out("print(mod 10 3)"), "1\n");
}

#[test]
fn 汇编式算术和运算符能混写() {
    assert_eq!(out("print(add 1 2 + 10)"), "13\n");
    assert_eq!(out("print(mul 2 3 - 1)"), "5\n");
}

#[test]
fn 带括号的老写法仍然有效() {
    assert_eq!(out("print(add(2, 3))"), "5\n");
    assert_eq!(out("print(mul(2, 3))"), "6\n");
}

#[test]
fn 只有一个数要报清楚() {
    let e = err_of("print(add 1)");
    assert!(e.contains("至少要两个"), "{}", e);
}

/* ── 交互式 ask：没答案时挂起，有答案时跑完 ── */

#[test]
fn 交互式运行会停在_ask_上等人回答() {
    let src = "ask \"你叫啥\"\nprint(\"你好，\" + ans)";
    let r1 = float_lang::api::run_interactive("m.cw", src, None, vec![], Default::default());
    assert!(r1.error.is_none(), "不该当错误处理");
    assert_eq!(r1.need_input.as_deref(), Some("你叫啥"));
    // 挂起前的输出要留着（界面要显示前半段）
    assert!(r1.output.contains("你叫啥"));

    let r2 = float_lang::api::run_interactive(
        "m.cw", src, None, vec!["小明".into()], Default::default(),
    );
    assert!(r2.need_input.is_none());
    assert_eq!(r2.outcome.unwrap().output, "你叫啥\n小明\n你好，小明\n");
}

#[test]
fn 交互式挂起不会被_try_吞掉() {
    let src = "try\n    ask \"继续吗\"\ncatch e\n    print(\"不该进来\")";
    let r = float_lang::api::run_interactive("m.cw", src, None, vec![], Default::default());
    assert_eq!(r.need_input.as_deref(), Some("继续吗"));
}

#[test]
fn 不挂起模式还是老行为_取完给空串() {
    // 测试和自动化里不该卡住：没有输入就直接空串
    assert_eq!(out("ask \"问题\"\nprint(\"[\" + ans + \"]\")"), "问题\n[]\n");
}

/* ── 嵌套：块、循环、异常、类都要能随便套 ── */

#[test]
fn 五层嵌套混搭块() {
    let src = "int 命中 = 0
for a = 1 to 2
    for b = 1 to 2
        while (b < 3)
            if (a == 1)
                if (b == 2)
                    for c in range(2)
                        try
                            if (c == 1)
                                throw \"内层错误\"
                            命中 += 1
                        catch e
                            命中 += 10
                        finally
                            命中 += 100
                else
                    命中 += 1000
            break
print(命中)";
    assert_eq!(out(src), "1211\n");
}

#[test]
fn 继承类的方法里套循环和_try() {
    let src = "class 角色
    str 名 = \"甲\"
    int 血 = 30
    fn 掉血(n)
        self.血 -= n
        if (self.血 <= 0)
            throw self.名 + \" 倒了\"
        return self.血

class 法师 extends 角色
    fn 施法(次数)
        int 结果 = 0
        for i in range(次数)
            try
                结果 = self.掉血(8)
            catch e
                结果 = -1
                break
        return 结果

法师 m = new 法师()
print(m.施法(2))
print(m.施法(6))";
    assert_eq!(out(src), "14\n-1\n");
}

#[test]
fn 嵌套列表字典都能遍历() {
    let src = "dict 玩家表 = {\"甲\": [1, 2, 3], \"乙\": [4, 5]}
for 名 in 玩家表
    list 分 = 玩家表[名]
    int 和 = 0
    for v in 分
        和 += v
    print(名, 和)
list 矩阵 = [[1,2],[3,4]]
for 行 in 矩阵
    for v in 行
        print(v)";
    assert_eq!(out(src), "甲 6\n乙 9\n1\n2\n3\n4\n");
}

#[test]
fn 残缺代码不会卡死() {
    // 语法错归错，但必须能返回，不能把编辑器挂住
    let e = err_of("if (\n\nfor\nwhile");
    assert!(!e.is_empty());
}

/* ── 类型标注不是装饰：一眼能看出来的错，编译期就拦住 ── */

#[test]
fn 类型不匹配在编译期就报错() {
    let e = err_of("int a = \"字符串\"");
    assert!(e.contains("int") && e.contains("str"), "{}", e);
    let e2 = err_of("str s = 1");
    assert!(e2.contains("str"), "{}", e2);
    let e3 = err_of("list xs = 5");
    assert!(e3.contains("list"), "{}", e3);
    // 声明之后再赋错类型也要拦
    let e4 = err_of("int n = 1\nn = \"x\"");
    assert!(e4.contains("int"), "{}", e4);
}

#[test]
fn 合理赋值不该被误报() {
    // int 位置给 float 是允许的（float 是更宽的类型）
    assert_eq!(out("float f = 1\nprint(f)"), "1\n");
    // var 明确表示"不强制"，随便换类型
    assert_eq!(out("var v = 1\nv = \"x\"\nprint(v)"), "x\n");
    // 右边是变量/调用时静态看不出来，不拦
    assert_eq!(out("str s = \"a\"\nint n = 2\nprint(s + \"b\", n + 1)"), "ab 3\n");
}
