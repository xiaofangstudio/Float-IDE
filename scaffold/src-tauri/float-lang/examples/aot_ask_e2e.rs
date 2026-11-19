// AOT 的 ask 端到端对拍：真的编一个 exe、真的喂标准输入、真的比输出。
// 用法：cargo run --release --example aot_ask_e2e
//
// 为什么单列一个 example 而不是塞进 cargo test：
// 这里要调 rustc 编一个完整程序（十几秒），放在默认测试里会把日常回归拖慢；
// 但它必须能一条命令跑起来，不能再出现"代码写好了、没人真跑过"的情况。

use float_lang::{aot, parser};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const SRC: &str = r#"ask "你叫啥"
ask "几岁"
print("你好，" + ans)
print("上一句答案：" + __last_ans)
"#;

fn main() {
    let inputs: Vec<String> = vec!["小明".into(), "8".into()];

    // 1) VM 基线：同一条脚本，喂同样的输入
    let vm = float_lang::api::run_source_with_input(SRC, inputs.clone())
        .expect("VM 跑不通，先修 VM");

    // 2) AOT：翻译 → 编 exe → 喂 stdin → 收输出
    let prog = parser::parse_program("ask.co", SRC);
    assert!(prog.errors.is_empty(), "语法错误：{:?}", prog.errors);
    let rust = match aot::emit(&prog) {
        Ok(r) => r,
        Err(e) => {
            println!("❌ AOT 拒绝了这个脚本：{}", e.message);
            std::process::exit(1);
        }
    };
    let out = match build_and_run(&rust, &inputs) {
        Ok(o) => o,
        Err(e) => {
            println!("❌ AOT 产物跑不通：{}", e);
            std::process::exit(1);
        }
    };

    if out.trim_end() == vm.output.trim_end() {
        println!("✅ ask 对拍一致（VM 与 AOT exe）");
        println!("---- 输出 ----\n{}", out);
    } else {
        println!("❌ ask 输出不一致");
        println!("VM :\n{:?}", vm.output);
        println!("AOT:\n{:?}", out);
        std::process::exit(1);
    }
}

fn build_and_run(rust: &str, inputs: &[String]) -> Result<String, String> {
    let dir = std::env::temp_dir().join("copper_aot_ask_e2e");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).map_err(|e| e.to_string())?;
    std::fs::write(
        dir.join("Cargo.toml"),
        format!(
            "[package]\nname = \"askcase\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [dependencies]\nfloat-lang = {{ path = {:?} }}\n\n\
             [[bin]]\nname = \"askcase\"\npath = \"src/main.rs\"\n\n\
             [profile.release]\nopt-level = 3\nlto = true\ncodegen-units = 1\npanic = \"abort\"\n",
            env!("CARGO_MANIFEST_DIR")
        ),
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(dir.join("src").join("main.rs"), rust).map_err(|e| e.to_string())?;

    let build = Command::new("cargo")
        .args(["build", "--release", "-q"])
        .current_dir(&dir)
        .output()
        .map_err(|e| e.to_string())?;
    if !build.status.success() {
        let err = String::from_utf8_lossy(&build.stderr).to_string();
        let e: Vec<&str> = err.lines().filter(|l| l.starts_with("error")).take(4).collect();
        return Err(if e.is_empty() { err } else { e.join(" | ") });
    }

    let exe: PathBuf = dir.join("target").join("release").join("askcase.exe");
    let mut child = Command::new(&exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        // stderr 留着看 panic，但不算对拍内容
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    {
        let mut stdin = child.stdin.take().ok_or("拿不到 stdin")?;
        for line in inputs {
            writeln!(stdin, "{}", line).map_err(|e| e.to_string())?;
        }
    }
    let run = child.wait_with_output().map_err(|e| e.to_string())?;
    if !run.status.success() {
        return Err(format!(
            "运行退出码 {:?}\nstderr: {}",
            run.status.code(),
            String::from_utf8_lossy(&run.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&run.stdout).to_string())
}
