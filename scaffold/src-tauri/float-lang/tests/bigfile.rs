//! 大文件稳定性回归：3000~10000 行的"游戏级"脚本必须能解析、编译、跑完，
//! 而且极端形状（几万项拼接、几千层括号）要么跑通、要么给可读报错，**绝不能崩**。
//!
//! 这些测试都在内存里造源码，不调用 rustc，跑得很快。

use float_lang::{bytecode, parser, Vm};

/// 造一个"游戏规模"的脚本：数据表 + 规则函数 + 主循环
fn make_game(lines_target: usize) -> String {
    let mut out: Vec<String> = Vec::new();
    out.push("/* 生成的大文件".to_string());

    // 数据表：每行一条记录，凑到目标行数的四成
    let rows = (lines_target * 2 / 5).max(100);
    out.push("list 地图 = [".into());
    let mut recs = Vec::new();
    for r in 0..rows {
        recs.push(format!("    [{}, {}, \"地形{}\", {}]", r, r % 37, r % 12, r * 3 % 100));
    }
    out.push(recs.join(",\n"));
    out.push("]".into());

    // 规则函数：每个 4 行
    let fns = (lines_target / 8).max(50);
    for i in 0..fns {
        out.push(format!("fn 规则{}(int x, int y)", i));
        out.push("    if (x > y)".into());
        out.push(format!("        return x - y + {}", i));
        out.push(format!("    return y - x + {}", i));
    }

    // 主循环
    out.push("int 总分 = 0".into());
    out.push("for i in range(200)".into());
    out.push("    int 血 = 地图[i % len(地图)][1]".into());
    for i in 0..(fns.min(200)) {
        out.push(format!("    if (血 > {})", i % 50));
        out.push(format!("        总分 += 规则{}(i, 血)", i));
        out.push("    else".into());
        out.push("        总分 += 血".into());
    }
    out.push("print(总分)".into());
    out.push("print(len(地图))".into());
    out.join("\n") + "\n"
}

fn compile_and_run(src: &str) -> String {
    let prog = parser::parse_program("big.co", src);
    assert!(prog.errors.is_empty(), "不该有语法错：{:?}", prog.errors.first().map(|e| e.message.clone()));
    let (chunk, _fns, errs) = bytecode::compile(&prog);
    assert!(errs.is_empty(), "不该有编译错：{:?}", errs.first().map(|e| e.message.clone()));
    let mut vm = Vm::new(chunk);
    vm.run().expect("应该能跑").output
}

#[test]
fn 五千行游戏脚本能跑() {
    let src = make_game(5000);
    assert!(src.lines().count() >= 4000, "生成的样本不够大：{}", src.lines().count());
    let out = compile_and_run(&src);
    assert!(out.lines().count() >= 1, "至少要有输出");
}

#[test]
fn 一万行游戏脚本能跑() {
    let src = make_game(10000);
    assert!(src.lines().count() >= 8000, "生成的样本不够大：{}", src.lines().count());
    let out = compile_and_run(&src);
    assert!(!out.is_empty(), "应该有输出");
}

#[test]
fn 五千行地图表能解析且顺序正确() {
    // 跨行的单个列表字面量：这是最容易被写成 O(n²) 的形状
    let mut rows = Vec::new();
    for r in 0..5000 {
        rows.push(format!("    [{}, \"地块{}\"]", r, r));
    }
    let src = format!("list 地图 = [\n{}\n]\nprint(len(地图))\nprint(地图[4999][0])\n", rows.join(",\n"));
    let out = compile_and_run(&src);
    assert_eq!(out.trim(), "5000\n4999", "大表格内容/顺序不对：{}", out);
}

#[test]
fn 两万项字符串拼接不会崩且顺序正确() {
    // 拼接顺序必须和源里一致（平衡化只改分组，不改顺序）
    let parts: Vec<String> = (0..20000).map(|i| format!("\"{}\"", i % 10)).collect();
    let src = format!("str s = {}\nprint(len(s))\nprint(s[0] + s[19999])\n", parts.join(" + "));
    let out = compile_and_run(&src);
    assert_eq!(out.trim(), "20000\n09", "拼接结果不对：{}", out);
}

#[test]
fn 深括号不再设上限() {
    // 解析跟 lang_run / examples/run 一样跑在 64MB 大栈线程上：
    // debug 构建的默认测试线程只有 2MB，50 层左右就会溢出
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            let src = format!("print({}1{})\n", "(".repeat(200), ")".repeat(200));
            let prog = parser::parse_program("deep.co", &src);
            assert!(prog.errors.is_empty(), "200 层括号不该报错");
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn 死循环刷屏不再被输出上限拦住() {
    // 20 KB × 3000 = 60 MB，旧版会卡在 32 MB，现在必须完整跑完
    let src = r#"
str s = ""
for i in range(2000)
    s += "0123456789"
for i in range(3000)
    print(s)
print("走得到这里")
"#;
    let prog = parser::parse_program("flood.co", src);
    assert!(prog.errors.is_empty(), "{:?}", prog.errors.first().map(|e| e.message.clone()));
    let (chunk, _f, errs) = bytecode::compile(&prog);
    assert!(errs.is_empty());
    let mut vm = Vm::new(chunk);
    let o = vm.run().expect("60MB 输出应该允许");
    assert!(o.output.len() > 32 * 1024 * 1024, "输出只有 {} 字节", o.output.len());
}

#[test]
fn 平衡化不改变减法左结合语义() {
    let out = compile_and_run("print(10 - 3 - 2)\nprint(100 / 5 / 2)\n");
    assert_eq!(out.trim(), "5\n10", "左结合语义被改了：{}", out);
}
