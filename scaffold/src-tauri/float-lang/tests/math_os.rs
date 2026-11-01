//! Math 与 OS 能力测试，外加"能不能扛住大工程"的规模测试。

use float_lang::api::run_from;

fn out(src: &str) -> String {
    match float_lang::run_text(src) {
        Ok(s) => s,
        Err(e) => panic!("运行失败：{}", e),
    }
}

fn tmp_dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("float-lang-os-{}-{}", tag, std::process::id()));
    let _ = std::fs::create_dir_all(&d);
    d
}

/* ───────── Math ───────── */

#[test]
fn 三角与反三角() {
    assert_eq!(out("print(round(sin(0) * 1000))"), "0\n");
    // sin(pi/2) = 1
    assert!(out("print(near(sin(pi() / 2), 1))").trim() == "true");
    assert!(out("print(near(cos(pi()), -1))").trim() == "true");
    assert!(out("print(near(atan2(1, 1), pi() / 4))").trim() == "true");
    assert!(out("print(near(asin(1), pi() / 2))").trim() == "true");
    assert!(out("print(near(acos(1), 0))").trim() == "true");
    assert!(out("print(near(atan(1), pi() / 4))").trim() == "true");
}

#[test]
fn 对数与指数() {
    assert_eq!(out("print(log2(8))"), "3\n");
    assert_eq!(out("print(log10(1000))"), "3\n");
    assert!(out("print(near(log1p(0), 0))").trim() == "true");
    assert!(out("print(near(expm1(0), 0))").trim() == "true");
    assert!(out("print(near(exp(0), 1))").trim() == "true");
}

#[test]
fn 整数数学() {
    assert_eq!(out("print(gcd(12, 18))"), "6\n");
    assert_eq!(out("print(lcm(4, 6))"), "12\n");
    assert_eq!(out("print(factorial(5))"), "120\n");
    assert_eq!(out("print(comb(5, 2))"), "10\n");
    assert_eq!(out("print(perm(5, 2))"), "20\n");
    assert_eq!(out("print(isqrt(17))"), "4\n");
    assert_eq!(out("print(isqrt(16))"), "4\n");
    assert_eq!(out("print(trunc(-2.7))"), "-2\n");
    assert_eq!(out("print(sign(-9))"), "-1\n");
}

#[test]
fn 数值判断与常量() {
    assert_eq!(out("print(isfinite(1.0))"), "true\n");
    assert_eq!(out("print(isinf(inf()))"), "true\n");
    assert_eq!(out("print(isnan(nan()))"), "true\n");
    assert_eq!(out("print(isnan(1.0))"), "false\n");
    assert!(out("print(near(pi(), 3.141592653589793))").trim() == "true");
    assert!(out("print(near(tau(), pi() * 2))").trim() == "true");
    assert!(out("print(near(e(), 2.718281828459045))").trim() == "true");
    assert!(out("print(near(degrees(pi()), 180))").trim() == "true");
    assert!(out("print(near(radians(180), pi()))").trim() == "true");
}

#[test]
fn hypot_任意多维() {
    assert_eq!(out("print(hypot(3, 4))"), "5\n");
    assert_eq!(out("print(hypot(2, 3, 6))"), "7\n");
    assert_eq!(out("print(hypot(-3, -4))"), "5\n");
}

#[test]
fn 数学错误的说法是人话() {
    let e = match float_lang::run_text("print(factorial(-1))") {
        Err(e) => e.message,
        Ok(_) => panic!("本该报错"),
    };
    assert!(e.contains("factorial"), "{}", e);
    let e = match float_lang::run_text("print(isqrt(-4))") {
        Err(e) => e.message,
        Ok(_) => panic!("本该报错"),
    };
    assert!(e.contains("isqrt"), "{}", e);
}

/* ───────── OS ───────── */

#[test]
fn 目录增删与列举() {
    let dir = tmp_dir("dir");
    let base = dir.clone();
    let src = r#"
print(makedir("sub"))
print(isdir("sub"))
writefile("sub/a.txt", "x")
list names = listdir("sub")
print(names[0])
print(filesize("sub/a.txt"))
print(removefile("sub/a.txt"))
print(removedir("sub"))
print(exists("sub"))
"#;
    let r = run_from("main.cw", src, Some(&base)).expect("目录脚本应跑通");
    assert_eq!(r.output, "true\ntrue\na.txt\n1\ntrue\ntrue\nfalse\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn 路径与环境变量() {
    assert_eq!(out(r#"print(pathjoin("a", "b", "c.txt"))"#), "a/b/c.txt\n");
    assert_eq!(out("print(type(getenv(\"PATH\")))"), "str\n");
    // 读一个肯定不存在的环境变量：给空串，不炸
    assert_eq!(out("print(len(getenv(\"FLOAT_NOT_EXIST_XYZ\")))"), "0\n");
}

#[test]
fn cwd_是脚本所在目录() {
    let dir = tmp_dir("cwd");
    let base = dir.clone();
    let r = run_from("main.cw", "print(cwd())", Some(&base)).expect("应跑通");
    let shown = r.output.trim().replace('\\', "/");
    let want = dir.to_string_lossy().replace('\\', "/");
    assert!(shown.ends_with(&want) || want.ends_with(&shown), "cwd={} want={}", shown, want);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn runcmd_能跑系统命令() {
    let dir = tmp_dir("cmd");
    let base = dir.clone();
    let src = if cfg!(windows) {
        "dict r = runcmd(\"echo hi\")\nprint(r[\"code\"])\nprint(contain(r[\"out\"], \"hi\"))\n"
    } else {
        "dict r = runcmd(\"echo hi\")\nprint(r[\"code\"])\nprint(contain(r[\"out\"], \"hi\"))\n"
    };
    let r = run_from("main.cw", src, Some(&base)).expect("命令应跑通");
    assert_eq!(r.output, "0\ntrue\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn renamefile_能改名() {
    let dir = tmp_dir("rename");
    let base = dir.clone();
    let src = r#"
writefile("old.txt", "内容")
print(renamefile("old.txt", "new.txt"))
print(exists("old.txt"))
print(isfile("new.txt"))
"#;
    let r = run_from("main.cw", src, Some(&base)).expect("应跑通");
    assert_eq!(r.output, "true\nfalse\ntrue\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/* ───────── 规模：能不能扛住真工程 ───────── */

#[test]
fn 上万行单文件能编译能跑() {
    // 5000 个变量声明 + 5000 次累加：一万行级别的脚本
    let mut src = String::new();
    for i in 0..5000 {
        src.push_str(&format!("int v{} = {}\n", i, i % 97));
    }
    src.push_str("int s = 0\n");
    for i in 0..5000 {
        src.push_str(&format!("s += v{}\n", i));
    }
    src.push_str("print(s)\n");

    let t = std::time::Instant::now();
    let r = run_from("main.cw", &src, None).expect("大文件应能跑");
    let cost = t.elapsed();
    let expect: i64 = (0..5000).map(|i| (i % 97) as i64).sum();
    assert_eq!(r.output.trim(), expect.to_string());
    assert!(cost.as_secs() < 5, "一万行跑了 {:?}，太慢", cost);
}

#[test]
fn 深嵌套不会爆栈() {
    // IDE 会在大栈线程里跑（见 lang.rs），这里照样模拟：50 层与 150 层都要能跑
    let run_big = |src: String| -> Result<String, String> {
        std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(move || match run_from("main.cw", &src, None) {
                Ok(o) => Ok(o.output),
                Err(e) => Err(e.message),
            })
            .expect("起线程")
            .join()
            .expect("线程没崩")
    };

    for depth in [50usize, 150] {
        let mut src = String::new();
        for i in 0..depth {
            src.push_str(&"    ".repeat(i));
            src.push_str("if (true)\n");
        }
        src.push_str(&"    ".repeat(depth));
        src.push_str("print(\"深嵌套也能跑\")\n");
        let out = run_big(src).expect("深嵌套应能跑");
        assert_eq!(out, "深嵌套也能跑\n", "{} 层", depth);
    }

    // 再深就该明确报错，而不是把进程栈吃掉
    let depth = 400usize;
    let mut src = String::new();
    for i in 0..depth {
        src.push_str(&"    ".repeat(i));
        src.push_str("if (true)\n");
    }
    src.push_str(&"    ".repeat(depth));
    src.push_str("print(\"太深了\")\n");
    let err = run_big(src).expect_err("超过上限应报错");
    assert!(err.contains("嵌套太深"), "{}", err);
}

#[test]
fn 几十层嵌套_import() {
    // 90 层链式 import（上限 100 层以内）：证明深一点的引用链照样能摊平
    let dir = tmp_dir("manyfiles");
    let base = dir.clone();
    let n = 90;
    for i in 0..n {
        let body = if i + 1 < n {
            format!("import \"f{}.cw\"\nint v{} = {} + 1\n", i + 1, i, 0)
        } else {
            format!("int v{} = 1\n", n - 1)
        };
        std::fs::write(dir.join(format!("f{}.cw", i)), body).expect("写文件");
    }
    let main = "import \"f0.cw\"\nint v0 = 0\nprint(v0)\n".to_string();
    std::fs::write(dir.join("main.cw"), &main).expect("写 main");

    let t = std::time::Instant::now();
    let r = run_from("main.cw", &main, Some(&base)).expect("90 层 import 应能编译");
    let cost = t.elapsed();
    assert_eq!(r.output, "0\n");
    assert!(cost.as_secs() < 10, "90 层 import 用了 {:?}", cost);
    let _ = std::fs::remove_dir_all(&dir);
}

/* ───────── 更大规模：超多文件 / 超长代码 / 深 import 链 ───────── */

#[test]
fn 一千个文件扁平_import() {
    let dir = tmp_dir("thousand");
    let base = dir.clone();
    let n = 1000;
    let mut main = String::new();
    for i in 0..n {
        std::fs::write(dir.join(format!("m{}.cw", i)), format!("fn f{}()\n    return {}\n", i, i))
            .expect("写库文件");
        main.push_str(&format!("import \"m{}.cw\"\n", i));
    }
    main.push_str("int total = 0\n");
    for i in 0..n {
        main.push_str(&format!("total = total + f{}()\n", i));
    }
    main.push_str("print(total)\n");
    std::fs::write(dir.join("main.cw"), &main).expect("写 main");

    let t = std::time::Instant::now();
    let r = run_from("main.cw", &main, Some(&base)).expect("1000 个文件应能编译运行");
    let cost = t.elapsed();
    let want: i64 = (0..n as i64).sum();
    assert_eq!(r.output.trim(), want.to_string());
    assert!(cost.as_secs() < 20, "1000 文件用了 {:?}", cost);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn 五万行单文件() {
    let mut src = String::new();
    for i in 0..25000 {
        src.push_str(&format!("int y{} = {}\n", i, i % 997));
    }
    src.push_str("int total = 0\n");
    for i in 0..25000 {
        src.push_str(&format!("total = total + y{}\n", i));
    }
    src.push_str("print(total > 0)\n");

    let t = std::time::Instant::now();
    let r = run_from("main.cw", &src, None).expect("五万行应能跑");
    let cost = t.elapsed();
    assert_eq!(r.output.trim(), "true");
    assert!(cost.as_secs() < 20, "五万行用了 {:?}", cost);
}

#[test]
fn 超深_import_链给中文报错而不是崩() {
    let dir = tmp_dir("deepimport");
    let base = dir.clone();
    let n = 400;
    for i in 0..n {
        let body = if i + 1 < n {
            format!("import \"d{}.cw\"\nfn g{}(x)\n    return x\n", i + 1, i)
        } else {
            format!("fn g{}(x)\n    return x\n", n - 1)
        };
        std::fs::write(dir.join(format!("d{}.cw", i)), body).expect("写文件");
    }
    let main = "import \"d0.cw\"\nprint(\"ok\")\n";
    std::fs::write(dir.join("main.cw"), main).expect("写 main");
    let e = match run_from("main.cw", main, Some(&base)) {
        Err(e) => e.message,
        Ok(_) => panic!("深链该报错"),
    };
    assert!(e.contains("import"), "{}", e);
    let _ = std::fs::remove_dir_all(&dir);
}
