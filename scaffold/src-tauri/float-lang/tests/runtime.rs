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
    assert_eq!(out("// 只有注释\n"), "");
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
