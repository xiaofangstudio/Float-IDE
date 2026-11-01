//! 语言内核相关命令：把 IDE 的运行（F5）接到 Rust 内核上。
//!
//! 只做"跑一次拿结果"，不做断点/单步——调试（F6）暂时还走旧的 TS 解释器。
//! 这样至少**执行**这件事只有一套实现，不是两套各跑各的。

use serde::Serialize;

#[derive(Serialize)]
pub struct LangDiag {
    pub severity: String,
    pub message: String,
    pub line: u32,
    pub col: u32,
}

#[derive(Serialize)]
pub struct LangRunResult {
    /// 脚本 print 出来的内容
    pub output: String,
    /// 脚本 `报告` 出来的内容
    pub reports: Vec<String>,
    /// 出错信息（已经带好行号和调用链的中文文案），没错就是 null
    pub error: Option<String>,
    /// 出错所在行（给问题面板用）
    pub error_line: u32,
    /// 用了多少条指令（性能参考）
    pub steps: u64,
    /// 停在哪个 ask 上等人回答（有值 = 还没跑完，界面要弹输入框）
    pub need_input: Option<String>,
}

/// 主文件所在目录：import 的相对路径按它找。
fn base_dir(file: &str) -> Option<std::path::PathBuf> {
    let p = std::path::Path::new(file).parent()?;
    if p.as_os_str().is_empty() {
        None
    } else {
        Some(p.to_path_buf())
    }
}

/* ── 网络能力注入 ──
 * 内核保持零依赖，取网页/发网页的实际 HTTP 实现由这里挂进去（进程内只挂一次）。 */
static FETCH_INSTALL: std::sync::Once = std::sync::Once::new();

fn ensure_fetch_installed() {
    FETCH_INSTALL.call_once(|| {
        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(10))
            .build();
        float_lang::host::set_fetch(std::sync::Arc::new(move |method, url, body| {
            let req = match (method, body) {
                ("POST", Some(_)) => agent.post(url),
                _ => agent.get(url),
            };
            let resp = match body {
                Some(b) => req.send_string(b),
                None => req.call(),
            };
            match resp {
                Ok(r) => r.into_string().map_err(|e| format!("读响应失败：{}", e)),
                Err(ureq::Error::Status(code, r)) => {
                    let detail = r.into_string().unwrap_or_default();
                    if detail.chars().count() > 300 {
                        Err(format!("HTTP {}：{}", code, detail.chars().take(300).collect::<String>()))
                    } else {
                        Err(format!("HTTP {}：{}", code, detail))
                    }
                }
                Err(e) => Err(e.to_string()),
            }
        }));
    });
}

fn fail(msg: String, line: u32) -> LangRunResult {
    LangRunResult {
        output: String::new(),
        reports: Vec::new(),
        error: Some(msg),
        error_line: line,
        steps: 0,
        need_input: None,
    }
}

/// 跑一段脚本。`file` 用于报错显示，也用于解析 import 的相对路径。
/// `extra` 是 IDE 当前所有打开的缓冲区（路径 → 源码），
/// 这样"库文件改了还没存盘"也能按新内容跑。
#[tauri::command]
pub fn lang_run(
    file: String,
    source: String,
    inputs: Vec<String>,
    extra: Option<std::collections::HashMap<String, String>>,
) -> LangRunResult {
    let base = base_dir(&file);
    ensure_fetch_installed();
    /* 交互式跑：脚本里的 ask 没有现成答案时先停下，把提问交给界面，
     * 界面拿到回答补进 inputs 再重跑一次（确定性脚本，重跑结果一致）。
     *
     * 整段放到一条大栈线程上跑：词法/语法/编译都是递归下降，几百层缩进的脚本
     * 在默认栈（1~2MB）里可能直接爆栈把整个 IDE 带走。64MB 够几千层用了。
     * 注意：结果里有 Rc（脚本的值），不能跨线程搬，所以组装成纯数据的
     * LangRunResult 也在线程里做完，只把字符串/数字带回来。 */
    let extra = extra.unwrap_or_default();
    let handle = std::thread::Builder::new()
        .name("float-run".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            let r = float_lang::api::run_interactive(&file, &source, base.as_deref(), inputs, extra);
            finish(r)
        });
    match handle {
        Ok(h) => match h.join() {
            Ok(v) => v,
            Err(_) => fail("运行线程崩了（很可能是脚本太深/太大，或者踩到了内核 bug）".into(), 0),
        },
        Err(e) => fail(format!("启动运行线程失败：{}", e), 0),
    }
}

/// 把内核结果整理成给前端的纯数据（这一步在线程里做，避免 Rc 跨线程）
fn finish(r: float_lang::api::InteractiveRun) -> LangRunResult {
    if let Some(e) = r.error {
        return LangRunResult {
            output: r.output,
            reports: Vec::new(),
            error: Some(e.message.clone()),
            error_line: e.span.line,
            steps: 0,
            need_input: None,
        };
    }
    if let Some(prompt) = r.need_input {
        return LangRunResult {
            output: r.output,
            reports: Vec::new(),
            error: None,
            error_line: 0,
            steps: 0,
            need_input: Some(prompt),
        };
    }
    match r.outcome {
        Some(o) => LangRunResult {
            output: o.output,
            reports: o.reports,
            error: None,
            error_line: 0,
            steps: o.steps,
            need_input: None,
        },
        None => fail("运行结束但没有结果".into(), 0),
    }
}

/// 静态检查（不执行）：语法 + 语义，给问题面板用。
#[tauri::command]
pub fn lang_check(file: String, source: String) -> Vec<LangDiag> {
    let base = base_dir(&file);
    let prepared = match float_lang::api::prepare(&file, &source, base.as_deref()) {
        Ok(p) => p,
        Err(e) => {
            return vec![LangDiag {
                severity: "error".into(),
                message: e.message.clone(),
                line: e.span.line,
                col: e.span.col,
            }]
        }
    };
    let prog = float_lang::parser::parse_program(&file, &prepared.source);
    let mut out: Vec<LangDiag> = Vec::new();
    let push = |out: &mut Vec<LangDiag>, severity: &str, message: String, line: u32, col: u32| {
        let (line, owner) = remap(&prepared, line);
        let message = match owner {
            Some(name) if name != file => format!("{}（在 {} 里）", message, name),
            _ => message,
        };
        out.push(LangDiag { severity: severity.into(), message, line, col });
    };
    for e in &prog.errors {
        push(&mut out, "error", e.message.clone(), e.span.line, e.span.col);
    }
    if out.is_empty() {
        let (_chunk, _f, errs) = float_lang::bytecode::compile(&prog);
        for e in &errs {
            push(&mut out, "error", e.message.clone(), e.span.line, e.span.col);
        }
    }
    if out.is_empty() {
        for d in float_lang::sema::analyze(&prog) {
            push(
                &mut out,
                match d.severity {
                    float_lang::sema::Severity::Error => "error",
                    float_lang::sema::Severity::Warning => "warning",
                    float_lang::sema::Severity::Hint => "info",
                },
                d.message.clone(),
                d.line,
                d.col,
            );
        }
    }
    out
}

/// 合并源码里的行号 → (原文件行号, 文件名)
fn remap(p: &float_lang::api::Prepared, line: u32) -> (u32, Option<String>) {
    if line == 0 {
        return (0, None);
    }
    let i = line as usize - 1;
    match (p.line_file.get(i), p.line_local.get(i)) {
        (Some(f), Some(local)) => (
            *local,
            p.files.get(*f as usize).map(|s| s.to_string()),
        ),
        _ => (line, None),
    }
}
