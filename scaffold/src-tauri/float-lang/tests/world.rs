//! 「世界接口」测试：文件 / 时间 / 网络 / 调试输出 / 静态类型拦截。
//!
//! 这些是外部评价里点名的缺口（readfile / fetch / now / 编译前拦截），
//! 这份文件把它们全部锁死：以后任何人改坏了，这里立刻红。

use float_lang::api::{run_from, run_interactive};
use float_lang::host;
use std::sync::Arc;

fn out(src: &str) -> String {
    match float_lang::run_text(src) {
        Ok(s) => s,
        Err(e) => panic!("运行失败：{}", e),
    }
}

fn err_of(src: &str) -> String {
    match float_lang::run_text(src) {
        Ok(s) => panic!("本该报错，却跑出了：{}", s),
        Err(e) => e.message,
    }
}

fn compile_err(src: &str) -> String {
    match run_from("main.cw", src, None) {
        Ok(_) => panic!("本该编译期拦下：{}", src),
        Err(e) => e.message,
    }
}

/// 一个进程内只装一次假网络：不用真的连外网，测的是「请求 → 结果」这条链路
fn install_fake_fetch() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        host::set_fetch(Arc::new(|method, url, body| {
            if url.contains("bad.example") {
                return Err(format!("HTTP 500：服务器炸了（{}）", method));
            }
            let b = body.unwrap_or("");
            Ok(format!("[{} {}] body={}", method, url, b))
        }));
    });
}

/* ───────── 文件 ───────── */

fn tmp_dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!(
        "float-lang-test-{}-{}",
        tag,
        std::process::id()
    ));
    let _ = std::fs::create_dir_all(&d);
    d
}

#[test]
fn 文件写入读出与追加() {
    let dir = tmp_dir("fileio");
    let base = dir.clone();
    let src = r#"
writefile "demo.txt" "第一行"
print(readfile "demo.txt")
appendfile "demo.txt" "第二行"
print(readfile "demo.txt")
print(hasfile "demo.txt")
print(hasfile "没有这个.txt")
"#;
    let r = run_from("main.cw", src, Some(&base)).expect("文件脚本应跑通");
    assert_eq!(
        r.output,
        "第一行\n第一行第二行\ntrue\nfalse\n",
        "写入→读出→追加→读出→存在性检查"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn 文件覆盖写不残留旧内容() {
    let dir = tmp_dir("fileio2");
    let base = dir.clone();
    let src = r#"
writefile "a.txt" "旧的很长很长"
writefile "a.txt" "新"
print(readfile "a.txt")
"#;
    let r = run_from("main.cw", src, Some(&base)).expect("应跑通");
    assert_eq!(r.output, "新\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn 读不存在的文件给中文错误() {
    let dir = tmp_dir("fileio3");
    let base = dir.clone();
    let e = match run_from("main.cw", r#"print(readfile "缺.txt")"#, Some(&base)) {
        Ok(_) => panic!("本该报错"),
        Err(e) => e.message,
    };
    assert!(e.contains("缺.txt"), "错误里要带上文件名：{}", e);
    assert!(e.contains("文件不存在") || e.contains("读不到"), "错误要说人话：{}", e);
    let _ = std::fs::remove_dir_all(&dir);
}

/* ───────── 时间 ───────── */

#[test]
fn 时间戳是毫秒级且在合理范围() {
    let v = out("print(now())");
    let ms: i64 = v.trim().parse().expect("时间戳应是整数");
    // 2023-11-14 之后的毫秒数；下限防止"返回了 0/秒级"这种实现滑坡
    assert!(ms > 1_700_000_000_000, "时间戳太小：{}", ms);
    let v2: i64 = out("print(now())").trim().parse().unwrap();
    assert!(v2 >= ms, "时间戳不应倒退：{} → {}", ms, v2);
}

/* ───────── 网络 ───────── */

#[test]
fn 取网页与发网页走宿主钩子() {
    install_fake_fetch();
    let o = out(r#"print(fetch("https://good.example/api"))"#);
    assert_eq!(o, "[GET https://good.example/api] body=\n");
    let o = out(r#"print(post("https://good.example/api", "数据"))"#);
    assert_eq!(o, "[POST https://good.example/api] body=数据\n");
}

#[test]
fn 网络失败能被try接住() {
    install_fake_fetch();
    let o = out(
        r#"
try
    print(fetch("https://bad.example/x"))
catch e
    print("接住了：" + e)
end
"#,
    );
    assert!(o.contains("接住了："), "网络错误应能被 catch：{}", o);
    assert!(o.contains("HTTP 500"), "错误里要有 HTTP 状态：{}", o);
}

/* ───────── 调试输出 ───────── */

#[test]
fn 调试三档输出带级别标记() {
    let o = out(r#"调试 报告 "普通""#);
    assert_eq!(o, "\u{1}1 普通\n");
    let o = out(r#"调试 警告 "注意""#);
    assert_eq!(o, "\u{1}2 注意\n");
    let o = out(r#"调试 报错 "炸了""#);
    assert_eq!(o, "\u{1}3 炸了\n");
}

#[test]
fn 调试支持函数式写法与多参数() {
    let o = out(r#"调试(报告, "函数式")"#);
    assert_eq!(o, "\u{1}1 函数式\n");
    let o = out("int a = 7\n调试 警告 a \"个\"\n");
    assert_eq!(o, "\u{1}2 7 个\n");
    let o = out(r#"调试 报告"#);
    assert_eq!(o, "\u{1}1 \n");
}

#[test]
fn 调试级别词写错在编译期拦下() {
    let e = compile_err(r#"调试 随便 "x""#);
    assert!(e.contains("级别"), "错误要提示写级别：{}", e);
    let e = compile_err(r#"调试(警告 "x")"#);
    assert!(e.contains("逗号"), "函数式少逗号要有提示：{}", e);
}

#[test]
fn 调试报错不中断程序() {
    let o = out(
        r#"
调试 报错 "这只是给开发者看的"
int x = 40
x += 2
print(x)
"#,
    );
    assert!(o.contains("42\n"), "调试报错后程序要继续跑：{}", o);
}

#[test]
fn 调试行不会被当成ask提问() {
    let src = "print(\"欢迎\")\n调试 报错 \"调试行\"\nask \"名字？\"\n";
    let r = run_interactive("main.cw", src, None, vec![], Default::default());
    assert!(r.need_input.is_some(), "应停在 ask 上");
    assert_eq!(r.need_input.as_deref(), Some("名字？"), "调试行不能被当成提问");
}

/* ───────── 类型上界：编译前拦截 ───────── */

#[test]
fn 跨类型字面量赋值编译期拦截() {
    let e = compile_err("int a = 1.5");
    assert!(e.contains("int") && e.contains("float"), "{}", e);
    let e = compile_err(r#"int a = "字符串""#);
    assert!(e.contains("int") && e.contains("str"), "{}", e);
    let e = compile_err(r#"str s = 42"#);
    assert!(e.contains("str") && e.contains("int"), "{}", e);
    let e = compile_err("bool b = 1");
    assert!(e.contains("bool") && e.contains("int"), "{}", e);
}

#[test]
fn 加宽允许_收窄拦截_后续赋值也拦截() {
    // int 字面量给 float：允许（加宽）
    run_from("main.cw", "float f = 1", None).expect("int→float 应放行");
    // 声明后再赋值同样要拦（不只第一行）
    let e = compile_err("int a = 1\na = 2.5");
    assert!(e.contains("int") && e.contains("float"), "{}", e);
    // 变量赋值不确定类型，运行时也不误伤
    assert_eq!(out("int a = 1\na = 2\nprint(a)"), "2\n");
}

/* ───────── alias / #define（文本替换，类似 C 的 #define） ───────── */

#[test]
fn 别名能替换名字与代码片段() {
    assert_eq!(out("alias 两次 = 2\nprint(两次)"), "2\n");
    assert_eq!(out("#define 两次 2\nprint(两次)"), "2\n");
    assert_eq!(out("alias PI = 3.14\nprint(PI)"), "3.14\n");
    // 换成一整条语句：这是 #define 最上头的玩法
    assert_eq!(out("alias 打招呼 = print(\"你好\")\n打招呼"), "你好\n");
    // 换进函数体里也照常
    assert_eq!(
        out("alias 双倍 = x * 2\nfn f(x)\n    return 双倍\n\nprint(f(21))"),
        "42\n"
    );
}

#[test]
fn 中文换写法仍然兼容() {
    assert_eq!(out("换 两次 成 2\nprint(两次)"), "2\n");
}

#[test]
fn 别名只换整词() {
    assert_eq!(out("alias 值 = 1\nint 变量值 = 9\nprint(值, 变量值)"), "1 9\n");
}

#[test]
fn 别名之后行号还是准的() {
    let e = match run_from("main.cw", "alias a = 1\nprint(a)\nprint(1 / 0)", None) {
        Ok(_) => panic!("本该报错"),
        Err(e) => e,
    };
    assert_eq!(e.span.line, 3, "定义行变成空行，但行号不能错位");
}

#[test]
fn 别名写错会明确报错() {
    let e = compile_err("alias a 42");
    assert!(e.contains("alias"), "{}", e);
    let e = compile_err("alias a = b\nalias b = a\nprint(a)");
    assert!(e.contains("环"), "{}", e);
}

/* ───────── 不回退：既有语义不被新东西踩坏 ───────── */

#[test]
fn 新旧内置函数共存() {
    assert_eq!(out("print(len(\"abc\") + strlen(\"de\"))"), "5\n");
    assert_eq!(out("print(type(now()))"), "int\n");
    assert_eq!(out("print(type(hasfile(\"随便\")))"), "bool\n");
}

#[test]
fn 中文内置名作为兼容别名仍然可用() {
    // 早期实现用的是中文名，只在真机上跑过的老脚本还能跑；
    // 正式写法一律英文（补全和文档里只有英文）。
    assert_eq!(out("print(type(时间戳()))"), "int\n");
    assert_eq!(out("print(type(有文件(\"随便\")))"), "bool\n");
}
