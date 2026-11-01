//! 对外的一站式入口：源码 → 解析 → 编译 → 运行。
//!
//! Tauri 命令、WASM 导出、测试都走这里，避免每个调用方各拼一遍。

use crate::bytecode::{self, Chunk};
use crate::error::{FloatError, FResult};
use crate::import;
use crate::parser;
use crate::sema;
use crate::vm::{RunOutcome, Vm};
use std::path::Path;
use std::rc::Rc;

/// 展开 import 之后得到的"源代码 + 行号归属表"
pub struct Prepared {
    pub source: String,
    pub files: Vec<Rc<str>>,
    pub line_file: Vec<u16>,
    pub line_local: Vec<u32>,
}

/// 把 import 摊平。`base` 是解析相对路径用的目录（主文件所在目录）。
pub fn prepare(file: &str, src: &str, base: Option<&Path>) -> FResult<Prepared> {
    prepare_with(file, src, base, std::collections::HashMap::new())
}

/// 同上，外加一份"内存里的文件内容"：IDE 传当前缓冲区进来，
/// 用户改了库文件还没存盘就直接 F5 也能跑对内容。
pub fn prepare_with(
    file: &str,
    src: &str,
    base: Option<&Path>,
    overlay: std::collections::HashMap<String, String>,
) -> FResult<Prepared> {
    let ex = import::expand_with(file, src, base, overlay)?;
    // 「换 … 成 …」是纯文本替换，必须在解析之前做完；
    // 它保证行数不变，所以下面这套行号归属表照样有效。
    let source = crate::macros::expand(file, &ex.source)?;
    Ok(Prepared {
        source,
        files: ex.files,
        line_file: ex.line_file,
        line_local: ex.line_local,
    })
}

/// 把行号从"合并源码"翻译回"原始文件 + 原始行号"，
/// 报错时用得上：库里的错不能报成主文件里一个不存在的行。
pub fn remap_error(mut e: FloatError, p: &Prepared, main_file: &str) -> FloatError {
    let i = e.span.line as usize - 1;
    if let (Some(f), Some(local)) = (p.line_file.get(i), p.line_local.get(i)) {
        let name = p.files.get(*f as usize).map(|s| s.to_string()).unwrap_or_default();
        e.span.line = *local;
        e.span.end_line = *local;
        if name != main_file {
            e.message = format!("{}（在 {} 里）", e.message, name);
        }
    }
    e
}

/// 编译到字节码（含 import 展开、语法错、语义错的短路）
pub fn compile_source(file: &str, src: &str, base: Option<&Path>) -> FResult<Chunk> {
    compile_source_with(file, src, base, std::collections::HashMap::new())
}

/// 同上，带内存缓冲区
pub fn compile_source_with(
    file: &str,
    src: &str,
    base: Option<&Path>,
    overlay: std::collections::HashMap<String, String>,
) -> FResult<Chunk> {
    let p = prepare_with(file, src, base, overlay)?;
    let prog = parser::parse_program(file, &p.source);
    if let Some(e) = prog.errors.first() {
        return Err(remap_error(e.clone(), &p, file));
    }
    let (mut chunk, _funcs, errs) = bytecode::compile(&prog);
    if let Some(e) = errs.first() {
        return Err(remap_error(e.clone(), &p, file));
    }
    chunk.files = p.files;
    chunk.line_file = p.line_file;
    chunk.line_local = p.line_local;
    Ok(chunk)
}

/// 只做静态检查（不运行）：给 IDE 的「问题」面板用。
pub fn analyze_source(src: &str) -> Vec<sema::Diag> {
    let base = std::env::current_dir().ok();
    let source = match prepare("main.cw", src, base.as_deref()) {
        Ok(p) => p.source,
        Err(_) => return Vec::new(),
    };
    let prog = parser::parse_program("main.cw", &source);
    if !prog.errors.is_empty() {
        // 语法都没过就别做语义分析了，免得报一堆假的
        return Vec::new();
    }
    sema::analyze(&prog)
}

/// 编译并运行一段 CW Script 源码。
///
/// 编译期有错就直接返回第一个错（不执行半截程序）。
pub fn run_source(src: &str) -> FResult<RunOutcome> {
    let base = std::env::current_dir().ok();
    run_from("main.cw", src, base.as_deref())
}

/// 指定主文件（和它的目录）跑：import 的相对路径按这个目录找。
/// 同时把它设为文件内置函数（读文件/写文件…）的相对路径锚点。
pub fn run_from(file: &str, src: &str, base: Option<&Path>) -> FResult<RunOutcome> {
    crate::host::set_base(base);
    let chunk = compile_source(file, src, base)?;
    let mut vm = Vm::new(chunk);
    vm.run()
}

/// 带预设输入跑：脚本里的 `ask` 依次取用 inputs 里的行。
pub fn run_source_with_input(src: &str, inputs: Vec<String>) -> FResult<RunOutcome> {
    let base = std::env::current_dir().ok();
    run_from_with_input("main.cw", src, base.as_deref(), inputs)
}

/// 同上，但可以由调用方指定主文件和目录（IDE 传的就是真实路径）。
pub fn run_from_with_input(
    file: &str,
    src: &str,
    base: Option<&Path>,
    inputs: Vec<String>,
) -> FResult<RunOutcome> {
    let chunk = compile_source(file, src, base)?;
    let mut vm = Vm::new(chunk);
    vm.set_inputs(inputs);
    vm.run()
}

/// 只跑一次、只要输出文本（测试和简单集成用）
pub fn run_text(src: &str) -> Result<String, FloatError> {
    run_source(src).map(|o| o.output)
}

/// 交互式运行的结果：要么跑完，要么停在某个 `ask` 上等人回答。
pub struct InteractiveRun {
    pub outcome: Option<RunOutcome>,
    /// 停在哪个提问上（有值就说明在等人输入）
    pub need_input: Option<String>,
    /// 到目前为止打印出来的内容（挂起时也要留着）
    pub output: String,
    pub error: Option<FloatError>,
}

/// 带 ask 交互的跑法：
/// 没有预设答案的 ask 会让 VM 挂起、把提问交回给调用方；
/// 调用方拿到答案后把 inputs 补一条**从头重跑**——
/// 脚本对同样的输入是确定性的，所以重跑得到的就是同一条路径，
/// 换来的是 VM 不用做「挂起/恢复」那套复杂状态机，也不会漏掉任何运行时状态。
pub fn run_interactive(
    file: &str,
    src: &str,
    base: Option<&Path>,
    inputs: Vec<String>,
    overlay: std::collections::HashMap<String, String>,
) -> InteractiveRun {
    crate::host::set_base(base);
    let chunk = match compile_source_with(file, src, base, overlay) {
        Ok(c) => c,
        Err(e) => {
            return InteractiveRun {
                outcome: None,
                need_input: None,
                output: String::new(),
                error: Some(e),
            }
        }
    };
    let mut vm = Vm::new(chunk);
    vm.set_inputs(inputs);
    vm.set_interactive(true);
    match vm.run() {
        Ok(o) => InteractiveRun {
            output: o.output.clone(),
            outcome: Some(o),
            need_input: None,
            error: None,
        },
        Err(e) => {
            // 注意顺序：take_prompt 要读 out 的最后一行，必须先取提问再清输出
            let prompt = if vm.needs_input() { vm.take_prompt() } else { None };
            let output = vm.take_output();
            if prompt.is_some() || vm.needs_input() {
                InteractiveRun { outcome: None, need_input: prompt, output, error: None }
            } else {
                InteractiveRun { outcome: None, need_input: None, output, error: Some(e) }
            }
        }
    }
}
