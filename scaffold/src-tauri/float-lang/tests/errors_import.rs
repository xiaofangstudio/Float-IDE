//! try / catch / finally / throw / import 的端到端测试。
//!
//! 这两样是最后补齐的两块"语言必备件"：错误能接住，代码能拆文件。

use float_lang::run_text;
use std::io::Write;

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

/* ───────────── try / catch ───────────── */

#[test]
fn throw_能被_catch_接住() {
    let src = "try
    throw \"余额不够\"
catch e
    print \"接住了：\" + e";
    assert_eq!(out(src), "接住了：余额不够\n");
}

#[test]
fn 运行时错误也能接住() {
    // 除以零 / 不存在的方法都会走到同一条错误通道
    let src = "try
    list xs
    print xs.没有这个方法()
catch e
    print \"出错了\"";
    assert_eq!(out(src), "出错了\n");
}

#[test]
fn throw_抛什么就接什么() {
    // 语言规则：throw 的值原样传给 catch（不做包装），
    // 所以 print e 得到的是自己抛的那句话
    let src = "try
    throw \"原样\" + \"传过来\"
catch e
    print e";
    assert_eq!(out(src), "原样传过来\n");
}

#[test]
fn 运行时错误接到的是一张带消息和行号的字典() {
    let src = "try
    int n = 1
    print n.什么
catch e
    print e.行
    print e.类型";
    // 成员访问失败 → catch 拿到 {"消息":…, "行":3, "类型":"运行时错误"}
    assert!(out(src).starts_with("3\n"), "{}", out(src));
}

#[test]
fn 没出错就不进_catch() {
    let src = "try
    print \"正常\"
catch e
    print \"不该到这\"";
    assert_eq!(out(src), "正常\n");
}

#[test]
fn catch_之后照常往下跑() {
    let src = "try
    throw \"x\"
catch e
    print \"1\"
print \"2\"";
    assert_eq!(out(src), "1\n2\n");
}

#[test]
fn finally_两种情况都跑() {
    let ok = "try
    print \"试\"
finally
    print \"收尾\"";
    assert_eq!(out(ok), "试\n收尾\n");

    let bad = "try
    throw \"炸\"
finally
    print \"收尾\"";
    // 没有 catch，finally 跑完错误继续往外抛
    let e = err_of(bad);
    assert!(e.contains("炸"), "{}", e);
}

#[test]
fn try里出错之后的语句不再执行() {
    let src = "try
    throw \"1\"
    print \"这行不该跑\"
catch e
    print \"接住\"";
    assert_eq!(out(src), "接住\n");
}

#[test]
fn 函数里抛出的错能被外面的_catch_接住() {
    let src = "fn 检查(n)
    if (n < 0)
        throw \"不能是负数\"
    return n
try
    print 检查(-1)
catch e
    print e
print \"程序还在\"";
    assert_eq!(out(src), "不能是负数\n程序还在\n");
}

#[test]
fn 嵌套_try_里层先接() {
    let src = "try
    try
        throw \"内\"
    catch e
        print \"内层：\" + e
    throw \"外\"
catch e
    print \"外层：\" + e";
    assert_eq!(out(src), "内层：内\n外层：外\n");
}

#[test]
fn catch_里再抛会传给外层() {
    let src = "try
    try
        throw \"一\"
    catch e
        throw \"二\"
catch e
    print e";
    assert_eq!(out(src), "二\n");
}

#[test]
fn 循环里的_try_每圈都能接() {
    let src = "for i in range(3)
    try
        if (i == 1)
            throw \"跳过这圈\"
        print i
    catch e
        print \"接住第 \" + i"; 
    assert_eq!(out(src), "0\n接住第 1\n2\n");
}

#[test]
fn break_跳出带try的循环后错误照旧往外传() {
    // 跳出 try 时处理器要撤掉，否则后面的错误会被跳进已经废弃的 catch
    let src = "try
    for i in range(3)
        try
            break
        catch e
            print \"不该到这\"
    throw \"外面的错\"
catch e
    print \"外层接住：\" + e";
    assert_eq!(out(src), "外层接住：外面的错\n");
}

#[test]
fn 没有_try_时错误照样终止程序() {
    let e = err_of("throw \"没人接\"");
    assert!(e.contains("没人接"), "{}", e);
}

#[test]
fn 空_try_没有_catch_是语法错() {
    let e = err_of("try
    print 1");
    assert!(e.contains("catch"), "{}", e);
}

/* ───────────── import ───────────── */

fn write(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let p = dir.join(name);
    let mut f = std::fs::File::create(&p).unwrap();
    f.write_all(body.as_bytes()).unwrap();
    p
}

fn run_file(file: &std::path::Path, src: &str) -> Result<String, float_lang::FloatError> {
    let base = file.parent();
    float_lang::api::run_from(file.to_str().unwrap(), src, base).map(|o| o.output)
}

#[test]
fn import_能拿到库里的函数() {
    let dir = std::env::temp_dir().join("cw_import_fn");
    write(&dir, "mathx.cw", "fn 平方(x)\n    return x * x\n");
    let main = dir.join("main.cw");
    let src = "import \"mathx.cw\"\nprint 平方(7)";
    assert_eq!(run_file(&main, src).unwrap(), "49\n");
}

#[test]
fn import_不带后缀也能找到() {
    let dir = std::env::temp_dir().join("cw_import_noext");
    write(&dir, "工具.cw", "fn 你好()\n    return \"你好\"\n");
    let main = dir.join("main.cw");
    assert_eq!(run_file(&main, "import \"工具\"\nprint 你好()").unwrap(), "你好\n");
}

#[test]
fn import_同一个库两次不会重复定义() {
    let dir = std::env::temp_dir().join("cw_import_twice");
    write(&dir, "a.cw", "fn f()\n    return 1\n");
    let main = dir.join("main.cw");
    let src = "import \"a.cw\"\nimport \"a.cw\"\nprint f()";
    assert_eq!(run_file(&main, src).unwrap(), "1\n");
}

#[test]
fn import_能套娃() {
    let dir = std::env::temp_dir().join("cw_import_nest");
    write(&dir, "b.cw", "fn 二()\n    return 2\n");
    write(&dir, "a.cw", "import \"b.cw\"\nfn 一()\n    return 二() + 1\n");
    let main = dir.join("main.cw");
    assert_eq!(run_file(&main, "import \"a.cw\"\nprint 一()").unwrap(), "3\n");
}

#[test]
fn import_循环引用要报错() {
    let dir = std::env::temp_dir().join("cw_import_cycle");
    write(&dir, "x.cw", "import \"y.cw\"\nfn xf()\n    return 1\n");
    write(&dir, "y.cw", "import \"x.cw\"\nfn yf()\n    return 2\n");
    let main = dir.join("main.cw");
    let e = run_file(&main, "import \"x.cw\"\nprint xf()").unwrap_err();
    assert!(e.message.contains("循环"), "{}", e.message);
}

#[test]
fn import_找不到文件要说清楚() {
    let dir = std::env::temp_dir().join("cw_import_missing");
    std::fs::create_dir_all(&dir).unwrap();
    let main = dir.join("main.cw");
    let e = run_file(&main, "import \"没有.cw\"").unwrap_err();
    assert!(e.message.contains("找不到库文件"), "{}", e.message);
}

#[test]
fn 库里的运行时错误会指回库的文件和行号() {
    let dir = std::env::temp_dir().join("cw_import_errline");
    write(&dir, "坏库.cw", "fn 炸()\n    return 1 / 0\n");
    let main = dir.join("main.cw");
    let out = run_file(&main, "import \"坏库.cw\"\nprint 炸()");
    // 具体文案看 VM，关键是提到了库文件名
    let msg = match out {
        Ok(s) => panic!("本该出错，跑出了 {}", s),
        Err(e) => e.message,
    };
    assert!(msg.contains("坏库.cw"), "{}", msg);
}
