//! AOT 回归：翻译层的行为（不需要 rustc，纯 emit 检查）。
//! 真正"跑起来对不对"由 examples/aot_parity.rs 对拍（会调用 cargo，单独跑）。

use float_lang::{aot, parser};

fn emit(src: &str) -> Result<String, aot::AotError> {
    let prog = parser::parse_program("t.co", src);
    assert!(prog.errors.is_empty(), "语法不该出错：{:?}", prog.errors);
    aot::emit(&prog)
}

#[test]
fn 简单程序能翻成_rust() {
    let r = emit("int a = 1\nprint(a + 2)\n").expect("应该能翻");
    assert!(r.contains("fn run_top"), "缺 run_top");
    assert!(r.contains("println!"), "缺 println");
}

#[test]
fn 整数变量被特化成_i64() {
    let r = emit("int s = 0\nfor i in range(10)\n    s += i\nprint(s)\n").expect("应该能翻");
    assert!(r.contains("gi: Vec<i64>"), "全局整数应该用 i64 数组");
    assert!(!r.contains("want_int(&arith"), "循环体里不该走通用算术");
}

#[test]
fn 指针现在能翻成_rust() {
    // 指针是这一版新补的能力：&局部 → ptr_of_cell、&列表[i] → ptr_of_index，
    // 读 *p、写 *p / *p += v 都走 ptr_get / ptr_set（和 VM 共用同一块内存）
    let r = emit("int x = 1\nptr p = &x\n*p = 2\nprint(*p)\n").expect("指针应该能翻");
    assert!(r.contains("ptr_of_cell") || r.contains("ptr_of_"), "缺取地址：{}", &r[..r.len().min(400)]);
    assert!(r.contains("ptr_set") || r.contains("ptr_get"), "缺解引用");
}

#[test]
fn 有_ask_的程序明确拒绝() {
    let e = emit("int a = ask(\"几岁\")\n").expect_err("ask 应该拒绝");
    assert!(e.message.contains("ask"), "原因要说清楚：{}", e.message);
}

#[test]
fn 未定义变量在翻译期就报() {
    let e = emit("print(nope)\n").expect_err("读未定义变量应该拒绝");
    assert!(e.message.contains("没有定义"), "{}", e.message);
}

#[test]
fn 回调式高阶函数与内联函数能翻成_rust() {
    // data.* 9 个高阶函数已从「VM 编译期内联展开」补齐为 prelude 运行时实现
    // （hof_run/hof_dispatch），回调经 dcall 静态表 / 注册表间接调用；
    // 内联函数 fn(x) return … 编译成隐藏函数 __lambda_行_列，发射 V::Native。
    let src = "list xs = [1, 2]\nfn twice(x)\n    return x * 2\nprint(data.map(xs, twice))\n";
    let r = emit(src).expect("高阶函数应该能翻");
    assert!(r.contains("hof_dispatch") || r.contains("hof_run"), "缺 hof 运行时:{}", r.len());
    assert!(r.contains("data.map"), "缺高阶函数调用");
    // 内联函数：lambda 要登记成隐藏函数并发射 V::Native 引用
    let src2 = "print(data.map([1,2,3], fn(x) return x * 2))\n";
    let r2 = emit(src2).expect("lambda 应该能翻");
    assert!(r2.contains("__lambda_"), "缺隐藏函数");
    assert!(r2.contains("V::Native(\"__lambda_"), "缺函数值引用");
}

#[test]
fn 类能翻出构造与方法分派() {
    let r = emit("class Dog\n    str name = \"\"\n    fn init(n)\n        name = n\n    fn bark()\n        return name + \"汪\"\nDog d = new Dog(\"大黄\")\nprint(d.bark())\n")
        .expect("类应该能翻");
    assert!(r.contains("fn cf___new_Dog_"), "缺构造函数");
    assert!(r.contains("fn mcall"), "缺方法分派");
    assert!(r.contains("(\"Dog\", \"bark\")"), "分派表缺 Dog.bark");
}

#[test]
fn 作用域语义与_vm_一致_块内声明块外可见() {
    let r = emit("if (true)\n    int a = 1\nprint(a)\n").expect("应该能翻");
    // 块内声明的变量提升到顶层槽位（全局/整数槽），所以块外读得到；
    // 顶层不该出现 Rust 局部变量
    assert!(r.contains("g.gi[") || r.contains("g.v["), "缺全局槽");
    assert!(!r.contains("let mut c_a"), "顶层不该出现局部变量");
}

/* ── 分块（增量）编译：文件结构与"大表格走紧凑数据" ── */

fn emit_split_of(src: &str) -> Vec<float_lang::aot::SplitFile> {
    let prog = parser::parse_program("t.co", src);
    assert!(prog.errors.is_empty(), "{:?}", prog.errors.first().map(|e| e.message.clone()));
    float_lang::aot::emit_split(&prog, "demo", "path/to/float-lang").expect("应该能分块")
}

#[test]
fn 分块会生成运行时加函数块加主程序() {
    let mut src = String::new();
    for i in 0..450 {
        src.push_str(&format!("fn f{}(int x)\n    return x + {}\n", i, i));
    }
    src.push_str("print(f0(1))\n");
    let files = emit_split_of(&src);

    let paths: Vec<&str> = files.iter().map(|f| f.rel.as_str()).collect();
    assert!(paths.contains(&"Cargo.toml"), "缺 workspace 清单");
    assert!(paths.contains(&"rt/src/lib.rs"), "缺运行时");
    assert!(paths.contains(&"app/src/main.rs"), "缺主程序");
    let chunks = paths.iter().filter(|p| p.starts_with("chunk") && p.ends_with("lib.rs")).count();
    assert!(chunks >= 2 && chunks <= 12, "函数块数量不对：{}", chunks);

    let app = files.iter().find(|f| f.rel == "app/src/main.rs").unwrap();
    assert!(app.content.contains("register(copper_chunk0::TABLE)"), "启动时没登记函数表");
    assert!(app.content.contains("fn main()"), "缺 main");
}

#[test]
fn 大表格走紧凑数据而不是几千条语句() {
    // 300 个元素的列表：必须编成 parse_data(...)，否则 rustc 会被拖死
    let mut src = String::from("list 表 = [\n");
    let rows: Vec<String> = (0..300).map(|i| format!("    [{}, \"地块{}\"]", i, i)).collect();
    src.push_str(&rows.join(",\n"));
    src.push_str("\n]\nprint(len(表))\n");
    let files = emit_split_of(&src);
    let all: String = files.iter().map(|f| f.content.clone()).collect();
    assert!(all.contains("parse_data("), "大表格应该走紧凑数据");
}

#[test]
fn 分块模式下跨块调用走注册表() {
    let mut src = String::new();
    for i in 0..300 {
        src.push_str(&format!("fn g{}(int x)\n    return x + {}\n", i, i));
    }
    src.push_str("print(g0(1))\nprint(g299(2))\n");
    let files = emit_split_of(&src);
    let all: String = files.iter().map(|f| f.content.clone()).collect();
    assert!(all.contains("call_named(g,"), "跨块调用应该走 call_named");
}
