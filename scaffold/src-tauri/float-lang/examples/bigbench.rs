// 大文件分阶段计时：解析 / 编译 / 运行，找真正的瓶颈。
use std::time::Instant;


/// Windows：读自己的峰值工作集（不引第三方依赖，两个 API 就够）
#[cfg(windows)]
fn peak_mb() -> f64 {
    #[repr(C)]
    struct Pmc {
        cb: u32,
        page_faults: u32,
        peak_working_set: usize,
        working_set: usize,
        peak_paged_pool: usize,
        paged_pool: usize,
        peak_nonpaged_pool: usize,
        nonpaged_pool: usize,
        pagefile: usize,
        peak_pagefile: usize,
        private: usize,
    }
    extern "system" {
        fn GetCurrentProcess() -> isize;
        fn K32GetProcessMemoryInfo(h: isize, p: *mut Pmc, cb: u32) -> i32;
    }
    let mut m = Pmc {
        cb: std::mem::size_of::<Pmc>() as u32,
        page_faults: 0,
        peak_working_set: 0,
        working_set: 0,
        peak_paged_pool: 0,
        paged_pool: 0,
        peak_nonpaged_pool: 0,
        nonpaged_pool: 0,
        pagefile: 0,
        peak_pagefile: 0,
        private: 0,
    };
    unsafe {
        if K32GetProcessMemoryInfo(GetCurrentProcess(), &mut m, m.cb) != 0 {
            m.peak_working_set as f64 / 1048576.0
        } else {
            0.0
        }
    }
}
#[cfg(not(windows))]
fn peak_mb() -> f64 {
    0.0
}

fn main() {
    let path = std::env::args().nth(1).expect("用法：bigbench 文件.co");
    let src = std::fs::read_to_string(&path).expect("读文件");
    let t0 = Instant::now();
    let prep = float_lang::api::prepare(&path, &src, None).expect("预处理");
    let t_prep = t0.elapsed();
    let t1 = Instant::now();
    eprintln!("[1] 开始解析");
    let prog = float_lang::parser::parse_program(&path, &prep.source);
    eprintln!("[2] 解析完成");
    let t_parse = t1.elapsed();
    assert!(prog.errors.is_empty(), "语法错：{:?}", prog.errors.first().map(|e| e.message.clone()));
    let t2 = Instant::now();
    let (chunk, funcs, errs) = float_lang::bytecode::compile(&prog);
    eprintln!("[3] 编译完成");
    let t_compile = t2.elapsed();
    assert!(errs.is_empty(), "编译错：{:?}", errs.first().map(|e| e.message.clone()));
    // 真实路径（api::run_from）里 AST 在运行前就已经释放；这里也照做，
    // 否则峰值内存会把 AST 和字节码加在一起，量出虚高的数字
    let n_funcs = funcs.len();
    drop(funcs);
    drop(prog);
    let t3 = Instant::now();
    let mut vm = float_lang::Vm::new(chunk);
    let out = vm.run().expect("运行");
    let t_run = t3.elapsed();
    println!(
        "预处理 {:?} / 解析 {:?} / 编译 {:?} / 运行 {:?} | 函数 {} | 输出 {} | 峰值内存 {:.1} MB",
        t_prep, t_parse, t_compile, t_run, n_funcs, out.output.lines().count(), peak_mb()
    );
}
