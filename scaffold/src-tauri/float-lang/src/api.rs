//! 对外的一站式入口：源码 → 解析 → 编译 → 运行。
//!
//! Tauri 命令、WASM 导出、测试都走这里，避免每个调用方各拼一遍。

use crate::bytecode;
use crate::error::{FloatError, FResult};
use crate::parser;
use crate::vm::{RunOutcome, Vm};

/// 编译并运行一段 CW Script 源码。
///
/// 编译期有错就直接返回第一个错（不执行半截程序）。
pub fn run_source(src: &str) -> FResult<RunOutcome> {
    let prog = parser::parse_program("main.cw", src);
    if let Some(e) = prog.errors.first() {
        return Err(e.clone());
    }
    let (chunk, _funcs, errs) = bytecode::compile(&prog);
    if let Some(e) = errs.first() {
        return Err(e.clone());
    }
    let mut vm = Vm::new(chunk);
    vm.run()
}

/// 带预设输入跑：脚本里的 `ask` 依次取用 inputs 里的行。
pub fn run_source_with_input(src: &str, inputs: Vec<String>) -> FResult<RunOutcome> {
    let prog = parser::parse_program("main.cw", src);
    if let Some(e) = prog.errors.first() {
        return Err(e.clone());
    }
    let (chunk, _funcs, errs) = bytecode::compile(&prog);
    if let Some(e) = errs.first() {
        return Err(e.clone());
    }
    let mut vm = Vm::new(chunk);
    vm.set_inputs(inputs);
    vm.run()
}

/// 只跑一次、只要输出文本（测试和简单集成用）
pub fn run_text(src: &str) -> Result<String, FloatError> {
    run_source(src).map(|o| o.output)
}
