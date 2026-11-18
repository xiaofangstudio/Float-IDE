//! AOT 后端：把 Copper AST 直接翻译成 Rust 源码，交给 rustc/LLVM 编译成本机程序。
//!
//! 设计原则（对用户承诺的"别崩"）：
//! 1. **零改动既有路径**——本模块只读 AST，不碰 parser / bytecode / VM 的任何行为；
//! 2. **语义零漂移**——算术、比较、下标、成员、方法全部复用 VM 的实现
//!    （`arith` / `compare` / `index_get` / `member_get` / `*_method`），
//!    生成代码只是"换了种方式调用同一批函数"；
//! 3. **不支持的特性编译期明确报告**（指针 / ask / wait / time read / 嵌套函数等），
//!    由调用方回落 VM 运行——AOT 永远是快路径，VM 永远是兜底。
//!
//! 翻译模型：
//! - 全局变量 → `Globals { v, gi }` 槽位；函数内声明 → Rust 局部变量
//!   （沿用"块里声明的变量块外可见"的泄漏语义：所有声明提升到函数头）
//! - 控制流 → 原生 Rust 控制流，没有指令派发循环
//! - 常量整数 for → i64 原生循环；其余走 f64 通用循环（和 VM 的 RangeNext 同规则）
//! - 函数 → Rust fn，递归深度 thread_local 计数（沿用 400 上限）
//! - 类 → parser 已降级成 `__new_类_` / `__m_类_方法(self, …)`，这里生成
//!   一个沿 `__base` 链派发的方法分派器 + 直接函数调用

use crate::ast::{AssignOp, Expr, ExprKind, Op, Program, Stmt, StmtKind, Target};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

/// 编译期拒绝：调用方据此回落 VM（打印原因，结果照常给出）。
#[derive(Debug, Clone)]
pub struct AotError {
    pub message: String,
    pub line: u32,
}

impl AotError {
    fn unsupported(what: &str, line: u32) -> AotError {
        AotError { message: format!("AOT 暂不支持：{}（第 {} 行）", what, line), line }
    }
    fn msg(message: impl Into<String>) -> AotError {
        AotError { message: message.into(), line: 0 }
    }
}

#[derive(Clone)]
struct FnMeta {
    arity: usize,
    /// 每个参数的默认值表达式（没有就是 None）
    defaults: Vec<Option<Expr>>,
    /// 整型函数特化：参数/局部/返回全是 i64，跨调用边界不经过 Value 装箱。
    /// 由 infer_int_fns 推断（全部保守，宁可不特化也不改变语义）。
    /// 只在单文件模式启用；分块模式一律 false（注册表签名是 Vec<V>）。
    int_fn: bool,
    /// 浮点函数特化：参数/局部/返回全是 f64（与 int_fn 互斥）。
    /// 推断规则同整型（infer_float_fns），但体里禁止引用任何全局变量。
    float_fn: bool,
    /// 有没有延迟参数（by-name）：整型特化不收延迟参数
    has_lazy: bool,
}

struct Cg {
    /// 全部函数（含类降级出来的 __new_ / __m_）
    meta: HashMap<Rc<str>, FnMeta>,
    /// 全局变量，按出现顺序（槽位表）
    globals: Vec<Rc<str>>,
    gslots: HashMap<Rc<str>, usize>,
    /// 能证明是整数的全局变量：单独存 i64 数组，运算不碰 Value 枚举
    int_globals: HashSet<Rc<str>>,
    gi_list: Vec<Rc<str>>,
    gi_slots: HashMap<Rc<str>, usize>,
    /// 当前函数里能证明是整数的局部变量
    int_locals: Option<HashSet<Rc<str>>>,
    /// 整型函数模式：整个函数体只算 i64，不经 Value（由 render_fn_int 使用）
    whole_int: bool,
    /// 浮点函数模式：whole_int=true 时区分 i64 / f64 特化（render_fn_int 的 float 分支）
    float_mode: bool,
    /// 类名集合（__m_类_方法 的前缀）
    classes: HashSet<Rc<str>>,
    out: String,
    indent: usize,
    loop_depth: usize,
    /// 当前函数里声明过的名字；None = 顶层（全走全局槽）
    locals: Option<HashSet<Rc<str>>>,
    uses_mcall: bool,
    uses_dcall: bool,
    /// 分块编译模式：跨块的函数调用要走运行时注册表（间接调用）
    split: bool,
    /// 本块编号（split 时有效）
    my_chunk: usize,
    /// 函数名 → 块编号
    chunk_of: HashMap<Rc<str>, usize>,
    /// try 闭包栈：每进入一个 try 的 Rust 闭包就 push 当时的 loop_depth。
    /// 非空 = 正在闭包体内发射，return/break/continue 要用流向编码穿出闭包。
    try_stack: Vec<u32>,
    /// 内联函数登记表：源码位置 (行, 列) → 隐藏函数名 __lambda_行_列
    lambdas: HashMap<(u32, u32), Rc<str>>,
}

impl Cg {
    fn w(&mut self, line: String) {
        for _ in 0..self.indent {
            self.out.push_str("    ");
        }
        self.out.push_str(&line);
        self.out.push('\n');
    }

    fn slot_of(&mut self, name: &str) -> usize {
        if let Some(i) = self.gslots.get(name) {
            return *i;
        }
        // globals 快照可能是整份 clone 过来的（函数体 Cg 的 gslots 是空的），
        // 不查 vec 就会重复 push，同名变量出现两个槽：顶层写 A 槽、函数读 B 槽
        if let Some(i) = self.globals.iter().position(|n| n.as_ref() == name) {
            self.gslots.insert(Rc::from(name), i);
            return i;
        }
        let i = self.globals.len();
        self.globals.push(Rc::from(name));
        self.gslots.insert(Rc::from(name), i);
        i
    }

    fn is_local(&self, name: &str) -> bool {
        self.locals.as_ref().map(|l| l.contains(name)).unwrap_or(false)
    }

    /// 这个变量是否按 i64 存（能证明是整数）
    fn is_int_var(&self, name: &str) -> bool {
        if self.is_local(name) {
            self.int_locals.as_ref().map(|l| l.contains(name)).unwrap_or(false)
        } else {
            self.int_globals.contains(name)
        }
    }

    /// 整数全局变量的槽位
    fn gi_slot(&mut self, name: &str) -> usize {
        if let Some(i) = self.gi_slots.get(name) {
            return *i;
        }
        let i = self.gi_list.len();
        self.gi_list.push(Rc::from(name));
        self.gi_slots.insert(Rc::from(name), i);
        i
    }

    /// 读一个整数变量（保证返回 i64 表达式）
    fn read_int(&mut self, name: &str) -> String {
        if self.is_local(name) {
            fmt_local(name)
        } else {
            let i = self.gi_slot(name);
            format!("g.gi[{}]", i)
        }
    }

    /// 给整数变量赋值（值必须是 i64 表达式）
    fn write_int(&mut self, name: &str, expr: &str) {
        if self.is_local(name) {
            let l = fmt_local(name);
            self.w(format!("{} = {};", l, expr));
        } else {
            let i = self.gi_slot(name);
            self.w(format!("g.gi[{}] = {};", i, expr));
        }
    }

    /// 给一个名字赋值（局部 → Rust 局部；顶层 → 全局槽）
    /// VM 是值语义：统一 clone 绑定副本，裸变量 RHS 直接 move 会丢源变量（E0382）
    fn assign_var(&mut self, name: &str, val: &str) {
        if self.is_local(name) {
            let l = fmt_local(name);
            self.w(format!("{} = ({}).clone();", l, val));
        } else {
            let i = self.slot_of(name);
            self.w(format!("g.v[{}] = ({}).clone();", i, val));
        }
    }
}

fn fmt_local(name: &str) -> String {
    format!("c_{}", sanitize(name))
}

fn sanitize(name: &str) -> String {
    name.chars().map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' }).collect()
}

fn rust_fn_name(name: &str) -> String {
    format!("cf_{}", sanitize(name))
}

fn lit_str(s: &str) -> String {
    format!("{:?}", s)
}

fn num_lit(f: f64) -> String {
    if f.fract() == 0.0 && f.abs() < 9.0e18 {
        format!("V::Int({}i64)", f as i64)
    } else {
        format!("V::Float({:?})", f)
    }
}

fn em(e: &Expr) -> String {
    format!(".map_err(|m| fe({}, {}, m))?", e.span.line, e.span.col)
}

fn em_ln(line: u32, col: u32) -> String {
    format!(".map_err(|m| fe({}, {}, m))?", line, col)
}

/* ───────────────────────── 前置扫描 ───────────────────────── */

/// try 体里有没有 return / break / continue（这些跨闭包会变语义，直接拒绝）
fn needs_control_flow(body: &[Stmt]) -> bool {
    for s in body {
        let hit = match &s.kind {
            StmtKind::Return(_) | StmtKind::Break | StmtKind::Continue | StmtKind::Stop => true,
            StmtKind::If { branches, else_body } => {
                branches.iter().any(|(_, b)| needs_control_flow(b))
                    || else_body.as_deref().map(needs_control_flow).unwrap_or(false)
            }
            StmtKind::While { body, .. } | StmtKind::Repeat { body, .. } | StmtKind::Group(body) => needs_control_flow(body),
            _ => false,
        };
        if hit {
            return true;
        }
    }
    false
}


/// 扫生成代码里 `g.v[N]` / `g.gi[N]` 的最大下标，把「至少需要的槽数」抬上去。
///
/// 背景：`slot_of` 会在生成函数体时动态新增全局槽，只按顶层语句算出的数量
/// 开 `Globals.v` 就可能越界 panic（嵌套 for 踩过一次）。按最终代码反推是最
/// 可靠的兜底——宁可多开几个 V::Null，也绝不让生成出来的程序崩。
fn needed_slots(code: &str, need_v: &mut usize, need_gi: &mut usize) {
    let b = code.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let (is_v, skip) = if b[i..].starts_with(b"g.v[") {
            (true, 4)
        } else if b[i..].starts_with(b"g.gi[") {
            (false, 5)
        } else {
            i += 1;
            continue;
        };
        let mut j = i + skip;
        let mut n: usize = 0;
        let mut any = false;
        while j < b.len() && b[j].is_ascii_digit() {
            n = n.saturating_mul(10).saturating_add((b[j] - b'0') as usize);
            any = true;
            j += 1;
        }
        if any {
            let need = n + 1;
            if is_v { *need_v = (*need_v).max(need); } else { *need_gi = (*need_gi).max(need); }
        }
        i = if j > i { j } else { i + 1 };
    }
}

fn collect_locals(body: &[Stmt], out: &mut HashSet<Rc<str>>) -> Result<(), AotError> {
    for s in body {
        match &s.kind {
            StmtKind::Decl { name, .. } => {
                out.insert(Rc::clone(name));
            }
            /* 循环变量和循环体里的声明都得算局部：以前这里没递归 body，
             * 嵌套 for 的 j 会被当成全局，生成代码里冒出没登记过的槽号。 */
            StmtKind::ForRange { var, body, .. } => {
                out.insert(Rc::clone(var));
                collect_locals(body, out)?;
            }
            StmtKind::ForIn { var, value_name, body, .. } => {
                out.insert(Rc::clone(var));
                if let Some(v) = value_name {
                    out.insert(Rc::clone(v));
                }
                collect_locals(body, out)?;
            }
            StmtKind::If { branches, else_body } => {
                for (_, b) in branches {
                    collect_locals(b, out)?;
                }
                if let Some(b) = else_body {
                    collect_locals(b, out)?;
                }
            }
            StmtKind::While { body, .. } | StmtKind::Repeat { body, .. } | StmtKind::Group(body) => collect_locals(body, out)?,
            StmtKind::Try { body, catch_name, catch_body, .. } => {
                if let Some(n) = catch_name {
                    out.insert(Rc::clone(n));
                }
                collect_locals(body, out)?;
                collect_locals(catch_body.as_deref().unwrap_or(&[]), out)?;
            }
            StmtKind::FuncDef { .. } => return Err(AotError::unsupported("函数里再定义函数", s.span.line)),
            _ => {}
        }
    }
    Ok(())
}

/// 收集一个作用域里所有"赋值目标"的名字（这些名字可能是新建的全局）
fn collect_assigned(body: &[Stmt], out: &mut HashSet<Rc<str>>) {
    for s in body {
        match &s.kind {
            StmtKind::Assign { target: Target::Var(n), .. } | StmtKind::IncDec { target: Target::Var(n), .. } => {
                out.insert(Rc::clone(n));
            }
            StmtKind::If { branches, else_body } => {
                for (_, b) in branches {
                    collect_assigned(b, out);
                }
                if let Some(b) = else_body {
                    collect_assigned(b, out);
                }
            }
            StmtKind::While { body, .. } | StmtKind::Repeat { body, .. } | StmtKind::Group(body) => collect_assigned(body, out),
            StmtKind::ForRange { body, .. } | StmtKind::ForIn { body, .. } => collect_assigned(body, out),
            StmtKind::Try { body, catch_body, finally, .. } => {
                collect_assigned(body, out);
                collect_assigned(catch_body.as_deref().unwrap_or(&[]), out);
                collect_assigned(finally.as_deref().unwrap_or(&[]), out);
            }
            _ => {}
        }
    }
}

/// 赋值右边是不是"整型形状"（能安全放进 i64）
fn rhs_int_shaped(e: &Expr, ints: &HashSet<Rc<str>>) -> bool {
    match &e.kind {
        ExprKind::Num(f) => f.fract() == 0.0 && f.abs() < 9.0e15,
        ExprKind::Var(n) => ints.contains(n),
        ExprKind::Un { op: Op::Neg, a } => rhs_int_shaped(a, ints),
        ExprKind::Bin { op: Op::Add | Op::Sub | Op::Mul, a, b } => rhs_int_shaped(a, ints) && rhs_int_shaped(b, ints),
        // 取模：整数 % 整数 结果还是整数（VM 侧 arith(Mod) 对 Int/Int 返回 Int）。
        // 除法一律不放行：Copper 的 `/` 会掉进浮点世界，特化成 i64 就回不来了。
        ExprKind::Bin { op: Op::Mod, a, b } => rhs_int_shaped(a, ints) && rhs_int_shaped(b, ints),
        _ => false,
    }
}

/// 把一个作用域（顶层或单个函数体）里"能证明是整数"的变量挑出来。
/// 规则：声明成 int，且所有赋值/自增的右边都是整型形状；有一次不像就整趟放弃（保守）。
fn classify_int_vars(body: &[Stmt]) -> HashSet<Rc<str>> {
    let mut candidates: HashSet<Rc<str>> = HashSet::new();
    fn find_cands(body: &[Stmt], out: &mut HashSet<Rc<str>>) {
        for s in body {
            match &s.kind {
                // 显式 `let i: int` 当然算；`let i = 0` 这种推断型的（TypeName::Var）
                // 只要初始值是整型形状也算——不认它，顶层循环变量永远进不了 gi，
                // 整个 run_top 就一直在 V 枚举上做装箱算术（实测 1 亿次循环 3.5 s）。
                StmtKind::Decl { ty, name, init } => {
                    let declared_int = matches!(
                        ty,
                        crate::TypeName::Int | crate::TypeName::Var | crate::TypeName::Named
                    );
                    if declared_int {
                        let empty: HashSet<Rc<str>> = HashSet::new();
                        let init_ok = match init {
                            Some(e) => rhs_int_shaped(e, &empty),
                            None => false,
                        };
                        if init_ok {
                            out.insert(Rc::clone(name));
                        }
                    } else if matches!(ty, crate::TypeName::Int) {
                        out.insert(Rc::clone(name));
                    }
                }
                // for 变量是 range 产生的整数；for-in 的变量来源不定，不算
                StmtKind::ForRange { var, body, .. } => {
                    out.insert(Rc::clone(var));
                    find_cands(body, out);
                }
                StmtKind::If { branches, else_body } => {
                    for (_, b) in branches {
                        find_cands(b, out);
                    }
                    if let Some(b) = else_body {
                        find_cands(b, out);
                    }
                }
                StmtKind::While { body, .. } | StmtKind::Repeat { body, .. } | StmtKind::Group(body) => find_cands(body, out),
                StmtKind::ForIn { body, .. } => find_cands(body, out),
                StmtKind::Try { body, catch_body, finally, .. } => {
                    find_cands(body, out);
                    find_cands(catch_body.as_deref().unwrap_or(&[]), out);
                    find_cands(finally.as_deref().unwrap_or(&[]), out);
                }
                _ => {}
            }
        }
    }
    find_cands(body, &mut candidates);
    if candidates.is_empty() {
        return candidates;
    }

    // 反复剔除：直到没有变量因为"某次赋值不像整数"被踢掉
    for _ in 0..8 {
        let mut bad: HashSet<Rc<str>> = HashSet::new();
        fn scan_assigns(body: &[Stmt], cands: &HashSet<Rc<str>>, bad: &mut HashSet<Rc<str>>) {
            for s in body {
                match &s.kind {
                    StmtKind::Assign { target: Target::Var(name), value, .. } => {
                        if cands.contains(name) && !rhs_int_shaped(value, cands) {
                            bad.insert(Rc::clone(name));
                        }
                    }
                    StmtKind::Destructure { names, .. } => {
                        for n in names {
                            if cands.contains(n) {
                                bad.insert(Rc::clone(n));
                            }
                        }
                    }
                    StmtKind::ForIn { var, value_name, body, .. } => {
                        if cands.contains(var) {
                            bad.insert(Rc::clone(var));
                        }
                        if let Some(v) = value_name {
                            if cands.contains(v) {
                                bad.insert(Rc::clone(v));
                            }
                        }
                        // 循环体也得扫：这个分支原先吞掉所有 ForIn，后面的递归永远走不到
                        scan_assigns(body, cands, bad);
                    }
                    StmtKind::If { branches, else_body } => {
                        for (_, b) in branches {
                            scan_assigns(b, cands, bad);
                        }
                        if let Some(b) = else_body {
                            scan_assigns(b, cands, bad);
                        }
                    }
                    StmtKind::While { body, .. } | StmtKind::Repeat { body, .. } | StmtKind::Group(body) => scan_assigns(body, cands, bad),
                    StmtKind::ForRange { body, .. } => scan_assigns(body, cands, bad),
                    StmtKind::Try { body, catch_body, finally, .. } => {
                        scan_assigns(body, cands, bad);
                        scan_assigns(catch_body.as_deref().unwrap_or(&[]), cands, bad);
                        scan_assigns(finally.as_deref().unwrap_or(&[]), cands, bad);
                    }
                    _ => {}
                }
            }
        }
        scan_assigns(body, &candidates, &mut bad);
        if bad.is_empty() {
            break;
        }
        let before = candidates.len();
        for b in &bad {
            candidates.remove(b);
        }
        if candidates.len() == before {
            break;
        }
    }
    candidates
}

/* ── 内联函数（fn(x) return …）编译支持 ──
 * VM 在调用点内联展开 lambda（bytecode::emit_hof），AOT 把它编译成
 * 隐藏函数 __lambda_行_列（能参与整型特化推断），表达式位置发射
 * V::Native 引用，运行时经 dcall / call_named 间接调用。 */

/// 遍历一棵表达式树的所有节点（含自身）。
fn for_each_expr(e: &Expr, f: &mut dyn FnMut(&Expr)) {
    f(e);
    match &e.kind {
        ExprKind::Call { callee, args } => {
            for_each_expr(callee, f);
            for a in args {
                for_each_expr(a, f);
            }
        }
        ExprKind::Un { a, .. } => for_each_expr(a, f),
        ExprKind::Bin { a, b, .. } => {
            for_each_expr(a, f);
            for_each_expr(b, f);
        }
        ExprKind::Ternary { cond, a, b } => {
            for_each_expr(cond, f);
            for_each_expr(a, f);
            for_each_expr(b, f);
        }
        ExprKind::List(xs) => {
            for x in xs {
                for_each_expr(x, f);
            }
        }
        ExprKind::Dict(items) => {
            for (k, v) in items {
                for_each_expr(k, f);
                for_each_expr(v, f);
            }
        }
        ExprKind::Idx { obj, index } => {
            for_each_expr(obj, f);
            for_each_expr(index, f);
        }
        ExprKind::Slice { obj, from, to } => {
            for_each_expr(obj, f);
            if let Some(x) = from {
                for_each_expr(x, f);
            }
            if let Some(x) = to {
                for_each_expr(x, f);
            }
        }
        ExprKind::Member { obj, .. } => for_each_expr(obj, f),
        ExprKind::Lambda { params, body } => {
            for p in params {
                if let Some(d) = &p.default {
                    for_each_expr(d, f);
                }
            }
            for_each_expr(body, f);
        }
        ExprKind::New { args, .. } => {
            for a in args {
                for_each_expr(a, f);
            }
        }
        _ => {}
    }
}

/// 遍历赋值目标里的表达式。
fn for_each_target(t: &Target, f: &mut dyn FnMut(&Expr)) {
    match t {
        Target::Var(_) => {}
        Target::Member(obj, _) => for_each_expr(obj, f),
        Target::Idx(obj, idx) => {
            for_each_expr(obj, f);
            for_each_expr(idx, f);
        }
        Target::Deref(e) => for_each_expr(e, f),
    }
}

/// 遍历一段语句里出现的所有表达式（不进 FuncDef——函数体另行收集）。
/// 覆盖不全没关系：漏掉的 lambda 在 emit_expr 处会报「内部错误：内联函数
/// 未登记」整体回落 VM，不会静默算错。
fn for_each_stmt_expr(s: &Stmt, f: &mut dyn FnMut(&Expr)) {
    match &s.kind {
        StmtKind::ExprStmt(e) | StmtKind::Throw(e) | StmtKind::IfOut(e) => for_each_expr(e, f),
        StmtKind::Assign { target, value, .. } => {
            for_each_target(target, f);
            for_each_expr(value, f);
        }
        StmtKind::IncDec { target, .. } => for_each_target(target, f),
        StmtKind::Decl { init: Some(e), .. } => for_each_expr(e, f),
        StmtKind::Print(args) | StmtKind::Report(args) => {
            for a in args {
                for_each_expr(a, f);
            }
        }
        StmtKind::Debug { args, .. } => {
            for a in args {
                for_each_expr(a, f);
            }
        }
        StmtKind::Ask(Some(e)) => for_each_expr(e, f),
        StmtKind::If { branches, else_body } => {
            for (c, b) in branches {
                if let Some(c) = c {
                    for_each_expr(c, f);
                }
                for_each_stmts_expr(b, f);
            }
            if let Some(b) = else_body {
                for_each_stmts_expr(b, f);
            }
        }
        StmtKind::While { cond, body } | StmtKind::WaitUntil { cond, body } => {
            for_each_expr(cond, f);
            for_each_stmts_expr(body, f);
        }
        StmtKind::Repeat { count, body } => {
            for_each_expr(count, f);
            for_each_stmts_expr(body, f);
        }
        StmtKind::ForRange { from, to, step, body, .. } => {
            if let Some(x) = from {
                for_each_expr(x, f);
            }
            for_each_expr(to, f);
            if let Some(x) = step {
                for_each_expr(x, f);
            }
            for_each_stmts_expr(body, f);
        }
        StmtKind::ForIn { iter, body, .. } => {
            for_each_expr(iter, f);
            for_each_stmts_expr(body, f);
        }
        StmtKind::Destructure { value, .. } => for_each_expr(value, f),
        StmtKind::Return(Some(e)) => for_each_expr(e, f),
        StmtKind::Try { body, catch_body, finally, .. } => {
            for_each_stmts_expr(body, f);
            if let Some(cb) = catch_body {
                for_each_stmts_expr(cb, f);
            }
            if let Some(fbody) = finally {
                for_each_stmts_expr(fbody, f);
            }
        }
        StmtKind::Group(b) => {
            for x in b {
                for_each_stmt_expr(x, f);
            }
        }
        _ => {}
    }
}

/// 遍历一段语句的所有表达式。
fn for_each_stmts_expr(body: &[Stmt], f: &mut dyn FnMut(&Expr)) {
    for s in body {
        for_each_stmt_expr(s, f);
    }
}

/// 收集程序里的内联函数：登记成隐藏函数 __lambda_行_列 并加入函数表。
/// 嵌套 lambda 通过「新函数进表后继续扫」的不动点循环覆盖。
fn collect_lambdas(
    top_stmts: &[Stmt],
    fns: &mut Vec<(Rc<str>, Vec<crate::ast::Param>, Vec<Stmt>)>,
) -> HashMap<(u32, u32), Rc<str>> {
    let mut map: HashMap<(u32, u32), Rc<str>> = HashMap::new();
    let mut sources: Vec<Vec<Stmt>> = vec![top_stmts.to_vec()];
    sources.extend(fns.iter().map(|(_, _, b)| b.clone()));
    let mut i = 0;
    while i < sources.len() {
        let mut found: Vec<(Rc<str>, Vec<crate::ast::Param>, Vec<Stmt>)> = Vec::new();
        for_each_stmts_expr(&sources[i], &mut |e| {
            if let ExprKind::Lambda { params, body } = &e.kind {
                let key = (e.span.line, e.span.col);
                if !map.contains_key(&key) {
                    let name: Rc<str> =
                        Rc::from(format!("__lambda_{}_{}", key.0, key.1).as_str());
                    map.insert(key, Rc::clone(&name));
                    let ret =
                        Stmt::new(StmtKind::Return(Some((**body).clone())), e.span.clone());
                    found.push((Rc::clone(&name), params.clone(), vec![ret]));
                }
            }
        });
        for (n, p, b) in found {
            sources.push(b.clone());
            fns.push((n, p, b));
        }
        i += 1;
    }
    map
}

/// 表达式层扫描：& / * 不支持
fn scan_expr(e: &Expr) -> Result<(), AotError> {
    match &e.kind {
        ExprKind::Un { op: Op::Addr, .. } | ExprKind::Un { op: Op::Deref, .. } => {
            Err(AotError::unsupported("指针取地址 / 解引用（& 与 *）", e.span.line))
        }
        ExprKind::List(xs) => {
            for x in xs {
                scan_expr(x)?;
            }
            Ok(())
        }
        ExprKind::Dict(pairs) => {
            for (k, v) in pairs {
                scan_expr(k)?;
                scan_expr(v)?;
            }
            Ok(())
        }
        ExprKind::Un { a, .. } => scan_expr(a),
        ExprKind::Bin { a, b, .. } => {
            scan_expr(a)?;
            scan_expr(b)
        }
        ExprKind::Ternary { cond, a, b } => {
            scan_expr(cond)?;
            scan_expr(a)?;
            scan_expr(b)
        }
        ExprKind::Idx { obj, index } => {
            scan_expr(obj)?;
            scan_expr(index)
        }
        ExprKind::Slice { obj, from, to } => {
            // 切片已经能用 VM 的 slice_value 发射了，这里只做递归检查
            scan_expr(obj)?;
            if let Some(f) = from {
                scan_expr(f)?;
            }
            if let Some(t) = to {
                scan_expr(t)?;
            }
            Ok(())
        }
        ExprKind::Member { obj, .. } => scan_expr(obj),
        ExprKind::Call { callee, args } => {
            if let ExprKind::Var(n) = &callee.kind {
                if &**n == "ask" {
                    return Err(AotError::unsupported("ask（需要交互宿主）", e.span.line));
                }
            }
            scan_expr(callee)?;
            for a in args {
                scan_expr(a)?;
            }
            Ok(())
        }
        ExprKind::New { args, .. } => {
            for a in args {
                scan_expr(a)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn scan_stmt(s: &Stmt, fns: &mut Vec<(Rc<str>, Vec<crate::ast::Param>, Vec<Stmt>)>) -> Result<(), AotError> {
    match &s.kind {
        StmtKind::FuncDef { name, params, body, .. } => {
            fns.push((Rc::clone(name), params.clone(), body.clone()));
            scan_body(body, fns)?;
        }
        StmtKind::Group(b) => {
            for x in b {
                scan_stmt(x, fns)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn scan_body(body: &[Stmt], fns: &mut Vec<(Rc<str>, Vec<crate::ast::Param>, Vec<Stmt>)>) -> Result<(), AotError> {
    for s in body {
        match &s.kind {
            StmtKind::FuncDef { name, params, body: fb, .. } => {
                fns.push((Rc::clone(name), params.clone(), fb.clone()));
                scan_body(fb, fns)?;
            }
            StmtKind::Group(b) => {
                // class 降级出来的函数都在这里；scan_body 已经能处理 FuncDef，别重复收
                scan_body(b, fns)?;
            }
            StmtKind::ExprStmt(e) => scan_expr(e)?,
            StmtKind::Print(args) | StmtKind::Report(args) | StmtKind::Debug { args, .. } => {
                for a in args {
                    scan_expr(a)?;
                }
            }
            StmtKind::Decl { init: Some(e), .. } | StmtKind::Throw(e) | StmtKind::IfOut(e) | StmtKind::Return(Some(e)) => scan_expr(e)?,
            StmtKind::Assign { target, value, .. } => {
                match target {
                    Target::Var(_) => {}
                    Target::Member(obj, _) => scan_expr(obj)?,
                    Target::Idx(obj, idx) => {
                        scan_expr(obj)?;
                        scan_expr(idx)?;
                    }
                    Target::Deref(_) => return Err(AotError::unsupported("指针解引用赋值（*p = …）", s.span.line)),
                }
                scan_expr(value)?;
            }
            StmtKind::IncDec { target, .. } => match target {
                Target::Var(_) => {}
                Target::Member(obj, _) => scan_expr(obj)?,
                Target::Idx(obj, idx) => {
                    scan_expr(obj)?;
                    scan_expr(idx)?;
                }
                Target::Deref(_) => return Err(AotError::unsupported("指针解引用自增（*p += …）", s.span.line)),
            },
            StmtKind::If { branches, else_body } => {
                for (c, b) in branches {
                    if let Some(c) = c {
                        scan_expr(c)?;
                    }
                    scan_body(b, fns)?;
                }
                if let Some(b) = else_body {
                    scan_body(b, fns)?;
                }
            }
            StmtKind::While { cond, body } => {
                scan_expr(cond)?;
                scan_body(body, fns)?;
            }
            StmtKind::Repeat { count, body } => {
                scan_expr(count)?;
                scan_body(body, fns)?;
            }
            StmtKind::ForRange { from, to, step, body, .. } => {
                if let Some(f) = from {
                    scan_expr(f)?;
                }
                scan_expr(to)?;
                if let Some(st) = step {
                    scan_expr(st)?;
                }
                scan_body(body, fns)?;
            }
            StmtKind::ForIn { iter, body, .. } => {
                scan_expr(iter)?;
                scan_body(body, fns)?;
            }
            StmtKind::Destructure { value, .. } => scan_expr(value)?,
            StmtKind::Try { body, catch_body, finally, .. } => {
                scan_body(body, fns)?;
                scan_body(catch_body.as_deref().unwrap_or(&[]), fns)?;
                scan_body(finally.as_deref().unwrap_or(&[]), fns)?;
            }
            StmtKind::Ask(Some(e)) => scan_expr(e)?,
            StmtKind::Ask(None) => {}
            StmtKind::WaitUntil { .. } => return Err(AotError::unsupported("wait until", s.span.line)),
            StmtKind::TimeRead | StmtKind::TimeReset => return Err(AotError::unsupported("time read / time reset", s.span.line)),
            _ => {}
        }
    }
    Ok(())
}

/* ───────────────────────── 整型函数特化 ─────────────────────────
 * 一族"参数/局部/返回全是 i64"的函数：签名直接用 i64，跨调用边界
 * 不经 Value 装箱、不返回 Result、不走 TLS 深度守卫（深度用参数传）。
 * 这是 fib 这类纯计算负载从几十毫秒掉到个位数毫秒的关键。
 *
 * 推断全部保守：任何一条不满足就整体放弃，回落通用路径，绝不改变语义：
 *   - 函数体只出现：整型算术/位运算、整型比较（仅条件位置）、int 声明、
 *     range for、if/while、显式 return（最后一条顶层语句必须是 return）、
 *     以及对其他整型函数的调用（实参个数对齐、被调无默认值/延迟参数）
 *   - 所有声明必须是 int；字符串/容器/成员/下标/方法/内建/try/for-in/
 *     指针/函数值/print 等任何 V 世界的东西一票否决
 *   - 所有调用点（含顶层）传的实参都得是整型形状
 */

/// 表达式是不是"整型形状"（值是 i64）
fn int_shaped(e: &Expr, ints: &HashSet<Rc<str>>) -> bool {
    match &e.kind {
        ExprKind::Num(f) => f.fract() == 0.0 && f.abs() < 9.0e15,
        ExprKind::Var(n) => ints.contains(n),
        ExprKind::Un { op: Op::Neg, a } => int_shaped(a, ints),
        ExprKind::Bin {
            op: Op::Add | Op::Sub | Op::Mul | Op::BitAnd | Op::BitOr | Op::BitXor | Op::Shl | Op::Shr,
            a,
            b,
        } => int_shaped(a, ints) && int_shaped(b, ints),
        _ => false,
    }
}

/// 条件是不是"整型条件"（整型比较 / 布尔字面量 / 与或非）
fn int_cond(e: &Expr, ints: &HashSet<Rc<str>>) -> bool {
    match &e.kind {
        ExprKind::Bool(_) => true,
        ExprKind::Un { op: Op::Not, a } => int_cond(a, ints),
        ExprKind::Bin { op: Op::And | Op::Or, a, b } => int_cond(a, ints) && int_cond(b, ints),
        ExprKind::Bin { op: Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge, a, b } => {
            int_shaped(a, ints) && int_shaped(b, ints)
        }
        _ => false,
    }
}

/// 收集一段语句里所有对用户函数的调用（函数名, 实参表达式）。只看 Call 节点，
/// 函数名被当值引用（`x = fib`）不算调用——那条路由 dcall 包装，签名无碍。
fn collect_call_sites(body: &[Stmt], out: &mut Vec<(Rc<str>, Vec<Expr>)>) {
    fn walk_expr(e: &Expr, out: &mut Vec<(Rc<str>, Vec<Expr>)>) {
        match &e.kind {
            ExprKind::Call { callee, args } => {
                if let ExprKind::Var(n) = &callee.kind {
                    out.push((Rc::clone(n), args.clone()));
                }
                for a in args {
                    walk_expr(a, out);
                }
            }
            ExprKind::Un { a, .. } => walk_expr(a, out),
            ExprKind::Bin { a, b, .. } => {
                walk_expr(a, out);
                walk_expr(b, out);
            }
            ExprKind::Ternary { cond, a, b } => {
                walk_expr(cond, out);
                walk_expr(a, out);
                walk_expr(b, out);
            }
            ExprKind::List(items) => {
                for x in items {
                    walk_expr(x, out);
                }
            }
            ExprKind::Dict(items) => {
                for (k, v) in items {
                    walk_expr(k, out);
                    walk_expr(v, out);
                }
            }
            ExprKind::Idx { obj, index } => {
                walk_expr(obj, out);
                walk_expr(index, out);
            }
            ExprKind::Slice { obj, from, to } => {
                walk_expr(obj, out);
                if let Some(x) = from {
                    walk_expr(x, out);
                }
                if let Some(x) = to {
                    walk_expr(x, out);
                }
            }
            ExprKind::Member { obj, .. } => walk_expr(obj, out),
            ExprKind::Lambda { params, body } => {
                for p in params {
                    if let Some(d) = &p.default {
                        walk_expr(d, out);
                    }
                }
                walk_expr(body, out);
            }
            ExprKind::New { args, .. } => {
                for a in args {
                    walk_expr(a, out);
                }
            }
            _ => {}
        }
    }
    fn walk_target(t: &Target, out: &mut Vec<(Rc<str>, Vec<Expr>)>) {
        match t {
            Target::Var(_) => {}
            Target::Member(obj, _) => walk_expr(obj, out),
            Target::Idx(obj, idx) => {
                walk_expr(obj, out);
                walk_expr(idx, out);
            }
            Target::Deref(e) => walk_expr(e, out),
        }
    }
    fn walk_stmts(body: &[Stmt], out: &mut Vec<(Rc<str>, Vec<Expr>)>) {
        for s in body {
            match &s.kind {
                StmtKind::ExprStmt(e) => walk_expr(e, out),
                StmtKind::Print(args) | StmtKind::Report(args) => {
                    for a in args {
                        walk_expr(a, out);
                    }
                }
                StmtKind::Debug { args, .. } => {
                    for a in args {
                        walk_expr(a, out);
                    }
                }
                StmtKind::Ask(e) => {
                    if let Some(x) = e {
                        walk_expr(x, out);
                    }
                }
                StmtKind::Decl { init, .. } => {
                    if let Some(x) = init {
                        walk_expr(x, out);
                    }
                }
                StmtKind::Assign { target, value, .. } => {
                    walk_target(target, out);
                    walk_expr(value, out);
                }
                StmtKind::IncDec { target, .. } => walk_target(target, out),
                StmtKind::Return(e) => {
                    if let Some(x) = e {
                        walk_expr(x, out);
                    }
                }
                StmtKind::Throw(e) | StmtKind::IfOut(e) => walk_expr(e, out),
                StmtKind::If { branches, else_body } => {
                    for (c, b) in branches {
                        if let Some(c) = c {
                            walk_expr(c, out);
                        }
                        walk_stmts(b, out);
                    }
                    if let Some(b) = else_body {
                        walk_stmts(b, out);
                    }
                }
                StmtKind::While { cond, body } => {
                    walk_expr(cond, out);
                    walk_stmts(body, out);
                }
                StmtKind::WaitUntil { cond, body } => {
                    walk_expr(cond, out);
                    walk_stmts(body, out);
                }
                StmtKind::Repeat { count, body } => {
                    walk_expr(count, out);
                    walk_stmts(body, out);
                }
                StmtKind::ForRange { from, to, step, body, .. } => {
                    if let Some(x) = from {
                        walk_expr(x, out);
                    }
                    walk_expr(to, out);
                    if let Some(x) = step {
                        walk_expr(x, out);
                    }
                    walk_stmts(body, out);
                }
                StmtKind::ForIn { iter, body, .. } => {
                    walk_expr(iter, out);
                    walk_stmts(body, out);
                }
                StmtKind::Destructure { value, .. } => walk_expr(value, out),
                StmtKind::Try { body, catch_body, finally, .. } => {
                    walk_stmts(body, out);
                    if let Some(b) = catch_body {
                        walk_stmts(b, out);
                    }
                    if let Some(b) = finally {
                        walk_stmts(b, out);
                    }
                }
                _ => {}
            }
        }
    }
    walk_stmts(body, out);
}

/// 单个函数的整型纯度检查器
struct IntCheck<'a> {
    meta: &'a HashMap<Rc<str>, FnMeta>,
    known_fns: &'a HashSet<Rc<str>>,
    /// 整型名字全集：locals（含参数）∪ 整型全局
    ints: HashSet<Rc<str>>,
    calls: Vec<(Rc<str>, usize)>,
    /// 浮点模式：字面量任意、允许除法、禁止位运算/移位
    float_mode: bool,
}

fn ic_shaped(e: &Expr, ic: &mut IntCheck) -> bool {
    match &e.kind {
        ExprKind::Num(f) => {
            ic.float_mode || (f.fract() == 0.0 && f.abs() < 9.0e15)
        }
        ExprKind::Var(n) => ic.ints.contains(n),
        ExprKind::Un { op: Op::Neg, a } => ic_shaped(a, ic),
        // 调用也是整型形状：走 ic_expr 校验被调并登记（嵌套调用一起收）
        ExprKind::Call { .. } => ic_expr(e, ic),
        ExprKind::Bin {
            op: Op::Add | Op::Sub | Op::Mul | Op::BitAnd | Op::BitOr | Op::BitXor | Op::Shl | Op::Shr,
            a,
            b,
        } => ic_shaped(a, ic) && ic_shaped(b, ic),
        // 浮点独有：除法（i64 的除法会丢余数，走 V 世界才对）
        ExprKind::Bin { op: Op::Div, a, b } if ic.float_mode => {
            ic_shaped(a, ic) && ic_shaped(b, ic)
        }
        _ => false,
    }
}

fn ic_cond(e: &Expr, ic: &mut IntCheck) -> bool {
    match &e.kind {
        ExprKind::Bool(_) => true,
        ExprKind::Un { op: Op::Not, a } => ic_cond(a, ic),
        ExprKind::Bin { op: Op::And | Op::Or, a, b } => ic_cond(a, ic) && ic_cond(b, ic),
        ExprKind::Bin { op: Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge, a, b } => {
            ic_shaped(a, ic) && ic_shaped(b, ic)
        }
        _ => false,
    }
}

fn ic_expr(e: &Expr, ic: &mut IntCheck) -> bool {
    match &e.kind {
        ExprKind::Call { callee, args } => {
            let ExprKind::Var(n) = &callee.kind else {
                return false; // 间接调用/方法调用
            };
            let Some(m) = ic.meta.get(n) else {
                return false; // 内建函数/未知名
            };
            let has_defaults = m.defaults.iter().any(|d| d.is_some());
            let (arity, has_lazy) = (m.arity, m.has_lazy);
            if has_defaults || has_lazy || args.len() != arity {
                return false;
            }
            ic.calls.push((Rc::clone(n), args.len()));
            args.iter().all(|a| ic_shaped(a, ic))
        }
        _ => ic_shaped(e, ic),
    }
}

fn ic_stmt(s: &Stmt, ic: &mut IntCheck) -> bool {
    match &s.kind {
        StmtKind::Nop | StmtKind::Break | StmtKind::Continue => true,
        StmtKind::Group(b) => ic_stmts(b, ic, false),
        StmtKind::Return(Some(e)) => ic_shaped(e, ic),
        StmtKind::Return(None) => false,
        StmtKind::ExprStmt(e) => ic_expr(e, ic),
        StmtKind::Decl { ty: crate::TypeName::Int, init, .. } if !ic.float_mode => {
            init.as_ref().map(|e| ic_shaped(e, ic)).unwrap_or(true)
        }
        // 浮点函数里所有声明都是 f64 局部：`let s = 0.5`（无显式类型）按
        // init 形状收——VM 里它落 V::Float(0.5)，f64 局部语义一致
        StmtKind::Decl { init, .. } if ic.float_mode => {
            init.as_ref().map(|e| ic_shaped(e, ic)).unwrap_or(true)
        }
        StmtKind::Decl { .. } => false, // 非 int 声明会落进 V 世界
        StmtKind::Assign { target: Target::Var(n), op, value } => {
            if !ic.ints.contains(n) {
                return false; // 写非整型全局：值要落回 V 世界
            }
            match op {
                AssignOp::Set | AssignOp::Add | AssignOp::Sub | AssignOp::Mul => ic_shaped(value, ic),
                // 浮点函数允许 /=（f64 除法无歧义）
                AssignOp::Div if ic.float_mode => ic_shaped(value, ic),
                _ => false, // %= **= 会退回 V 世界
            }
        }
        StmtKind::Assign { .. } => false, // 成员/下标/指针目标
        StmtKind::IncDec { target: Target::Var(n), .. } => ic.ints.contains(n),
        StmtKind::IncDec { .. } => false,
        StmtKind::If { branches, else_body } => {
            branches.iter().all(|(c, b)| {
                c.as_ref().map(|c| ic_cond(c, ic)).unwrap_or(true) && ic_stmts(b, ic, false)
            }) && else_body.as_ref().map(|b| ic_stmts(b, ic, false)).unwrap_or(true)
        }
        StmtKind::While { cond, body } => ic_cond(cond, ic) && ic_stmts(body, ic, false),
        StmtKind::ForRange { var, from, to, step, body, .. } => {
            let ok_bounds = from.as_ref().map(|e| ic_shaped(e, ic)).unwrap_or(true)
                && ic_shaped(to, ic)
                && step.as_ref().map(|e| ic_shaped(e, ic)).unwrap_or(true);
            if !ok_bounds {
                return false;
            }
            // 循环变量在循环体里是数值局部（collect_locals 会把它收进 decls）；
            // 作用域只在本体，检查完就撤出，避免影响同函数后续语句
            let had = ic.ints.contains(var);
            ic.ints.insert(Rc::clone(var));
            let body_ok = ic_stmts(body, ic, false);
            if !had {
                ic.ints.remove(var);
            }
            body_ok
        }
        _ => false,
    }
}

fn ic_stmts(body: &[Stmt], ic: &mut IntCheck, top: bool) -> bool {
    if top {
        // 整型函数不能有隐式 Null 返回：最后一条顶层语句必须是显式 return
        match body.last().map(|s| &s.kind) {
            Some(StmtKind::Return(Some(_))) => {}
            _ => {
                #[cfg(feature = "aot_debug_int")]
                eprintln!("[aot] 拒绝（末句不是 return）");
                return false;
            }
        }
    }
    body.iter().all(|s| ic_stmt(s, ic))
}

/// 单个函数的整型纯度检查：通过返回它调用的函数列表，不通过返回 None。
/// float_mode=true 时是浮点版检查（禁止引用任何全局，允许除法）。
#[allow(clippy::too_many_arguments)]
fn int_fn_check(
    name: &Rc<str>,
    params: &[crate::ast::Param],
    body: &[Stmt],
    meta: &HashMap<Rc<str>, FnMeta>,
    int_globals: &HashSet<Rc<str>>,
    known_fns: &HashSet<Rc<str>>,
    float_mode: bool,
) -> Option<Vec<(Rc<str>, usize)>> {
    if name.starts_with("__m_") || name.starts_with("__new_") {
        #[cfg(feature = "aot_debug_int")]
        eprintln!("[aot] {}: 拒绝（类函数）", name);
        return None; // 类方法/构造器：self 是字典，天生不是整型
    }
    let mut locals = HashSet::new();
    collect_locals(body, &mut locals).ok()?;
    for p in params {
        // 有默认值/延迟参数的函数：调用点要补值或包闭包，整型路径不做
        if p.default.is_some() || p.lazy {
            #[cfg(feature = "aot_debug_int")]
            eprintln!("[aot] {}: 拒绝（默认值/延迟参数）", name);
            return None;
        }
        locals.insert(Rc::clone(&p.name));
    }
    let mut ints = locals.clone();
    // 浮点函数零全局：引用任何全局都会让 f64 世界和 V 世界串线，宁可回落
    if !float_mode {
        ints.extend(int_globals.iter().cloned());
    }
    let mut ic = IntCheck {
        meta,
        known_fns,
        ints,
        calls: Vec::new(),
        float_mode,
    };
    let pure = ic_stmts(body, &mut ic, true);
    #[cfg(feature = "aot_debug_int")]
    if !pure {
        eprintln!("[aot] {}: 拒绝（体不纯）", name);
    }
    if pure {
        // 调用目标必须是用户函数（内建/未知在 ic_expr 里已一票否决）
        if ic.calls.iter().all(|(c, _)| ic.known_fns.contains(c)) {
            return Some(std::mem::take(&mut ic.calls));
        }
        #[cfg(feature = "aot_debug_int")]
        eprintln!("[aot] {}: 拒绝（调用了非用户函数）", name);
    }
    None
}

/// 推断哪些函数可以整体整型特化；通过者标记 FnMeta.int_fn。
/// 只在单文件模式调用（分块模式的注册表签名固定是 Vec<V>）。
fn infer_int_fns(
    meta: &mut HashMap<Rc<str>, FnMeta>,
    fns: &[(Rc<str>, Vec<crate::ast::Param>, Vec<Stmt>)],
    int_globals: &HashSet<Rc<str>>,
    top_stmts: &[Stmt],
) {
    let known_fns: HashSet<Rc<str>> = fns.iter().map(|(n, _, _)| Rc::clone(n)).collect();

    // 第一轮：逐函数自检（调用目标只要"是用户函数"即可，闭合性下一轮收）
    let mut checks: HashMap<Rc<str>, Vec<(Rc<str>, usize)>> = HashMap::new();
    for (name, params, body) in fns {
        if let Some(calls) = int_fn_check(name, params, body, meta, int_globals, &known_fns, false) {
            checks.insert(Rc::clone(name), calls);
        }
    }

    // 顶层调用点也要满足：被调无默认值/延迟参数、实参个数对齐、实参整型形状
    // （顶层没有 Rust 局部，能证明整型的只有整型全局；嵌套调用一并校验）
    let known_set: HashSet<Rc<str>> = fns.iter().map(|(n, _, _)| Rc::clone(n)).collect();
    let mut top_ic = IntCheck {
        meta,
        known_fns: &known_set,
        ints: int_globals.clone(),
        calls: Vec::new(),
        float_mode: false,
    };
    let mut top_calls: Vec<(Rc<str>, Vec<Expr>)> = Vec::new();
    collect_call_sites(top_stmts, &mut top_calls);
    let mut bad: HashSet<Rc<str>> = HashSet::new();
    for (callee, args) in &top_calls {
        if !checks.contains_key(callee) {
            continue;
        }
        let Some(m) = meta.get(callee) else {
            continue;
        };
        if m.defaults.iter().any(|d| d.is_some()) || m.has_lazy || args.len() != m.arity {
            bad.insert(Rc::clone(callee));
            continue;
        }
        if args.iter().any(|a| !ic_expr(a, &mut top_ic)) {
            bad.insert(Rc::clone(callee));
        }
    }
    for n in bad {
        checks.remove(&n);
    }

    // 闭合：被调函数必须也在候选集里；不满足就逐轮级联剔除
    for _ in 0..8 {
        let mut dropped: Vec<Rc<str>> = Vec::new();
        for (n, calls) in &checks {
            if calls.iter().any(|(c, _)| !checks.contains_key(c)) {
                dropped.push(Rc::clone(n));
            }
        }
        if dropped.is_empty() {
            break;
        }
        for b in dropped {
            checks.remove(&b);
        }
    }

    for (name, m) in meta.iter_mut() {
        m.int_fn = checks.contains_key(name);
    }
    #[cfg(feature = "aot_debug_int")]
    eprintln!("[aot] 整型特化候选：{:?}", checks.keys().collect::<Vec<_>>());
}

/// 推断哪些函数可以整体浮点特化；通过者标记 FnMeta.float_fn。
/// 规则与 infer_int_fns 同构，差异：
///   1) 与 int_fn 互斥（整型函数优先——字面量都是整数的函数走 i64 更快）；
///   2) 函数体禁止引用任何全局（f64 世界不读 V 世界，杜绝串线）；
///   3) 顶层调用点实参不做形状校验——V 世界的 Int/Float 经 want_f64_at
///      运行时收敛（Int 自动转 f64，与 VM 的混合算术语义一致）。
fn infer_float_fns(
    meta: &mut HashMap<Rc<str>, FnMeta>,
    fns: &[(Rc<str>, Vec<crate::ast::Param>, Vec<Stmt>)],
    top_stmts: &[Stmt],
) {
    let known_fns: HashSet<Rc<str>> = fns.iter().map(|(n, _, _)| Rc::clone(n)).collect();

    // 第一轮：逐函数自检（只考虑没被整型特化收编的函数）
    let mut checks: HashMap<Rc<str>, Vec<(Rc<str>, usize)>> = HashMap::new();
    for (name, params, body) in fns {
        if meta.get(name).map(|m| m.int_fn).unwrap_or(false) {
            continue;
        }
        if let Some(calls) = int_fn_check(name, params, body, meta, &HashSet::new(), &known_fns, true) {
            checks.insert(Rc::clone(name), calls);
        }
    }

    // 顶层调用点：只要求被调无默认值/延迟参数、实参个数对齐（实参运行时转换）
    let mut bad: HashSet<Rc<str>> = HashSet::new();
    let mut top_calls: Vec<(Rc<str>, Vec<Expr>)> = Vec::new();
    collect_call_sites(top_stmts, &mut top_calls);
    for (callee, args) in &top_calls {
        if !checks.contains_key(callee) {
            continue;
        }
        let Some(m) = meta.get(callee) else {
            continue;
        };
        if m.defaults.iter().any(|d| d.is_some()) || m.has_lazy || args.len() != m.arity {
            bad.insert(Rc::clone(callee));
        }
    }
    for n in bad {
        checks.remove(&n);
    }

    // 闭合：被调函数必须也在浮点候选集里（浮点函数调整型函数 = 拒绝）
    for _ in 0..8 {
        let mut dropped: Vec<Rc<str>> = Vec::new();
        for (n, calls) in &checks {
            if calls.iter().any(|(c, _)| !checks.contains_key(c)) {
                dropped.push(Rc::clone(n));
            }
        }
        if dropped.is_empty() {
            break;
        }
        for b in dropped {
            checks.remove(&b);
        }
    }

    for (name, m) in meta.iter_mut() {
        m.float_fn = checks.contains_key(name);
    }
    #[cfg(feature = "aot_debug_int")]
    eprintln!("[aot] 浮点特化候选：{:?}", checks.keys().collect::<Vec<_>>());
}

/* ───────────────────────── 主入口 ───────────────────────── */

/// 把整棵 AST 翻成一个完整的 Rust 程序（main.rs 内容）。
pub fn emit(prog: &Program) -> Result<String, AotError> {
    if let Some(e) = prog.errors.first() {
        return Err(AotError::msg(format!("语法有错，先修语法：{}", e.message)));
    }

    // 收集函数（含类降级的 __new_ / __m_）+ 扫不支持的特性
    let mut fns: Vec<(Rc<str>, Vec<crate::ast::Param>, Vec<Stmt>)> = Vec::new();
    scan_body(&prog.body, &mut fns)?;

    // 内联函数（fn(x) return …）：编译成隐藏函数并登记，表达式位置发射
    // V::Native 引用。必须在 meta 构建之前做，lambda 才能参与特化推断。
    let top_stmts0: Vec<Stmt> = prog
        .body
        .iter()
        .filter(|s| !matches!(s.kind, StmtKind::FuncDef { .. }))
        .cloned()
        .collect();
    let lambda_map = collect_lambdas(&top_stmts0, &mut fns);

    let mut meta: HashMap<Rc<str>, FnMeta> = HashMap::new();
    let mut methods: Vec<(String, String, usize)> = Vec::new();
    let mut classes: HashSet<Rc<str>> = HashSet::new();
    for (name, params, _) in &fns {
        meta.insert(
            Rc::clone(name),
            FnMeta {
                arity: params.len(),
                defaults: params.iter().map(|p| p.default.clone()).collect(),
                int_fn: false,
        float_fn: false,
                has_lazy: params.iter().any(|p| p.lazy),
            },
        );
        if let Some(n) = name.strip_prefix("__m_") {
            if let Some(pos) = n.rfind('_') {
                let cls = n[..pos].to_string();
                let meth = n[pos + 1..].to_string();
                // 方法的参数个数：签名里第 0 个是 self
                methods.push((cls.clone(), meth.clone(), params.len().saturating_sub(1)));
                classes.insert(Rc::from(cls.as_str()));
            }
        }
    }

    // 顶层能证明是整数的变量：单独存 i64
    let top_stmts: Vec<Stmt> = prog
        .body
        .iter()
        .filter(|s| !matches!(s.kind, StmtKind::FuncDef { .. }))
        .cloned()
        .collect();
    let int_globals = classify_int_vars(&top_stmts);
    // gi 索引一次算定：所有函数共用同一套编号
    let mut gi_sorted: Vec<Rc<str>> = int_globals.iter().cloned().collect();
    gi_sorted.sort();
    let gi_list0 = gi_sorted.clone();
    let mut gi_slots0: HashMap<Rc<str>, usize> = HashMap::new();
    for (i, n) in gi_list0.iter().enumerate() {
        gi_slots0.insert(Rc::clone(n), i);
    }

    // 整型函数特化：能整族 i64 直通的函数在此标记（单文件模式专属）
    infer_int_fns(&mut meta, &fns, &int_globals, &top_stmts);
    // 浮点函数特化：f64 直通（与整型互斥，单文件模式专属）
    infer_float_fns(&mut meta, &fns, &top_stmts);

    let mut cg = Cg {
        meta,
        globals: Vec::new(),
        gslots: HashMap::new(),
        int_globals: int_globals.clone(),
        gi_list: gi_list0.clone(),
        gi_slots: gi_slots0.clone(),
        int_locals: None,
        classes,
        out: String::new(),
        indent: 1,
        loop_depth: 0,
        locals: None, // 顶层：没有 Rust 局部，全走全局槽
        uses_mcall: false,
        uses_dcall: false,
        whole_int: false,
        float_mode: false,
        split: false,
        my_chunk: 0,
        chunk_of: HashMap::new(),
            try_stack: Vec::new(),
            lambdas: lambda_map.clone(),
    };

    // 函数体里"赋值给没声明过的名字"会在 VM 里创建全局；AOT 提前把这些名字登记成槽位
    {
        let mut extra: HashSet<Rc<str>> = HashSet::new();
        for (_, _, body) in &fns {
            let mut locals_here = HashSet::new();
            collect_locals(body, &mut locals_here)?;
            let mut assigned = HashSet::new();
            collect_assigned(body, &mut assigned);
            for n in assigned {
                if !locals_here.contains(&n) {
                    extra.insert(n);
                }
            }
        }
        let mut names: Vec<Rc<str>> = extra.into_iter().collect();
        names.sort();
        for n in names {
            cg.slot_of(&n);
        }
    }

    // 顶层主体（FuncDef 单独出）
    let top: Vec<&Stmt> = prog
        .body
        .iter()
        .filter(|s| !matches!(s.kind, StmtKind::FuncDef { .. }))
        .collect();
    emit_stmts(&mut cg, &top, true)?;
    let body_code = std::mem::take(&mut cg.out);
    let mut uses_mcall = cg.uses_mcall;
    let mut uses_dcall = cg.uses_dcall;
    let globals = std::mem::take(&mut cg.globals);
    let n_globals = globals.len();
    let n_gi = gi_list0.len();

    // ── 各函数 ──
    let mut fn_code = Vec::new();
    for (name, params, body) in &fns {
        // 整型函数特化：签名/返回全是 i64，跨调用边界不装箱（见 infer_int_fns）
        if cg.meta.get(name).map(|m| m.int_fn).unwrap_or(false) {
            fn_code.push(render_fn_int(
                &cg.meta,
                &globals,
                &int_globals,
                &gi_list0,
                &gi_slots0,
                &cg.classes,
                name,
                params,
                body,
                &lambda_map,
                false,
            )?);
            continue;
        }
        // 浮点函数特化：签名/返回全是 f64（见 infer_float_fns）
        if cg.meta.get(name).map(|m| m.float_fn).unwrap_or(false) {
            fn_code.push(render_fn_int(
                &cg.meta,
                &globals,
                &int_globals,
                &gi_list0,
                &gi_slots0,
                &cg.classes,
                name,
                params,
                body,
                &lambda_map,
                true,
            )?);
            continue;
        }
        let int_locals = classify_int_vars(body);
        let mut fcg = Cg {
            meta: cg.meta.clone(),
            globals: globals.clone(),
            gslots: HashMap::new(),
            int_globals: int_globals.clone(),
            gi_list: gi_list0.clone(),
            gi_slots: gi_slots0.clone(),
            int_locals: Some(int_locals.clone()),
            classes: cg.classes.clone(),
            out: String::new(),
            indent: 1,
            loop_depth: 0,
            locals: Some(HashSet::new()),
            uses_mcall: false,
            uses_dcall: false,
            whole_int: false,
        float_mode: false,
            split: false,
            my_chunk: 0,
            chunk_of: HashMap::new(),
            try_stack: Vec::new(),
            lambdas: lambda_map.clone(),
        };
        for g in &globals {
            fcg.slot_of(g);
        }
        let mut locals = HashSet::new();
        collect_locals(body, &mut locals)?;
        for p in params {
            locals.insert(Rc::clone(&p.name));
        }
        fcg.locals = Some(locals.clone());

        let _ = &int_locals;

        // 递归深度护栏（沿用 VM 的 400 上限）
        let mut sig = String::new();
        sig.push_str(&format!(
            "#[allow(unused_mut)]\nfn {}(g: &mut Globals",
            rust_fn_name(name)
        ));
        for p in params {
            sig.push_str(&format!(", mut c_{}: V", sanitize(&p.name)));
        }
        sig.push_str(") -> R<V> {\n");
        sig.push_str(&format!("    let _guard = enter({})?;\n", lit_str(name)));
        fcg.out.push_str(&sig);

        // 局部变量统一在函数头声明（块内声明也要活到函数外——和 VM 的泄漏语义一致）
        let mut decls: Vec<String> = Vec::new();
        for l in &locals {
            if params.iter().any(|p| &p.name == l) {
                continue; // 参数已经在签名里
            }
            if int_locals.contains(l) {
                decls.push(format!("    let mut {}: i64 = 0;", fmt_local(l)));
            } else {
                decls.push(format!("    let mut {}: V = V::Null;", fmt_local(l)));
            }
        }
        decls.sort();
        for d in decls {
            fcg.out.push_str(&d);
            fcg.out.push('\n');
        }

        emit_stmt_slice(&mut fcg, body, false)?;
        fcg.out.push_str("    Ok(V::Null)\n}\n");
        fn_code.push(std::mem::take(&mut fcg.out));
        // 函数体里的方法调用 / 函数值调用也要并入全局 flag：
        // mcall / dcall 的运行时支撑是按整份程序生成一次的，漏报就是 E0425
        uses_mcall = uses_mcall || fcg.uses_mcall;
        uses_dcall = uses_dcall || fcg.uses_dcall;
    }

    /* 兜底：按最终生成的代码反推槽数。顶层算出的 n_globals 可能漏掉函数体里
     * 动态新增的槽，少开一格就是一次越界 panic。 */
    let mut n_globals = n_globals;
    let mut n_gi = n_gi;
    needed_slots(&body_code, &mut n_globals, &mut n_gi);
    for f in &fn_code {
        needed_slots(f, &mut n_globals, &mut n_gi);
    }

    /* ── 组装 ── */
    let mut doc: Vec<String> = Vec::new();
    doc.push("// 本文件由 Float IDE AOT 自动生成（Copper → Rust）。要改逻辑请改源 .co 再编译。".into());
    doc.push("#![allow(dead_code, unused_imports, unused_variables, unused_mut, unused_parens, non_snake_case, unreachable_patterns, unreachable_code, unused_assignments)]".into());
    doc.push(String::new());
    doc.push(PRELUDE.into());
    doc.push(String::new());

    if uses_mcall {
        doc.push(gen_mcall(&methods, false));
        doc.push(String::new());
    }
    if uses_dcall {
        doc.push(gen_dcall(&cg.meta));
        doc.push(String::new());
    }

    
    doc.push(String::new());
    doc.push("fn main() {".into());
    // 大栈线程：深递归专用。512MB 只是虚拟预留，Windows/Linux 都按需提交，
    // 递归不深就几乎不占内存；栈溢出仍有 INT_CALL_DEPTH 计数兜底，不会 abort。
    doc.push("    let __h = std::thread::Builder::new().stack_size(512 * 1024 * 1024).spawn(|| {".into());
    doc.push(format!(
        "    let mut g = Globals {{ v: vec![V::Null; {}], gi: vec![0i64; {}], err: 0 }};",
        n_globals, n_gi
    ));
    doc.push("    match run_top(&mut g) {".into());
    doc.push("        Ok(_) => {}".into());
    doc.push("        Err(e) => {".into());
    doc.push("            use std::io::Write as _; let _ = std::io::stdout().flush();".into());
    doc.push("            eprintln!(\"出错（第 {} 行）：{}\", e.span.line, e.message);".into());
    doc.push("            std::process::exit(1);".into());
    doc.push("        }".into());
    doc.push("    }".into());
    // 整型特化路径的错误出口：err=2 整数溢出、err=3 除零，其余是递归超限
    doc.push("    if g.err == 2 {".into());
    doc.push("        use std::io::Write as _; let _ = std::io::stdout().flush();".into());
    doc.push("        eprintln!(\"出错：整数溢出（超出 i64 范围）\");".into());
    doc.push("        std::process::exit(1);".into());
    doc.push("    }".into());
    doc.push("    if g.err == 3 {".into());
    doc.push("        use std::io::Write as _; let _ = std::io::stdout().flush();".into());
    doc.push("        eprintln!(\"出错：除数不能是 0\");".into());
    doc.push("        std::process::exit(1);".into());
    doc.push("    }".into());
    doc.push("    if g.err != 0 {".into());
    doc.push("        use std::io::Write as _; let _ = std::io::stdout().flush();".into());
    doc.push("        eprintln!(\"递归太深（上限 {} 层）\", float_lang::limits::INT_CALL_DEPTH);".into());
    doc.push("        std::process::exit(1);".into());
    doc.push("    }".into());
    doc.push("    }).unwrap();".into());
    doc.push("    __h.join().unwrap();".into());
    doc.push("}".into());
    doc.push(String::new());
    doc.push("#[allow(unused_mut)]".into());
    doc.push("fn run_top(g: &mut Globals) -> R<V> {".into());
    doc.push(body_code);
    doc.push("    out_flush();".into());
    doc.push("    Ok(V::Null)".into());
    doc.push("}".into());
    doc.push(String::new());
    doc.push(fn_code.join(""));

    let out = doc.join("\n");

    /* 体积闸门：生成代码太大时 rustc 会吃几个 G 内存（实测把整台机器顶崩过）。
     * 超过阈值就直接拒绝，让上层回落 VM —— 结果一样，但不会把用户的电脑拖死。
     * 想强制走单文件（跨块调用会变成按名字查表的间接调用，慢一档）时，
     * 可以用 COPPER_AOT_MAX_LINES / COPPER_AOT_MAX_BYTES 临时放宽。 */
    let max_lines = std::env::var("COPPER_AOT_MAX_LINES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(4_000);
    let max_bytes = std::env::var("COPPER_AOT_MAX_BYTES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(400_000);
    let lines = out.lines().count();
    if out.len() > max_bytes || lines > max_lines {
        return Err(AotError::msg(format!(
            "脚本太大，AOT 跳过（生成 {} 行代码；超过 {} 行或 {} KB 就改用 VM 跑，免得编译吃爆内存）",
            lines,
            max_lines,
            max_bytes / 1024
        )));
    }

    Ok(out)
}

/* ───────────────────────── 语句生成 ───────────────────────── */

fn emit_stmts(cg: &mut Cg, body: &[&Stmt], top_level: bool) -> Result<(), AotError> {
    for s in body {
        emit_stmt(cg, s, top_level)?;
    }
    Ok(())
}

fn emit_stmt_slice(cg: &mut Cg, body: &[Stmt], top_level: bool) -> Result<(), AotError> {
    for s in body {
        emit_stmt(cg, s, top_level)?;
    }
    Ok(())
}

fn emit_stmts_owned(cg: &mut Cg, body: &[Stmt], top_level: bool) -> Result<(), AotError> {
    for s in body {
        emit_stmt(cg, s, top_level)?;
    }
    Ok(())
}

fn emit_stmt(cg: &mut Cg, s: &Stmt, top_level: bool) -> Result<(), AotError> {
    /* 整型函数模式：语句集收窄到纯 i64 世界。Decl/Assign/IncDec/If/While/
     * ExprStmt/Group 原有发射路径本身就支持整型变量（is_int_var 快路径），
     * 直接放行；Return/ForRange 走专用整型发射；其余语句推断器保证不会出现，
     * 万一出现就在这里显式报错让上层回落 VM，绝不生成错误代码。 */
    if cg.whole_int {
        match &s.kind {
            StmtKind::Nop | StmtKind::Break | StmtKind::Continue | StmtKind::Group(_)
            | StmtKind::Decl { .. } | StmtKind::Assign { .. } | StmtKind::IncDec { .. }
            | StmtKind::If { .. } | StmtKind::While { .. } | StmtKind::ExprStmt(_) => {}
            StmtKind::Return(e) => {
                let bad = |line: u32| {
                    AotError::msg(format!(
                        "内部错误：整型函数的 return 不是整型表达式（第 {} 行）",
                        line
                    ))
                };
                let x = match e {
                    // whole_int 模式下 emit_expr 直接出 i64 代码（调用走 emit_call_i64）
                    Some(x) => emit_expr(cg, x)?,
                    None => return Err(bad(s.span.line)),
                };
                cg.w(format!("return ({});", x));
                return Ok(());
            }
            StmtKind::ForRange { var, from, to, step, inclusive, body } => {
                return emit_for_range_int(
                    cg,
                    var,
                    from.as_ref(),
                    to,
                    step.as_ref(),
                    *inclusive,
                    body,
                    top_level,
                );
            }
            _ => {
                return Err(AotError::unsupported("整型函数里出现非整型语句", s.span.line));
            }
        }
    }
    match &s.kind {
        StmtKind::Nop => Ok(()),
        StmtKind::Group(b) => emit_stmts_owned(cg, b, top_level),
        StmtKind::Decl { name, ty, init } => {
            let is_int = cg.is_int_var(name);
            match (is_int, init) {
                (true, Some(e)) => {
                    /* 整型函数模式：emit_expr 直接给 i64 代码，不能再套 want_int_at
                     * （那玩意儿收 &Value，而且要 `?`，i64 函数里用不了） */
                    if cg.whole_int {
                        let x = emit_expr(cg, e)?;
                        cg.write_int(name, &x);
                    } else if let Some(x) = crate::aot::int_expr(cg, e) {
                        cg.write_int(name, &x);
                    } else {
                        let v = emit_expr(cg, e)?;
                        let expr = format!("want_int_at(&({}), {}, {})?", v, s.span.line, s.span.col);
                        cg.write_int(name, &expr);
                    }
                }
                (true, None) => cg.write_int(name, if cg.float_mode { "0.0" } else { "0" }),
                (false, Some(e)) => {
                    let v = emit_expr(cg, e)?;
                    cg.assign_var(name, &v);
                }
                /* `list xs` / `str s` / `dict d` 不给初值时要按类型给空容器，
                 * 不能一律 null —— 不然一句 `add 3 to xs` 就崩在 null 上。
                 * 这条和 VM 的默认值必须一模一样（VM 那边是 BuildList(0) 之类）。 */
                (false, None) => {
                    let d = match ty {
                        crate::TypeName::List => "V::List(Rc::new(std::cell::RefCell::new(Vec::new())))",
                        crate::TypeName::Dict => "V::Dict(Rc::new(std::cell::RefCell::new(Dict::new())))",
                        crate::TypeName::Str => "V::Str(Rc::new(String::new()))",
                        crate::TypeName::Bool => "V::Bool(false)",
                        crate::TypeName::Float => "V::Float(0.0)",
                        crate::TypeName::Int => "V::Int(0)",
                        _ => "V::Null",
                    };
                    cg.assign_var(name, d)
                }
            }
            Ok(())
        }
        StmtKind::Assign { target, op, value } => emit_assign(cg, target, *op, value, s.span.line),
        StmtKind::IncDec { target, delta, .. } => {
            let one = Expr::new(ExprKind::Num(1.0), s.span.clone());
            let op = if *delta > 0 { AssignOp::Add } else { AssignOp::Sub };
            emit_assign(cg, target, op, &one, s.span.line)
        }
        StmtKind::ExprStmt(e) => {
            let v = emit_expr(cg, e)?;
            cg.w(format!("let _ = {};", v));
            Ok(())
        }
        StmtKind::Print(args) => {
            let items = emit_args(cg, args)?;
            let cloned: Vec<String> = items.iter().map(|x| format!("({}).clone()", x)).collect();
            cg.w(format!("out_line(&show(&[{}]));", cloned.join(", ")));
            Ok(())
        }
        StmtKind::Report(args) => {
            let items = emit_args(cg, args)?;
            let cloned: Vec<String> = items.iter().map(|x| format!("({}).clone()", x)).collect();
            cg.w(format!("eprintln!(\"[报告] {{}}\", show(&[{}]));", cloned.join(", ")));
            Ok(())
        }
        StmtKind::Debug { level, args } => {
            let items = emit_args(cg, args)?;
            let tag = match level {
                2 => "警告",
                3 => "报错",
                _ => "调试",
            };
            let cloned: Vec<String> = items.iter().map(|x| format!("({}).clone()", x)).collect();
            cg.w(format!("eprintln!(\"[{}] {{}}\", show(&[{}]));", tag, cloned.join(", ")));
            Ok(())
        }
        StmtKind::If { branches, else_body } => {
            for (idx, (cond, body)) in branches.iter().enumerate() {
                match cond {
                    Some(c) => {
                        let cc = emit_cond(cg, c)?;
                        if idx == 0 {
                            cg.w(format!("if {} {{", cc));
                        } else {
                            cg.w(format!("}} else if {} {{", cc));
                        }
                    }
                    None => {
                        if idx == 0 {
                            cg.w("if true {".into());
                        } else {
                            cg.w("} else {".into());
                        }
                    }
                }
                cg.indent += 1;
                emit_stmts_owned(cg, body, top_level)?;
                cg.indent -= 1;
            }
            if let Some(b) = else_body {
                cg.w("} else {".into());
                cg.indent += 1;
                emit_stmts_owned(cg, b, top_level)?;
                cg.indent -= 1;
            }
            cg.w("}".into());
            Ok(())
        }
        StmtKind::While { cond, body } => {
            let cc = emit_cond(cg, cond)?;
            cg.w(format!("while {} {{", cc));
            cg.indent += 1;
            cg.loop_depth += 1;
            emit_stmts_owned(cg, body, top_level)?;
            cg.loop_depth -= 1;
            cg.indent -= 1;
            cg.w("}".into());
            Ok(())
        }
        StmtKind::Repeat { count, body } => {
            let cc = emit_expr(cg, count)?;
            cg.w(format!("for __rep in 0..as_i64(&({}), {}, {})? {{", cc, s.span.line, s.span.col));
            cg.indent += 1;
            cg.loop_depth += 1;
            emit_stmts_owned(cg, body, top_level)?;
            cg.loop_depth -= 1;
            cg.indent -= 1;
            cg.w("}".into());
            Ok(())
        }
        StmtKind::ForRange { var, from, to, step, inclusive, body } => {
            emit_for_range(cg, var, from.as_ref(), to, step.as_ref(), *inclusive, body, top_level)
        }
        StmtKind::ForIn { var, value_name, iter, body } => {
            let it = emit_expr(cg, iter)?;
            match value_name {
                None => {
                    // 流式迭代：持引用按需取，不先整体 clone 容器（语义对齐 VM 的 IterSrc）
                    cg.w(format!("for __item in iter_src(&({})){} {{", it, em(iter)));
                    cg.assign_var(var, "__item.clone()");
                }
                Some(vn) => {
                    cg.w(format!("for (__k, __v) in iter_pair_src(&({})){} {{", it, em(iter)));
                    cg.assign_var(var, "__k.clone()");
                    cg.assign_var(vn, "__v.clone()");
                }
            }
            cg.indent += 1;
            cg.loop_depth += 1;
            emit_stmts_owned(cg, body, top_level)?;
            cg.loop_depth -= 1;
            cg.indent -= 1;
            cg.w("}".into());
            Ok(())
        }
        StmtKind::Destructure { names, value } => {
            let v = emit_expr(cg, value)?;
            cg.w(format!("let __dz = {};", v));
            for (i, n) in names.iter().enumerate() {
                let code = format!("index_get(&__dz, &V::Int({}i64)){}", i, em_ln(s.span.line, s.span.col));
                cg.assign_var(n, &code);
            }
            Ok(())
        }
        StmtKind::Break => {
            if cg.loop_depth == 0 {
                return Err(AotError::unsupported("循环外的 break", s.span.line));
            }
            // break 的目标是 try 闭包外的循环：用流向编码穿出闭包（VM 同语义）
            if cg.try_stack.last() == Some(&(cg.loop_depth as u32)) {
                cg.w("return Ok((Ok(V::Null), 2u8));".into());
            } else {
                cg.w("break;".into());
            }
            Ok(())
        }
        StmtKind::Continue => {
            if cg.loop_depth == 0 {
                return Err(AotError::unsupported("循环外的 continue", s.span.line));
            }
            if cg.try_stack.last() == Some(&(cg.loop_depth as u32)) {
                cg.w("return Ok((Ok(V::Null), 3u8));".into());
            } else {
                cg.w("continue;".into());
            }
            Ok(())
        }
        StmtKind::Stop => {
            cg.w("use std::io::Write as _; let _ = std::io::stdout().flush(); std::process::exit(0);".into());
            Ok(())
        }
        StmtKind::IfOut(cond) => {
            if cg.loop_depth == 0 {
                return Err(AotError::unsupported("循环外的 if out", s.span.line));
            }
            let cc = emit_cond(cg, cond)?;
            cg.w(format!("if {} {{ break; }}", cc));
            Ok(())
        }
        StmtKind::Return(e) => {
            let v = match e {
                Some(x) => emit_expr(cg, x)?,
                None => "V::Null".into(),
            };
            // try 闭包内：return 用流向编码穿出闭包（1 = return）
            if !cg.try_stack.is_empty() {
                cg.w(format!("return Ok((Ok({}), 1u8));", v));
            } else {
                cg.w(format!("return Ok({});", v));
            }
            Ok(())
        }
        StmtKind::Throw(e) => {
            let v = emit_expr(cg, e)?;
            if !cg.try_stack.is_empty() {
                cg.w(format!(
                    "return Ok((Err(fe({}, {}, disp(&({})))), 0u8));",
                    s.span.line, s.span.col, v
                ));
            } else {
                cg.w(format!("return Err(fe({}, {}, disp(&({}))));", s.span.line, s.span.col, v));
            }
            Ok(())
        }
        StmtKind::Try { body, catch_name, catch_body, finally } => {
            /* try 用 Rust 闭包实现异常边界；return / break / continue 用流向编码
             * （0 正常 / 1 return / 2 break / 3 continue）穿出闭包，语义和 VM 一致。
             * catch / finally 在闭包外（函数层级）发射，那里的控制流直接生效。 */
            cg.try_stack.push(cg.loop_depth as u32);
            cg.w("let __tr: R<(R<V>, u8)> = (|| -> R<(R<V>, u8)> {".into());
            cg.indent += 1;
            emit_stmts_owned(cg, body, top_level)?;
            cg.indent -= 1;
            cg.w("Ok((Ok(V::Null), 0u8))".into());
            cg.w("})();".into());
            cg.try_stack.pop();
            /* 闭包内部的 `?`（调用/索引/算术失败）走 R 提前返回，这里统一折成
             * "错误 + 流向 0"，让下面的 catch 按 VM 的语义接住它 */
            cg.w("let __tr: (R<V>, u8) = match __tr { Ok(x) => x, Err(e) => (Err(e), 0u8) };".into());
            cg.w("match __tr.1 {".into());
            cg.indent += 1;
            if cg.try_stack.is_empty() {
                // 最外层 try：控制流在这里真正落地
                cg.w("1 => return __tr.0,".into());
                if cg.loop_depth > 0 {
                    cg.w("2 => break,".into());
                    cg.w("3 => continue,".into());
                } else {
                    cg.w("2 | 3 => {}".into());
                }
            } else {
                // 嵌套 try：继续向外层闭包传播（外层闭包现在返回 R<(R<V>, u8)>）
                cg.w("1 => return Ok((__tr.0, 1u8)),".into());
                cg.w("2 => return Ok((__tr.0, 2u8)),".into());
                cg.w("3 => return Ok((__tr.0, 3u8)),".into());
            }
            cg.indent -= 1;
            cg.w("_ => match __tr.0 {".into());
            cg.indent += 1;
            if catch_body.is_some() || catch_name.is_some() {
                match catch_name {
                    Some(n) => {
                        let bound = "V::string(e.message.clone())".to_string();
                        cg.w("Err(e) => {".into());
                        cg.indent += 1;
                        cg.assign_var(n, &bound);
                        if let Some(cb) = catch_body {
                            emit_stmts_owned(cg, cb, top_level)?;
                        }
                        cg.indent -= 1;
                        cg.w("}".into());
                    }
                    None => {
                        cg.w("Err(_) => {".into());
                        cg.indent += 1;
                        if let Some(cb) = catch_body {
                            emit_stmts_owned(cg, cb, top_level)?;
                        }
                        cg.indent -= 1;
                        cg.w("}".into());
                    }
                }
            } else {
                cg.w("Err(_) => {}".into());
            }
            cg.w("Ok(_) => {}".into());
            cg.indent -= 1;
            cg.w("}".into());
            cg.indent -= 1;
            cg.w("}".into());
            if let Some(f) = finally {
                emit_stmts_owned(cg, f, top_level)?;
            }
            Ok(())
        }
        StmtKind::Ask(prompt) => {
            /* `ask` 在 VM 里是：先打提示（Print(1)），再读一行，回显，
             * 结果写进 ans / __last_ans 两个特殊全局。AOT 照这个语义发。 */
            if let Some(e) = prompt {
                let v = emit_expr(cg, e)?;
                cg.w(format!("out_line(&show(&[({}).clone()]));", v));
            }
            cg.w("let __ans_v = ask_line()?;".to_string());
            cg.assign_var("ans", "__ans_v");
            cg.assign_var("__last_ans", "__ans_v");
            Ok(())
        }
        StmtKind::WaitUntil { .. } => Err(AotError::unsupported("wait until", s.span.line)),
        StmtKind::TimeRead | StmtKind::TimeReset => Err(AotError::unsupported("time read / time reset", s.span.line)),
        StmtKind::FuncDef { .. } => Ok(()), // 已单独生成
        StmtKind::Unsupported { what } => Err(AotError::unsupported(what, s.span.line)),
    }
}

/// 复合赋值的运算编码（和 vm::compound_value 的约定一致）
fn bop_code(op: Op) -> u8 {
    match op {
        Op::Add => 0,
        Op::Sub => 1,
        Op::Mul => 2,
        Op::Div => 3,
        Op::Mod => 4,
        _ => 5,
    }
}

/// 编译期 Op → 生成代码里的快路径函数名
fn fast_fn(op: Op) -> &'static str {
    match op {
        Op::Add => "v_add",
        Op::Sub => "v_sub",
        Op::Mul => "v_mul",
        _ => "arith",
    }
}

fn emit_assign(cg: &mut Cg, target: &Target, op: AssignOp, value: &Expr, line: u32) -> Result<(), AotError> {
    /* `s = s + "字面量"` 是最热的拼接写法。通用路径每次都新建 String，
     * 循环里就是 O(n²)；VM 的 compound_value 是就地追加（O(n) 摊还）。
     * 这里认字面量这一种，命中就发就地追加，不命中照走通用路径。 */
    if let (Target::Var(name), AssignOp::Set) = (target, op) {
        if let ExprKind::Bin { op: Op::Add, a, b } = &value.kind {
            if let ExprKind::Var(vn) = &a.kind {
                if vn == name && cg.is_local(name) && !cg.is_int_var(name) {
                    let l = fmt_local(name);
                    let rhs = emit_expr(cg, b)?;
                    /* 右边是变量时要 clone（那个变量后面还要用）；是临时值就直接搬，省一次分配 */
                    let rhs_expr = if matches!(b.kind, ExprKind::Var(_)) {
                        format!("({}).clone()", rhs)
                    } else {
                        rhs
                    };
                    cg.w(format!(
                        "{{ let __t = {}; if !str_append_val(&mut {}, &__t) {{ {} = (arith(Op::Add, &{}, &__t)){}.clone(); }} }}",
                        rhs_expr,
                        l,
                        l,
                        l,
                        em_ln(line, 1)
                    ));
                    return Ok(());
                }
            }
        }
    }
    let val = emit_expr(cg, value)?;
    let compound = op.to_op();
    match target {
        Target::Var(name) => {
            if cg.is_int_var(name) {
                let cur = cg.read_int(name);
                /* 整型函数模式：整棵表达式已经是 i64 代码，直接算，不绕 Value */
                if cg.whole_int {
                    let x = emit_expr(cg, value)?;
                    match compound {
                        Some(o) => {
                            let sym = match o {
                                Op::Add => "+",
                                Op::Sub => "-",
                                Op::Mul => "*",
                                Op::Div => "/",
                                Op::Mod => "%",
                                _ => "",
                            };
                            if sym.is_empty() {
                                let expr = format!(
                                    "want_int_at(&arith(Op::{:?}, &V::Int({}), &({})){}, {}, {})?",
                                    o, cur, x, em_ln(line, 1), line, 1
                                );
                                cg.write_int(name, &expr);
                            } else {
                                cg.write_int(name, &format!("(({}) {} ({}))", cur, sym, x));
                            }
                        }
                        None => cg.write_int(name, &x),
                    }
                    return Ok(());
                }
                match compound {
                    Some(o) => {
                        if let Some(x) = crate::aot::int_expr(cg, value) {
                            let sym = match o {
                                Op::Add => "+",
                                Op::Sub => "-",
                                Op::Mul => "*",
                                // 浮点函数的 /= 是干净的 f64 除法，直接发
                                Op::Div if cg.float_mode => "/",
                                _ => "",
                            };
                            if sym.is_empty() {
                                // 除法/取余/幂：走通用路径再收回整数
                                let v = emit_expr(cg, value)?;
                                let expr = format!(
                                    "want_int_at(&arith(Op::{:?}, &V::Int({}), &({})){}, {}, {})?",
                                    o, cur, v, em_ln(line, 1), line, 1
                                );
                                cg.write_int(name, &expr);
                            } else {
                                let expr = format!("({}) {} ({})", cur, sym, x);
                                cg.write_int(name, &expr);
                            }
                        } else {
                            let v = emit_expr(cg, value)?;
                            let expr = format!(
                                "want_int_at(&arith(Op::{:?}, &V::Int({}), &({})){}, {}, {})?",
                                o, cur, v, em_ln(line, 1), line, 1
                            );
                            cg.write_int(name, &expr);
                        }
                    }
                    None => {
                        if let Some(x) = crate::aot::int_expr(cg, value) {
                            cg.write_int(name, &x);
                        } else {
                            let v = emit_expr(cg, value)?;
                            let expr = format!("want_int_at(&({}), {}, {})?", v, line, 1);
                            cg.write_int(name, &expr);
                        }
                    }
                }
                return Ok(());
            }
            if cg.is_local(name) {
                let l = fmt_local(name);
                match compound {
                    Some(o) => {
                        // 复合赋值走 VM 的同一个实现：字符串能就地追加（O(1) 摊还），整数有快路径
                        cg.w(format!(
                            "{{ let __old = std::mem::replace(&mut {}, V::Null); {} = compound_value({}, __old, &({})){}; }}",
                            l, l, bop_code(o), val, em_ln(line, 1)
                        ))
                    }
                    // VM 是值语义：赋值必须绑定副本，裸变量 RHS 直接 move 会让
                    // 源变量在后续循环/语句里凭空消失（E0382）
                    None => cg.w(format!("{} = ({}).clone();", l, val)),
                }
            } else {
                let i = cg.slot_of(name);
                match compound {
                    Some(o) => {
                        cg.w(format!(
                            "{{ let __old = std::mem::replace(&mut g.v[{}], V::Null); g.v[{}] = compound_value({}, __old, &({})){}; }}",
                            i, i, bop_code(o), val, em_ln(line, 1)
                        ))
                    }
                    None => cg.w(format!("g.v[{}] = ({}).clone();", i, val)),
                }
            }
            Ok(())
        }
        Target::Member(obj, name) => {
            let o = emit_expr(cg, obj)?;
            match compound {
                Some(o2) => cg.w(format!(
                    "{{ let __o = ({}).clone(); let __cur = member_get(&__o, {}){}; let __nv = arith(Op::{:?}, &__cur, &({})){}; member_set(&__o, {}, __nv){}; }}",
                    o, lit_str(name), em_ln(line, 1), o2, val, em_ln(line, 1), lit_str(name), em_ln(line, 1)
                )),
                None => cg.w(format!(
                    "member_set(&({}), {}, ({})){};",
                    o,
                    lit_str(name),
                    val,
                    em_ln(line, 1)
                )),
            }
            Ok(())
        }
        Target::Idx(obj, index) => {
            let o = emit_expr(cg, obj)?;
            let ix = emit_expr(cg, index)?;
            match compound {
                Some(o2) => cg.w(format!(
                    "{{ let __o = ({}).clone(); let __i = ({}).clone(); let __cur = index_get(&__o, &__i){}; let __nv = arith(Op::{:?}, &__cur, &({})){}; index_set(&__o, &__i, __nv){}; }}",
                    o, ix, em_ln(line, 1), o2, val, em_ln(line, 1), em_ln(line, 1)
                )),
                None => cg.w(format!(
                    "index_set(&({}), &({}), ({}).clone()){};",
                    o,
                    ix,
                    val,
                    em_ln(line, 1)
                )),
            }
            Ok(())
        }
        Target::Deref(_) => Err(AotError::unsupported("指针解引用赋值（*p = …）", line)),
    }
}

fn emit_args(cg: &mut Cg, args: &[Expr]) -> Result<Vec<String>, AotError> {
    args.iter().map(|a| emit_expr(cg, a)).collect()
}

/* ───────────────────────── 表达式生成 ───────────────────────── */

/// 纯字面量 → JSON 子集字符串（给 parse_data 用）。有一处不是字面量就返回 None。
fn data_json(e: &Expr) -> Option<String> {
    match &e.kind {
        ExprKind::Num(f) => {
            if !f.is_finite() {
                return None;
            }
            if f.fract() == 0.0 && f.abs() < 9.0e15 {
                Some(format!("{}", *f as i64))
            } else {
                Some(format!("{}", f))
            }
        }
        ExprKind::Str(s) => Some(json_str(s)),
        ExprKind::Bool(b) => Some(if *b { "true".into() } else { "false".into() }),
        ExprKind::Null => Some("null".into()),
        ExprKind::List(xs) => {
            let mut out = String::from("[");
            for (i, x) in xs.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&data_json(x)?);
            }
            out.push(']');
            Some(out)
        }
        ExprKind::Dict(ps) => {
            let mut out = String::from("{");
            for (i, (k, v)) in ps.iter().enumerate() {
                // 键必须是字符串字面量，否则退回普通生成
                let key = match &k.kind {
                    ExprKind::Str(s) => s.to_string(),
                    _ => return None,
                };
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&json_str(&key));
                out.push(':');
                out.push_str(&data_json(v)?);
            }
            out.push('}');
            Some(out)
        }
        _ => None,
    }
}

/// JSON 字符串转义（只处理必要的那几个，够数据表用了）
fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// 能证明是 i64 的表达式 → 直接生成 i64 代码（不碰 Value）。
/// 返回 None = 类型不确定 / 可能溢出成浮点，交回通用路径。
fn int_expr(cg: &mut Cg, e: &Expr) -> Option<String> {
    let fm = cg.float_mode;
    match &e.kind {
        ExprKind::Num(f) => {
            if fm {
                // 浮点模式任意数字字面量都收（{:?} 输出合法 Rust 字面量）
                Some(format!("{:?}f64", f))
            } else if f.fract() == 0.0 && f.abs() < 9.0e15 {
                Some(format!("{}i64", *f as i64))
            } else {
                None
            }
        }
        ExprKind::Var(name) => {
            if cg.is_int_var(name) {
                Some(cg.read_int(name))
            } else {
                None
            }
        }
        ExprKind::Un { op: Op::Neg, a } => {
            let x = int_expr(cg, a)?;
            Some(format!("(-({}))", x))
        }
        ExprKind::Bin { op, a, b } => {
            // 浮点模式：加减乘除全部直接发 f64 运算（f64 无溢出回绕，
            // inf/NaN 与 VM 的浮点世界语义一致）
            if fm {
                return match op {
                    Op::Add | Op::Sub | Op::Mul | Op::Div => {
                        let x = int_expr(cg, a)?;
                        let y = int_expr(cg, b)?;
                        let sym = match op {
                            Op::Add => "+",
                            Op::Sub => "-",
                            Op::Mul => "*",
                            _ => "/",
                        };
                        Some(format!("(({}) {} ({}))", x, sym, y))
                    }
                    _ => None,
                };
            }
            match op {
            // 只挑没有"溢出退化成浮点"歧义的（除法和幂会变浮点，走通用路径）。
            // 加减乘用 checked：VM 溢出会退回浮点世界，特化 i64 世界回不去，
            // 所以溢出时置 err=2 明确报「整数溢出」，绝不静默回绕出错误结果。
            Op::Add | Op::Sub | Op::Mul => {
                let x = int_expr(cg, a)?;
                let y = int_expr(cg, b)?;
                let m = match op {
                    Op::Add => "checked_add",
                    Op::Sub => "checked_sub",
                    _ => "checked_mul",
                };
                Some(format!(
                    "{{ match ({}) .{}({}) {{ Some(__v) => __v, None => {{ g.err = 2; 0i64 }} }} }}",
                    x, m, y
                ))
            }
            // 移位用 wrapping 对齐 VM：移位量超 64 位不 panic（和 VM 位运算一致）
            Op::Mod => {
                // 取模也走 i64 直算，但除零与 i64::MIN % -1 必须按 VM 的语义报错，
                // 不能让 Rust 原地 panic：err=3 除零，err=2 溢出。
                let x = int_expr(cg, a)?;
                let y = int_expr(cg, b)?;
                Some(format!(
                    "{{ let __n = ({}); let __d = ({}); if __d == 0 {{ g.err = 3; 0i64 }} else {{ match __n.checked_rem(__d) {{ Some(__v) => __v, None => {{ g.err = 2; 0i64 }} }} }} }}",
                    x, y
                ))
            }
            Op::BitAnd | Op::BitOr | Op::BitXor | Op::Shl | Op::Shr => {
                let x = int_expr(cg, a)?;
                let y = int_expr(cg, b)?;
                match op {
                    Op::BitAnd => Some(format!("(({}) & ({}))", x, y)),
                    Op::BitOr => Some(format!("(({}) | ({}))", x, y)),
                    Op::BitXor => Some(format!("(({}) ^ ({}))", x, y)),
                    Op::Shl => Some(format!("(({}).wrapping_shl(({}) as u32))", x, y)),
                    _ => Some(format!("(({}).wrapping_shr(({}) as u32))", x, y)),
                }
            }
            _ => None,
            }
        }
        _ => None,
    }
}

/// 条件表达式 → bool 代码。都是整数就直接比，省掉 Value 包装。
fn emit_cond(cg: &mut Cg, e: &Expr) -> Result<String, AotError> {
    /* 整型函数模式：条件只可能是整型比较 / 布尔字面量 / 与或非（推断器保证） */
    if cg.whole_int {
        return match &e.kind {
            ExprKind::Bool(b) => Ok(format!("{}", b)),
            ExprKind::Un { op: Op::Not, a } => Ok(format!("!({})", emit_cond(cg, a)?)),
            ExprKind::Bin { op, a, b } => match op {
                Op::And => Ok(format!("({}) && ({})", emit_cond(cg, a)?, emit_cond(cg, b)?)),
                Op::Or => Ok(format!("({}) || ({})", emit_cond(cg, a)?, emit_cond(cg, b)?)),
                _ => {
                    let bad = |line: u32| {
                        AotError::msg(format!(
                            "内部错误：整型条件的两边不是整型表达式（第 {} 行）",
                            line
                        ))
                    };
                    let x = int_expr(cg, a).ok_or_else(|| bad(e.span.line))?;
                    let y = int_expr(cg, b).ok_or_else(|| bad(e.span.line))?;
                    let sym = match op {
                        Op::Eq => "==",
                        Op::Ne => "!=",
                        Op::Lt => "<",
                        Op::Le => "<=",
                        Op::Gt => ">",
                        Op::Ge => ">=",
                        _ => return Err(bad(e.span.line)),
                    };
                    Ok(format!("(({}) {} ({}))", x, sym, y))
                }
            },
            _ => Err(AotError::msg(format!(
                "内部错误：整型函数条件不支持（第 {} 行）",
                e.span.line
            ))),
        };
    }
    if let ExprKind::Bin { op, a, b } = &e.kind {
        match op {
            Op::And => {
                let x = emit_cond(cg, a)?;
                let y = emit_cond(cg, b)?;
                return Ok(format!("(({}) && ({}))", x, y));
            }
            Op::Or => {
                let x = emit_cond(cg, a)?;
                let y = emit_cond(cg, b)?;
                return Ok(format!("(({}) || ({}))", x, y));
            }
            Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge => {
                if let (Some(x), Some(y)) = (int_expr(cg, a), int_expr(cg, b)) {
                    let sym = match op {
                        Op::Eq => "==",
                        Op::Ne => "!=",
                        Op::Lt => "<",
                        Op::Le => "<=",
                        Op::Gt => ">",
                        _ => ">=",
                    };
                    return Ok(format!("(({}) {} ({}))", x, sym, y));
                }
            }
            _ => {}
        }
    }
    if let ExprKind::Un { op: Op::Not, a } = &e.kind {
        let x = emit_cond(cg, a)?;
        return Ok(format!("(!({}))", x));
    }
    let v = emit_expr(cg, e)?;
    Ok(format!("t(&({}))", v))
}

fn emit_expr(cg: &mut Cg, e: &Expr) -> Result<String, AotError> {
    /* 整型函数模式：整个表达式树直接出 i64 代码（不经 Value 包装）。
     * 调用走 emit_call_i64（跨函数边界也不装箱）。 */
    if cg.whole_int {
        if let Some(x) = int_expr(cg, e) {
            return Ok(x);
        }
        match &e.kind {
            ExprKind::Call { .. } => return emit_call_i64(cg, e),
            // 含调用的算术：操作数递归出 i64 代码再拼运算符（加减乘 checked 防回绕）
            ExprKind::Un { op: Op::Neg, a } => return Ok(format!("(-({}))", emit_expr(cg, a)?)),
            ExprKind::Bin { op, a, b } => {
                let x = emit_expr(cg, a)?;
                let y = emit_expr(cg, b)?;
                return match op {
                    Op::Add => Ok(format!(
                        "{{ match ({}) .checked_add({}) {{ Some(__v) => __v, None => {{ g.err = 2; 0i64 }} }} }}",
                        x, y
                    )),
                    Op::Sub => Ok(format!(
                        "{{ match ({}) .checked_sub({}) {{ Some(__v) => __v, None => {{ g.err = 2; 0i64 }} }} }}",
                        x, y
                    )),
                    Op::Mul => Ok(format!(
                        "{{ match ({}) .checked_mul({}) {{ Some(__v) => __v, None => {{ g.err = 2; 0i64 }} }} }}",
                        x, y
                    )),
                    Op::BitAnd => Ok(format!("(({}) & ({}))", x, y)),
                    Op::BitOr => Ok(format!("(({}) | ({}))", x, y)),
                    Op::BitXor => Ok(format!("(({}) ^ ({}))", x, y)),
                    Op::Shl => Ok(format!("(({}).wrapping_shl(({}) as u32))", x, y)),
                    Op::Shr => Ok(format!("(({}).wrapping_shr(({}) as u32))", x, y)),
                    _ => Err(AotError::msg(format!(
                        "内部错误：整型函数出现了非整型表达式（第 {} 行）",
                        e.span.line
                    ))),
                };
            }
            _ => return Err(AotError::msg(format!(
                "内部错误：整型函数出现了非整型表达式（第 {} 行）",
                e.span.line
            ))),
        }
    }
    // 整数特化：能证明是 i64 的表达式直接算，最后再包一层 V::Int
    if let Some(x) = int_expr(cg, e) {
        return Ok(format!("V::Int({})", x));
    }
    match &e.kind {
        ExprKind::Num(f) => Ok(num_lit(*f)),
        ExprKind::Str(s) => Ok(format!("V::str({})", lit_str(s))),
        ExprKind::Bool(b) => Ok(format!("V::Bool({})", b)),
        ExprKind::Null => Ok("V::Null".into()),
        ExprKind::Var(name) => {
            // 局部/全局变量优先（名字可能和内建重合）
            if cg.is_local(name) {
                return Ok(fmt_local(name));
            }
            if let Some(i) = cg.gslots.get(name.as_ref()) {
                return Ok(format!("g.v[{}].clone()", i));
            }
            // 函数名 / 内建名 → 一等公民函数值
            if cg.meta.contains_key(name) || crate::builtins::is_builtin(name) {
                cg.uses_dcall = true;
                return Ok(format!("V::Native({})", lit_str(name)));
            }
            // 不是局部、不是全局、不是函数、不是内建：就是没定义。
            // 这里直接拒绝，交给 VM 报「变量没有定义」——两边行为一致。
            Err(AotError::msg(format!("变量「{}」没有定义（第 {} 行）", name, e.span.line)))
        }
        ExprKind::Lambda { params, body } => {
            // 内联函数已在 collect_lambdas 里登记成隐藏函数 __lambda_行_列。
            // 这里按位置查表发射 V::Native 引用；查不到说明编译器漏收集，明确报错。
            let key = (e.span.line, e.span.col);
            let name = cg.lambdas.get(&key).cloned().ok_or_else(|| {
                AotError::msg(format!("内部错误：内联函数未登记（第 {} 行）", e.span.line))
            })?;
            // 保守约束：VM 把 lambda 在调用点内联展开（动态作用域），AOT 编译成
            // 独立函数后只能访问全局与自身参数。引用外层局部变量会语义走样，
            // 这种场景明确回落 VM，绝不静默算错。
            let param_names: HashSet<Rc<str>> = params.iter().map(|p| Rc::clone(&p.name)).collect();
            let gslots = &cg.gslots;
            let mut bad: Option<Rc<str>> = None;
            for_each_expr(body, &mut |x| {
                if let ExprKind::Var(n) = &x.kind {
                    if bad.is_none() && !param_names.contains(n) && !gslots.contains_key(n.as_ref())
                    {
                        bad = Some(Rc::clone(n));
                    }
                }
            });
            if let Some(b) = bad {
                return Err(AotError::unsupported(
                    &format!(
                        "内联函数引用了外层局部变量「{}」（AOT 里请用全局变量或参数）",
                        b
                    ),
                    e.span.line,
                ));
            }
            cg.uses_dcall = true;
            Ok(format!("V::Native({})", lit_str(&name)))
        }
        ExprKind::SelfRef => Ok("c_self.clone()".into()),
        ExprKind::List(items) => {
            // 大表格：编成紧凑数据串，运行时一次解析（否则几千条语句会把 rustc 拖死）
            if items.len() >= 200 {
                if let Some(json) = data_json(e) {
                    return Ok(format!("parse_data({}){}", lit_str(&json), em(e)));
                }
            }
            let xs = emit_args(cg, items)?;
            let cloned: Vec<String> = xs.iter().map(|x| format!("({}).clone()", x)).collect();
            /* 小列表用 vec![…] 最省事；大列表（数据表）绝不能这么写——
             * vec![a, b, c, …] 上千个参数会让 rustc 在宏展开里递归到栈溢出
             * （实测 STATUS_STACK_BUFFER_OVERRUN）。改成逐项 push，线性代码。 */
            if cloned.len() <= 64 {
                let mut body = String::new();
                for (i, x) in cloned.iter().enumerate() {
                    if i > 0 {
                        body.push(',');
                        body.push(if i % 8 == 0 { '\n' } else { ' ' });
                    }
                    body.push_str(x);
                }
                return Ok(format!("V::list(vec![{}])", body));
            }
            let mut body = String::from("{ let mut __l: Vec<V> = Vec::with_capacity(");
            body.push_str(&cloned.len().to_string());
            body.push_str("); ");
            for (i, x) in cloned.iter().enumerate() {
                body.push_str(&format!("__l.push({});", x));
                if i % 4 == 3 {
                    body.push('\n');
                }
            }
            body.push_str("V::list(__l) }");
            Ok(body)
        }
        ExprKind::Dict(pairs) => {
            if pairs.len() >= 200 {
                if let Some(json) = data_json(e) {
                    return Ok(format!("parse_data({}){}", lit_str(&json), em(e)));
                }
            }
            let mut parts = String::new();
            for (k, v) in pairs {
                let kk = emit_expr(cg, k)?;
                let vv = emit_expr(cg, v)?;
                parts.push_str(&format!("__d.set(Rc::from(disp(&({})).as_str()), ({}).clone());\n", kk, vv));
            }
            Ok(format!(
                "{{ let mut __d = Dict::new();\n{}V::Dict(Rc::new(std::cell::RefCell::new(__d))) }}",
                parts
            ))
        }
        ExprKind::Un { op, a } => {
            let aa = emit_expr(cg, a)?;
            match op {
                Op::Neg => Ok(format!("{{ let __a = ({}).clone(); neg(&__a){} }}", aa, em(e))),
                Op::Not => Ok(format!("{{ let __a = ({}).clone(); V::Bool(!t(&__a)) }}", aa)),
                Op::Addr | Op::Deref => Err(AotError::unsupported("指针（& / *）", e.span.line)),
                Op::BitNot => Err(AotError::unsupported("按位取反 ~", e.span.line)),
                _ => Ok(format!("{{ let __a = ({}).clone(); arith(Op::{:?}, &V::Int(0), &__a){} }}", aa, op, em(e))),
            }
        }
        ExprKind::Bin { op, a, b } => match op {
            Op::And => {
                let aa = emit_expr(cg, a)?;
                let bb = emit_expr(cg, b)?;
                // sand/sor 返回 R<V>，这里就地解包成块表达式（值 = V），
                // 上层（return Ok(...) / 实参 / 赋值）拿到的才是纯 V
                Ok(format!("{{ let __l = ({}).clone(); if t(&__l) {{ ({}) }} else {{ __l }} }}", aa, bb))
            }
            Op::Or => {
                let aa = emit_expr(cg, a)?;
                let bb = emit_expr(cg, b)?;
                Ok(format!("{{ let __l = ({}).clone(); if t(&__l) {{ __l }} else {{ ({}) }} }}", aa, bb))
            }
            Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge => {
                let aa = emit_expr(cg, a)?;
                let bb = emit_expr(cg, b)?;
                // 实参先绑定局部再借用：&(含调用的表达式) 会把求值期间的 &mut g
                // 一起圈进借用检查，两个实参都是调用时直接 E0499
                Ok(format!(
                    "{{ let __a = ({}).clone(); let __b = ({}).clone(); v_cmp(Op::{:?}, &__a, &__b){} }}",
                    aa, bb, op, em(e)
                ))
            }
            Op::Add | Op::Sub | Op::Mul => {
                let aa = emit_expr(cg, a)?;
                let bb = emit_expr(cg, b)?;
                let f = match op {
                    Op::Add => "v_add",
                    Op::Sub => "v_sub",
                    _ => "v_mul",
                };
                Ok(format!(
                    "{{ let __a = ({}).clone(); let __b = ({}).clone(); {}(&__a, &__b){} }}",
                    aa, bb, f, em(e)
                ))
            }
            Op::BitAnd | Op::BitOr | Op::BitXor | Op::Shl | Op::Shr => {
                let aa = emit_expr(cg, a)?;
                let bb = emit_expr(cg, b)?;
                Ok(format!(
                    "{{ let __a = ({}).clone(); let __b = ({}).clone(); v_bit(Op::{:?}, &__a, &__b){} }}",
                    aa, bb, op, em(e)
                ))
            }
            _ => {
                let aa = emit_expr(cg, a)?;
                let bb = emit_expr(cg, b)?;
                Ok(format!(
                    "{{ let __a = ({}).clone(); let __b = ({}).clone(); arith(Op::{:?}, &__a, &__b){} }}",
                    aa, bb, op, em(e)
                ))
            }
        },
        ExprKind::Ternary { cond, a, b } => {
            let cc = emit_expr(cg, cond)?;
            let aa = emit_expr(cg, a)?;
            let bb = emit_expr(cg, b)?;
            Ok(format!("if t(&({})) {{ {} }} else {{ {} }}", cc, aa, bb))
        }
        ExprKind::Idx { obj, index } => {
            let oo = emit_expr(cg, obj)?;
            let ii = emit_expr(cg, index)?;
            Ok(format!("index_get(&({}), &({})){}", oo, ii, em(e)))
        }
        ExprKind::Slice { obj, from, to } => {
            // 切片直接复用 VM 的 slice_value：字符串按字符切、列表按元素切，
            // 越界/反向的语义跟解释器逐字对齐（以前这里是"不支持"，静默回落 VM）。
            let oo = emit_expr(cg, obj)?;
            let ff = match from {
                Some(x) => emit_expr(cg, x)?,
                None => "V::Null".to_string(),
            };
            let tt = match to {
                Some(x) => emit_expr(cg, x)?,
                None => "V::Null".to_string(),
            };
            Ok(format!("slice_value(&({}), &({}), &({})){}", oo, ff, tt, em(e)))
        }
        ExprKind::Member { obj, name } => {
            let oo = emit_expr(cg, obj)?;
            Ok(format!("member_get(&({}), {}){}", oo, lit_str(name), em(e)))
        }
        ExprKind::New { class, args } => {
            let ctor: Rc<str> = Rc::from(format!("__new_{}_", class).as_str());
            if !cg.meta.contains_key(ctor.as_ref()) {
                return Err(AotError::unsupported(&format!("类 {}", class), e.span.line));
            }
            emit_call(cg, &ctor, args, e)
        }
        ExprKind::Call { callee, args } => match &callee.kind {
            ExprKind::Member { obj, name } => {
                let oo = emit_expr(cg, obj)?;
                let aa = emit_args(cg, args)?;
                cg.uses_mcall = true;
                // 接收者先算到一个局部变量：不然 &g.v[i] 和 &mut g 会撞借用检查
                let cloned: Vec<String> = aa.iter().map(|a| format!("({}).clone()", a)).collect();
                // 实参先绑定局部：实参表达式内部可能又要 &mut g，跟 mcall(g, ...) 撞借用
                let binds: Vec<String> = cloned
                    .iter()
                    .enumerate()
                    .map(|(i, a)| format!("let __a{} = {}; ", i, a))
                    .collect();
                let bindp = binds.concat();
                let names: Vec<String> = (0..cloned.len()).map(|i| format!("__a{}", i)).collect();
                if cg.split {
                    // 分块模式：方法分派在运行时里（块里不知道有哪些类）
                    return Ok(format!(
                        "{{ let __recv = ({}).clone(); {}call_method(g, &__recv, {}, vec![{}], {}, {})? }}",
                        oo,
                        bindp,
                        lit_str(name),
                        names.join(", "),
                        e.span.line,
                        e.span.col
                    ));
                }
                Ok(format!(
                    "{{ let __recv = ({}).clone(); {}mcall(g, &__recv, {}, vec![{}], {}, {})? }}",
                    oo,
                    bindp,
                    lit_str(name),
                    names.join(", "),
                    e.span.line,
                    e.span.col
                ))
            }
            ExprKind::Var(name) => emit_call(cg, name, args, e),
            _ => {
                let ff = emit_expr(cg, callee)?;
                let aa = emit_args(cg, args)?;
                cg.uses_dcall = true;
                // 实参预绑定：实参内部可能再借 &mut g
                let binds: Vec<String> = aa
                    .iter()
                    .enumerate()
                    .map(|(i, a)| format!("let __a{} = ({}).clone(); ", i, a))
                    .collect();
                let bindp = binds.concat();
                let names: Vec<String> = (0..aa.len()).map(|i| format!("__a{}", i)).collect();
                Ok(format!(
                    "{{ {}dcall(g, &({}), vec![{}], {}, {})? }}",
                    bindp,
                    ff,
                    names.join(", "),
                    e.span.line,
                    e.span.col
                ))
            }
        },
    }
}

/// 调用：用户函数（补默认值）→ 内建 → 动态（一等公民函数值）
fn emit_call(cg: &mut Cg, name: &str, args: &[Expr], e: &Expr) -> Result<String, AotError> {
    // 整型函数（单文件模式特化）：i64 直接进、i64 直接出，无装箱无 Result
    let int_spec = cg.meta.get(name).map(|m| m.int_fn);
    if int_spec == Some(true) {
        let code = emit_call_i64_code(
            cg,
            name,
            args,
            e,
            "0",
            &format!(
                "return Err(fe({}, {}, if g.err == 2 {{ String::from(\"整数溢出（超出 i64 范围）\") }} else {{ String::from(\"递归太深\") }}))",
                e.span.line,
                e.span.col
            ),
        )?;
        return Ok(format!("V::Int({})", code));
    }
    // 浮点函数（单文件模式特化）：V 世界实参经 want_f64_at 收敛（Int 自动转
    // f64，与 VM 混合算术语义一致），返回包回 V::Float。f64 无溢出，g.err
    // 只可能是递归超限。
    // 注意：这个臂只在通用（V 世界）上下文触发——浮点函数体内部的调用走
    // emit_call_i64_code（float_mode=true），不会到这里。
    if cg.meta.get(name).map(|m| m.float_fn).unwrap_or(false) {
        // 实参先绑定局部再转 f64：嵌套调用实参与外层两阶段借用冲突（E0499）
        let mut pre = String::new();
        let mut conv = String::new();
        for (i, a) in args.iter().enumerate() {
            let v = emit_expr(cg, a)?;
            pre.push_str(&format!("let __a{} = ({}).clone();\n", i, v));
            conv.push_str(&format!(", want_f64_at(&__a{}, {}, {})?", i, e.span.line, e.span.col));
        }
        return Ok(format!(
            "{{ {}let __r = {}(g, 0{}); if g.err != 0 {{ {} }} V::Float(__r) }}",
            pre,
            rust_fn_name(name),
            conv,
            &format!(
                "return Err(fe({}, {}, String::from(\"递归太深\")))",
                e.span.line,
                e.span.col
            )
        ));
    }
    // 9 个回调式高阶函数：VM 里是编译期内联展开（bytecode::emit_hof），
    // stdlib 运行时没有对应实现。AOT 在 prelude 里补了运行时版（hof_dispatch，
    // 语义对齐 emit_hof），经 call_named 间接调用；回调（lambda 编译成的
    // __lambda_N 或函数值 V::Native）由 dcall 间接调用。
    if matches!(
        name,
        "data.map" | "data.filter" | "data.reduce" | "data.groupby" | "data.index_by"
            | "data.any" | "data.all" | "data.find" | "data.find_index"
    ) {
        let arity = if name == "data.reduce" { 3 } else { 2 };
        if args.len() != arity {
            return Err(AotError::unsupported(
                &format!("{} 需要 {} 个参数", name, arity),
                e.span.line,
            ));
        }
        cg.uses_dcall = true;
        let aa = emit_args(cg, args)?;
        let args_joined = aa
            .iter()
            .map(|a| format!("({}).clone()", a))
            .collect::<Vec<_>>()
            .join(", ");
        // 单文件：走 dcall_by_name 静态表（hof_dispatch 臂）；分块：走运行时
        // 注册表 call_named（REG miss → hof_dispatch，见 RT_REGISTRY）
        let call = if cg.split {
            format!("call_named(g, {}, vec![{}])", lit_str(name), args_joined)
        } else {
            format!(
                "dcall_by_name(g, {}, vec![{}], {}, {})",
                lit_str(name),
                args_joined,
                e.span.line,
                e.span.col
            )
        };
        return Ok(format!("{}{}", call, em(e)));
    }
    let aa = emit_args(cg, args)?;

    let meta_clone = cg.meta.get(name).cloned();
    if let Some(m) = meta_clone {
        // 分块模式：函数在别的块里 → 走运行时注册表（间接调用，块内调用仍是直接调用）
        if cg.split && cg.chunk_of.get(name).copied().unwrap_or(usize::MAX) != cg.my_chunk {
            let mut args: Vec<String> = aa.iter().map(|a| format!("({}).clone()", a)).collect();
            for i in aa.len()..m.arity {
                match m.defaults.get(i).and_then(|d| d.clone()) {
                    Some(d) => {
                        let dv = emit_expr(cg, &d)?;
                        args.push(dv);
                    }
                    None => args.push("V::Null".into()),
                }
            }
            return Ok(format!(
                "call_named(g, {}, vec![{}]){}",
                lit_str(name),
                args.join(", "),
                em(e)
            ));
        }
        let fname = rust_fn_name(name);
        // 实参统一先绑定局部再传：嵌套调用实参（h(g) 作为 f(g, …) 的实参）
        // 会和外层调用的两阶段借用撞车（E0499）。V 是 Rc 构造，clone 廉价。
        let mut pre = String::new();
        for (i, a) in aa.iter().enumerate() {
            pre.push_str(&format!("let __a{} = ({}).clone();\n", i, a));
        }
        let mut call = format!("{}(g", fname);
        for i in 0..aa.len() {
            call.push_str(&format!(", __a{}", i));
        }
        // 缺的参数用默认值表达式在调用点求值（和 VM 的 defaults 同一来源）；
        // 没写默认值的补 Null。默认值表达式同样先绑定（可能含调用）。
        for i in aa.len().min(m.arity)..m.arity {
            match m.defaults.get(i).and_then(|d| d.clone()) {
                Some(d) => {
                    // 默认值目前只在**调用点**求值。要是它引用了被调函数自己的参数
                    // （`fn area(w, h = w)`），调用点根本没有 w —— 以前这里报一句
                    // "变量没有定义"，看着像用户代码写错，其实是 AOT 的能力缺口。
                    let dv = emit_expr(cg, &d).map_err(|e| {
                        if e.message.contains("没有定义") {
                            AotError {
                                message: format!(
                                    "AOT 暂不支持：参数默认值引用了前一个参数（{}），默认值只能在调用点算，调用点没有这个参数",
                                    e.message
                                ),
                                line: e.line.max(d.span.line),
                            }
                        } else {
                            e
                        }
                    })?;
                    pre.push_str(&format!("let __d{} = ({});\n", i, dv));
                    call.push_str(&format!(", __d{}", i));
                }
                None => call.push_str(", V::Null"),
            }
        }
        call.push(')');
        return Ok(format!("{{ {}{}? }}", pre, call));
    }

    if crate::builtins::is_builtin(name) {
        let cloned: Vec<String> = aa.iter().map(|a| format!("({}).clone()", a)).collect();
        return Ok(format!("call_builtin({}, &[{}]){}", lit_str(name), cloned.join(", "), em(e)));
    }

    // 类名省略 new：与 VM / TS 引擎对齐——强制写 new，
    // 不然 `Hero(...)` 到底是实例化还是调函数，读代码的人分不清。
    let ctor = format!("__new_{}_", name);
    if cg.meta.contains_key(ctor.as_str()) {
        return Err(AotError {
            message: format!("「{}」是类名，要创建实例请写 new {}(…)", name, name),
            line: e.span.line,
        });
    }

    // 都不是 → 可能是装着函数值的变量
    cg.uses_dcall = true;
    let i = cg.slot_of(name);
    // dcall 拿 &mut g，实参（可能含调用）同样先绑定局部（E0499）
    let mut pre = String::new();
    for (k, a) in aa.iter().enumerate() {
        pre.push_str(&format!("let __a{} = ({}).clone();\n", k, a));
    }
    let refs = (0..aa.len())
        .map(|k| format!("__a{}", k))
        .collect::<Vec<_>>()
        .join(", ");
    Ok(format!(
        "{{ {}dcall(g, &g.v[{}].clone(), vec![{}], {}, {})? }}",
        pre,
        i,
        refs,
        e.span.line,
        e.span.col
    ))
}

/// 整型函数体内的调用：被调也是整型函数（推断器保证），i64 进 i64 出
fn emit_call_i64(cg: &mut Cg, e: &Expr) -> Result<String, AotError> {
    let ExprKind::Call { callee, args } = &e.kind else {
        return Err(AotError::msg("内部错误：期望调用表达式"));
    };
    let name = match &callee.kind {
        ExprKind::Var(n) => Rc::clone(n),
        _ => return Err(AotError::msg("内部错误：整型调用的目标必须是函数名")),
    };
    // 出错出口的返回值必须和函数签名同类型：浮点特化函数签名是 f64，
    // 这里以前硬写 "return 0"，rustc 直接 E0308（生成代码编不过 → 静默回落 VM）。
    let on_err = if cg.float_mode { "return 0.0" } else { "return 0" };
    emit_call_i64_code(cg, &name, args, e, "__d + 1", on_err)
}

/// 统一的整型调用发射器：生成"i64 进、i64 出"的调用块。
/// depth：被调的递归深度参数（dyn 入口传 "0"，整型函数内传 "__d + 1"）；
/// on_err：g.err 非 0 时执行的语句（dyn 上下文转成 Err，整型函数内直接 return 0）。
/// 实参里嵌套的整型调用递归展开成块表达式，不落回 V 世界。
fn emit_call_i64_code(
    cg: &mut Cg,
    name: &str,
    args: &[Expr],
    e: &Expr,
    depth: &str,
    on_err: &str,
) -> Result<String, AotError> {
    let spec = cg
        .meta
        .get(name)
        .map(|m| (m.int_fn, m.float_fn, m.arity, m.defaults.iter().any(|d| d.is_some())));
    let fm = cg.float_mode;
    // 整型函数只能调整型函数，浮点函数只能调浮点函数（互斥，推断器保证闭合）
    let ok = if fm {
        matches!(spec, Some((_, true, arity, false)) if arity == args.len())
    } else {
        matches!(spec, Some((true, _, arity, false)) if arity == args.len())
    };
    if !ok {
        return Err(AotError::unsupported(
            &format!(
                "{}函数调用非{}函数 {}",
                if fm { "浮点" } else { "整型" },
                if fm { "浮点" } else { "整型" },
                name
            ),
            e.span.line,
        ));
    }
    let mut aa = Vec::with_capacity(args.len());
    for a in args {
        if let Some(x) = int_expr(cg, a) {
            aa.push(x);
        } else if let ExprKind::Call { callee, args: inner } = &a.kind {
            let ExprKind::Var(cn) = &callee.kind else {
                return Err(AotError::msg(format!(
                    "内部错误：整型实参的调用目标必须是函数名（第 {} 行）",
                    a.span.line
                )));
            };
            aa.push(emit_call_i64_code(cg, cn, inner, a, depth, on_err)?);
        } else {
            return Err(AotError::msg(format!(
                "内部错误：整型实参不是整型形状（第 {} 行）",
                a.span.line
            )));
        }
    }
    // 块表达式实参（嵌套调用 / 溢出检查块）先绑定到局部变量再传参：
    // 直接内联的话，块里的 g.err 赋值会和第一个参数 &mut g 的借用撞车（E0506）
    let mut pre = String::new();
    let mut aa2 = Vec::with_capacity(aa.len());
    for (i, a) in aa.iter().enumerate() {
        if a.starts_with('{') {
            pre.push_str(&format!("let __arg{} = ({});\n", i, a));
            aa2.push(format!("__arg{}", i));
        } else {
            aa2.push(a.clone());
        }
    }
    let call = format!(
        "{}(g, {}{})",
        rust_fn_name(name),
        depth,
        aa2.iter().map(|a| format!(", {}", a)).collect::<String>()
    );
    Ok(format!(
        "{{ {}let __r = {}; if g.err != 0 {{ {} }} __r }}",
        pre, call, on_err
    ))
}

/// for x in a..b / for x in a..=b step s
fn emit_for_range(
    cg: &mut Cg,
    var: &Rc<str>,
    from: Option<&Expr>,
    to: &Expr,
    step: Option<&Expr>,
    inclusive: bool,
    body: &[Stmt],
    top_level: bool,
) -> Result<(), AotError> {
    // 整型函数模式：纯 i64 循环，每圈只动一个寄存器
    if cg.whole_int {
        return emit_for_range_int(cg, var, from, to, step, inclusive, body, top_level);
    }
    let to_c = emit_expr(cg, to)?;
    let from_c = match from {
        Some(f) => emit_expr(cg, f)?,
        None => "V::Int(0)".to_string(),
    };
    let step_c = match step {
        Some(st) => emit_expr(cg, st)?,
        None => "V::Int(1)".to_string(),
    };
    let incl = if inclusive { 1i64 } else { 0i64 };

    cg.w("{".into());
    cg.indent += 1;
    cg.w(format!("let __rf = ({}); let __rt = ({}); let __rs = ({});", from_c, to_c, step_c));
    cg.w("match (__rf.as_int(), __rt.as_int(), __rs.as_int()) {".into());
    cg.indent += 1;
    // 整数快路径
    cg.w("(Some(f), Some(tv), Some(st)) if st != 0 => {".into());
    cg.indent += 1;
    cg.w("let mut __i = f;".into());
    cg.w(format!("while (st > 0 && __i < tv + {}) || (st < 0 && __i > tv - {}) {{", incl, incl));
    cg.indent += 1;
    cg.loop_depth += 1;
    if cg.is_int_var(var) {
        cg.write_int(var, "(__i)");
    } else {
        cg.assign_var(var, "V::Int(__i)");
    }
    emit_stmts_owned(cg, body, top_level)?;
    cg.w("__i += st;".into());
    cg.loop_depth -= 1;
    cg.indent -= 1;
    cg.w("}".into());
    cg.indent -= 1;
    cg.w("}".into());
    // 通用路径（含浮点）：和 VM 的 RangeNext 同规则（inclusive 加 1e-9 余量）
    cg.w("_ => {".into());
    cg.indent += 1;
    cg.w("let f = __rf.as_num().ok_or_else(|| fe(0, 0, \"range 的边界要是数字\"))?;".into());
    cg.w("let tv = __rt.as_num().ok_or_else(|| fe(0, 0, \"range 的边界要是数字\"))?;".into());
    cg.w("let st = __rs.as_num().ok_or_else(|| fe(0, 0, \"range 的步长要是数字\"))?;".into());
    cg.w(format!("let eps = {} as f64 * 1e-9;", incl));
    cg.w("let mut __i = f;".into());
    cg.w("while (st > 0.0 && __i < tv + eps) || (st < 0.0 && __i > tv - eps) {".into());
    cg.indent += 1;
    cg.loop_depth += 1;
    if cg.is_int_var(var) {
        cg.write_int(var, "want_int_at(&num_to_v(__i), 0, 0)?");
    } else {
        cg.assign_var(var, "num_to_v(__i)");
    }
    emit_stmts_owned(cg, body, top_level)?;
    cg.w("__i += st;".into());
    cg.loop_depth -= 1;
    cg.indent -= 1;
    cg.w("}".into());
    cg.indent -= 1;
    cg.w("}".into());
    cg.indent -= 1;
    cg.w("}".into());
    cg.indent -= 1;
    cg.w("}".into());
    Ok(())
}

/// 整型函数里的 for i in a..b：纯 i64 循环，不碰 Value
#[allow(clippy::too_many_arguments)]
fn emit_for_range_int(
    cg: &mut Cg,
    var: &Rc<str>,
    from: Option<&Expr>,
    to: &Expr,
    step: Option<&Expr>,
    inclusive: bool,
    body: &[Stmt],
    top_level: bool,
) -> Result<(), AotError> {
    // 浮点函数模式：f64 计数器，eps 余量对齐 VM 浮点 range 语义（inclusive + 1e-9）
    if cg.float_mode {
        let f = match from {
            Some(x) => emit_expr(cg, x)?,
            None => "0.0".to_string(),
        };
        let t = emit_expr(cg, to)?;
        let st = match step {
            Some(x) => emit_expr(cg, x)?,
            None => "1.0".to_string(),
        };
        let eps = if inclusive { "1e-9" } else { "0.0" };
        cg.w("{".into());
        cg.indent += 1;
        cg.w(format!("let mut __i: f64 = ({});", f));
        cg.w(format!(
            "while (({}) > 0.0 && __i < ({}) + {}) || (({}) < 0.0 && __i > ({}) - {}) {{",
            st, t, eps, st, t, eps
        ));
        cg.indent += 1;
        cg.loop_depth += 1;
        cg.write_int(var, "(__i)");
        emit_stmts_owned(cg, body, top_level)?;
        cg.w(format!("__i += ({});", st));
        cg.loop_depth -= 1;
        cg.indent -= 1;
        cg.w("}".into());
        cg.indent -= 1;
        cg.w("}".into());
        return Ok(());
    }
    let bad = |x: &Expr| {
        AotError::msg(format!(
            "内部错误：整型 for 的边界不是整型表达式（第 {} 行）",
            x.span.line
        ))
    };
    let f = match from {
        Some(x) => emit_expr(cg, x)?,
        None => "0i64".to_string(),
    };
    let t = emit_expr(cg, to)?;
    let st = match step {
        Some(x) => emit_expr(cg, x)?,
        None => "1i64".to_string(),
    };
    let _ = bad; // 整型模式下 emit_expr 自带"非整型即报错"的兜底
    let incl = if inclusive { 1i64 } else { 0i64 };
    cg.w("{".into());
    cg.indent += 1;
    cg.w(format!("let mut __i = ({});", f));
    /* 步长为 0 时和通用路径一致：死循环（VM 里 RangeNext 也是死循环，
     * 靠 MAX_OUTPUT/内存护栏兜底） */
    cg.w(format!(
        "while ({} > 0 && __i < ({}) + {}) || ({} < 0 && __i > ({}) - {}) || ({}) == 0 {{",
        st, t, incl, st, t, incl, st
    ));
    cg.indent += 1;
    cg.loop_depth += 1;
    cg.write_int(var, "(__i)");
    emit_stmts_owned(cg, body, top_level)?;
    // 计数器也 checked：i64::MAX 附近的 range 不静默回绕（溢出置 err=2，
    // 下一轮循环条件自然不成立、break 由出口统一报错）
    cg.w(format!(
        "__i = match __i.checked_add(({})) {{ Some(__v) => __v, None => {{ g.err = 2; break }} }};",
        st
    ));
    cg.loop_depth -= 1;
    cg.indent -= 1;
    cg.w("}".into());
    cg.indent -= 1;
    cg.w("}".into());
    Ok(())
}

/* ───────────────────────── 生成器辅助：分派器 ───────────────────────── */


/* ═════════════════════════ 分块（增量）编译 ═════════════════════════
 *
 * 为什么需要：整份程序塞进一个 .rs 时，代码上到一两万行，rustc 单个编译单元
 * 就会吃掉几个 G 内存（实测把整台机器顶崩）。分块之后每个 rustc 只处理一小块，
 * 而且 cargo 会缓存没改动的块 —— 改一个函数只重编一块，这就是"增量"。
 *
 * 结构：
 *   rt/          运行时（值模型 + 算术 + 方法 + 函数注册表），永远不变，编一次就缓存住
 *   chunkK/      每块最多 ~60 个 Copper 函数，跨块调用走 rt 的注册表
 *   app/         顶层语句 + main + 类方法分派，负责把各块"装配"起来
 */

pub struct SplitFile {
    pub rel: String,
    pub content: String,
}

const ALLOW: &str = "#![allow(dead_code, unused_imports, unused_variables, unused_mut, unused_parens, non_snake_case, unreachable_patterns, unreachable_code, unused_assignments)]\n";

/// 注册表 + 跨块调用：分块模式下所有跨块调用都落到这里
const RT_REGISTRY: &str = r#"
pub type FnPtr = fn(&mut Globals, Vec<V>) -> R<V>;

thread_local! {
    // HashMap：按名字 O(1) 查（原来是 Vec 线性扫，函数越多调用越慢）
    static REG: std::cell::RefCell<std::collections::HashMap<&'static str, FnPtr>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// 每个块启动时把自己的函数表登记进来
pub fn register(t: &'static [(&'static str, usize, FnPtr)]) {
    REG.with(|r| r.borrow_mut().extend(t.iter().map(|(n, _, f)| (*n, *f))));
}

/// 按名字调用：跨块调用、一等公民函数值都走这里；查不到就先试 data.* 高阶
/// 函数（prelude 运行时版），再试内建函数
pub fn call_named(g: &mut Globals, name: &str, args: Vec<V>) -> Result<V, String> {
    let f = REG.with(|r| r.borrow().get(name).copied());
    match f {
        Some(f) => f(g, args).map_err(|e| e.message),
        None => match hof_dispatch(g, name, args.clone(), 0, 0) {
            Some(r) => r,
            None => call_builtin(name, &args),
        },
    }
}

/// 注册表里有没有这个名字（类方法分派用）
pub fn has_fn(name: &str) -> bool {
    REG.with(|r| r.borrow().contains_key(name))
}

/// 方法调用：沿 __base 链找 __m_类_方法，找到就带着 self 调；找不到退回字典内建方法
pub fn call_method(g: &mut Globals, o: &V, name: &str, args: Vec<V>, ln: u32, cl: u32) -> R<V> {
    if let V::Dict(d) = o {
        if let Some(cv) = d.borrow().get("__cls") {
            let mut cur = cv.to_display();
            let mut guard = 0;
            loop {
                guard += 1;
                if guard > 16 {
                    break;
                }
                let mname = format!("__m_{}_{}", cur, name);
                if has_fn(&mname) {
                    let mut a: Vec<V> = Vec::with_capacity(args.len() + 1);
                    a.push(o.clone());
                    a.extend(args);
                    return call_named(g, &mname, a).map_err(|m| fe(ln, cl, m));
                }
                match d.borrow().get("__base") {
                    Some(b) => cur = b.to_display(),
                    None => break,
                }
            }
            return dict_method(d, name, &args).map_err(|m| fe(ln, cl, m));
        }
    }
    method_plain(o, name, &args).map_err(|m| fe(ln, cl, m))
}

/// FNV-1a：分块要稳定（不能用 RandomState，否则每次编译块的归属都不一样，缓存全废）
pub fn fn_hash(s: &str) -> usize {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h as usize
}

/// 回调调用器（分块版）：函数值 → 运行时注册表
fn hof_call(g: &mut Globals, f: &V, a: Vec<V>, ln: u32, cl: u32) -> R<V> {
    match f {
        V::Native(nm) => call_named(g, nm, a).map_err(|m| fe(ln, cl, m)),
        V::Func(u) => call_named(g, u.name.as_ref(), a).map_err(|m| fe(ln, cl, m)),
        other => Err(fe(ln, cl, format!("{} 不是一个函数", other.to_display()))),
    }
}

/// data.* 高阶函数接住层：call_named 查不到用户函数时先试这里，
/// 不是高阶函数再落到 call_builtin（保持原有报错行为）
fn hof_dispatch(g: &mut Globals, name: &str, args: Vec<V>, ln: u32, cl: u32) -> Option<Result<V, String>> {
    let arity = if name == "data.reduce" { 3 } else { 2 };
    if !name.starts_with("data.") || args.len() != arity {
        return None;
    }
    if hof_iter(&args[0]).is_none() {
        return None;
    }
    Some(hof_run(g, name, args, ln, cl, hof_call).map_err(|e| e.message))
}
"#;

/// 估一条语句有多大（AST 节点数）。用来判断"这条语句要不要单独生成一个 crate"，
/// 因为 rustc 在单个函数特别大时会超线性变慢。
fn count_nodes(s: &Stmt) -> usize {
    fn expr_nodes(e: &Expr) -> usize {
        match &e.kind {
            ExprKind::List(xs) => 1 + xs.iter().map(expr_nodes).sum::<usize>(),
            ExprKind::Dict(ps) => 1 + ps.iter().map(|(k, v)| expr_nodes(k) + expr_nodes(v)).sum::<usize>(),
            ExprKind::Un { a, .. } => 1 + expr_nodes(a),
            ExprKind::Bin { a, b, .. } => 1 + expr_nodes(a) + expr_nodes(b),
            ExprKind::Ternary { cond, a, b } => 1 + expr_nodes(cond) + expr_nodes(a) + expr_nodes(b),
            ExprKind::Idx { obj, index } => 1 + expr_nodes(obj) + expr_nodes(index),
            ExprKind::Slice { obj, from, to } => {
                1 + expr_nodes(obj)
                    + from.as_ref().map(|x| expr_nodes(x)).unwrap_or(0)
                    + to.as_ref().map(|x| expr_nodes(x)).unwrap_or(0)
            }
            ExprKind::Member { obj, .. } => 1 + expr_nodes(obj),
            ExprKind::Call { callee, args } => 1 + expr_nodes(callee) + args.iter().map(expr_nodes).sum::<usize>(),
            ExprKind::New { args, .. } => 1 + args.iter().map(expr_nodes).sum::<usize>(),
            _ => 1,
        }
    }
    fn stmt_nodes(s: &Stmt) -> usize {
        match &s.kind {
            StmtKind::Decl { init, .. } => 1 + init.as_ref().map(expr_nodes).unwrap_or(0),
            StmtKind::Assign { value, target, .. } => {
                let t = match target {
                    Target::Idx(o, i) => expr_nodes(o) + expr_nodes(i),
                    Target::Member(o, _) => expr_nodes(o),
                    _ => 0,
                };
                1 + t + expr_nodes(value)
            }
            StmtKind::ExprStmt(e) => 1 + expr_nodes(e),
            StmtKind::Print(a) | StmtKind::Report(a) => 1 + a.iter().map(expr_nodes).sum::<usize>(),
            StmtKind::If { branches, else_body } => {
                1 + branches
                    .iter()
                    .map(|(c, b)| c.as_ref().map(expr_nodes).unwrap_or(0) + b.iter().map(stmt_nodes).sum::<usize>())
                    .sum::<usize>()
                    + else_body.as_ref().map(|b| b.iter().map(stmt_nodes).sum::<usize>()).unwrap_or(0)
            }
            StmtKind::ForRange { body, .. } | StmtKind::ForIn { body, .. } | StmtKind::While { body, .. } | StmtKind::Repeat { body, .. } => {
                1 + body.iter().map(stmt_nodes).sum::<usize>()
            }
            StmtKind::Group(b) => b.iter().map(stmt_nodes).sum::<usize>(),
            _ => 1,
        }
    }
    stmt_nodes(s)
}

/// 收集所有全局变量名（值槽），顺序按源码出现顺序，稳定可复现
fn collect_global_names(prog: &Program) -> Result<Vec<Rc<str>>, AotError> {
    let mut out: Vec<Rc<str>> = Vec::new();
    let mut seen: HashSet<Rc<str>> = HashSet::new();

    fn add(out: &mut Vec<Rc<str>>, seen: &mut HashSet<Rc<str>>, n: &Rc<str>) {
        if seen.insert(Rc::clone(n)) {
            out.push(Rc::clone(n));
        }
    }
    fn walk(body: &[Stmt], out: &mut Vec<Rc<str>>, seen: &mut HashSet<Rc<str>>) {
        for s in body {
            match &s.kind {
                StmtKind::Decl { name, .. } => add(out, seen, name),
                StmtKind::ForRange { var, body, .. } => {
                    add(out, seen, var);
                    walk(body, out, seen);
                }
                StmtKind::ForIn { var, value_name, body, .. } => {
                    add(out, seen, var);
                    if let Some(v) = value_name {
                        add(out, seen, v);
                    }
                    walk(body, out, seen);
                }
                StmtKind::Assign { target: Target::Var(n), .. }
                | StmtKind::IncDec { target: Target::Var(n), .. } => add(out, seen, n),
                StmtKind::If { branches, else_body } => {
                    for (_, b) in branches {
                        walk(b, out, seen);
                    }
                    if let Some(b) = else_body {
                        walk(b, out, seen);
                    }
                }
                StmtKind::While { body, .. } | StmtKind::Repeat { body, .. } | StmtKind::Group(body) => walk(body, out, seen),
                StmtKind::Try { body, catch_name, catch_body, finally } => {
                    if let Some(n) = catch_name {
                        add(out, seen, n);
                    }
                    walk(body, out, seen);
                    walk(catch_body.as_deref().unwrap_or(&[]), out, seen);
                    walk(finally.as_deref().unwrap_or(&[]), out, seen);
                }
                // 解构赋值 a, b = f()：名字也会占全局槽
                StmtKind::Destructure { names, .. } => {
                    for n in names {
                        add(out, seen, n);
                    }
                }
                _ => {}
            }
        }
    }

    // 顶层（不跨进函数体）
    walk(&prog.body, &mut out, &mut seen);

    // 函数体里"给没声明过的名字赋值"也会建全局
    for s in &prog.body {
        if let StmtKind::FuncDef { body, .. } = &s.kind {
            let mut locals = HashSet::new();
            collect_locals(body, &mut locals)?;
            let mut assigned = HashSet::new();
            collect_assigned(body, &mut assigned);
            let mut names: Vec<Rc<str>> = assigned.into_iter().collect();
            names.sort();
            for n in names {
                if !locals.contains(&n) && seen.insert(Rc::clone(&n)) {
                    out.push(n);
                }
            }
        }
    }
    Ok(out)
}

/// 生成一个 Copper 函数对应的 Rust 函数（分块模式：pub，跨块走注册表）
fn render_fn_split(
    meta: &HashMap<Rc<str>, FnMeta>,
    globals: &[Rc<str>],
    int_globals: &HashSet<Rc<str>>,
    gi_list0: &[Rc<str>],
    gi_slots0: &HashMap<Rc<str>, usize>,
    classes: &HashSet<Rc<str>>,
    chunk_of: &HashMap<Rc<str>, usize>,
    my_chunk: usize,
    name: &Rc<str>,
    params: &[crate::ast::Param],
    body: &[Stmt],
    lambdas: &HashMap<(u32, u32), Rc<str>>,
) -> Result<String, AotError> {
    let int_locals = classify_int_vars(body);
    let mut fcg = Cg {
        meta: meta.clone(),
        globals: globals.to_vec(),
        gslots: HashMap::new(),
        int_globals: int_globals.clone(),
        gi_list: gi_list0.to_vec(),
        gi_slots: gi_slots0.clone(),
        int_locals: Some(int_locals.clone()),
        classes: classes.clone(),
        out: String::new(),
        indent: 1,
        loop_depth: 0,
        locals: Some(HashSet::new()),
        try_stack: Vec::new(),
            lambdas: lambdas.clone(),
        uses_mcall: false,
        uses_dcall: false,
        whole_int: false,
        float_mode: false,
        split: true,
        my_chunk,
        chunk_of: chunk_of.clone(),
    };
    for g in globals {
        fcg.slot_of(g);
    }
    let mut locals = HashSet::new();
    collect_locals(body, &mut locals)?;
    for p in params {
        locals.insert(Rc::clone(&p.name));
    }
    fcg.locals = Some(locals.clone());

    let mut sig = String::new();
    sig.push_str(&format!("#[allow(unused_mut)]\npub fn {}(g: &mut Globals", rust_fn_name(name)));
    for p in params {
        sig.push_str(&format!(", mut c_{}: V", sanitize(&p.name)));
    }
    sig.push_str(") -> R<V> {\n");
    sig.push_str(&format!("    let _guard = enter({})?;\n", lit_str(name)));
    fcg.out.push_str(&sig);

    let mut decls: Vec<String> = Vec::new();
    for l in &locals {
        if params.iter().any(|p| &p.name == l) {
            continue;
        }
        if int_locals.contains(l) {
            decls.push(format!("    let mut {}: i64 = 0;", fmt_local(l)));
        } else {
            decls.push(format!("    let mut {}: V = V::Null;", fmt_local(l)));
        }
    }
    decls.sort();
    for d in decls {
        fcg.out.push_str(&d);
        fcg.out.push('\n');
    }

    emit_stmt_slice(&mut fcg, body, false)?;
    fcg.out.push_str("    Ok(V::Null)\n}\n");
    Ok(std::mem::take(&mut fcg.out))
}

/// 整型/浮点特化函数：参数/返回全是 i64 或 f64，无 Value 装箱、无 TLS 深度守卫。
/// 深度通过 __d 参数传递；出错（整型只可能是递归超限或溢出，浮点只可能递归
/// 超限）写 g.err 并返回 0/0.0，调用方（特化或通用路径）看到 g.err 都会向上传播。
/// 本函数仅供单文件模式使用（infer_int_fns / infer_float_fns 标记过的函数才会走到这里）。
#[allow(clippy::too_many_arguments)]
fn render_fn_int(
    meta: &HashMap<Rc<str>, FnMeta>,
    globals: &[Rc<str>],
    int_globals: &HashSet<Rc<str>>,
    gi_list0: &[Rc<str>],
    gi_slots0: &HashMap<Rc<str>, usize>,
    classes: &HashSet<Rc<str>>,
    name: &Rc<str>,
    params: &[crate::ast::Param],
    body: &[Stmt],
    lambdas: &HashMap<(u32, u32), Rc<str>>,
    float_mode: bool,
) -> Result<String, AotError> {
    let mut locals = HashSet::new();
    collect_locals(body, &mut locals)?;
    for p in params {
        locals.insert(Rc::clone(&p.name));
    }
    // 整型函数里所有名字都是 i64、浮点函数里都是 f64（推断器保证没有非数值声明）
    let mut fcg = Cg {
        meta: meta.clone(),
        globals: globals.to_vec(),
        gslots: HashMap::new(),
        int_globals: int_globals.clone(),
        gi_list: gi_list0.to_vec(),
        gi_slots: gi_slots0.clone(),
        int_locals: Some(locals.clone()),
        classes: classes.clone(),
        out: String::new(),
        indent: 1,
        loop_depth: 0,
        locals: Some(locals.clone()),
        uses_mcall: false,
        uses_dcall: false,
        whole_int: true,
        float_mode,
        split: false,
        my_chunk: 0,
        chunk_of: HashMap::new(),
            try_stack: Vec::new(),
            lambdas: lambdas.clone(),
    };
    for g in globals {
        fcg.slot_of(g);
    }

    let num_ty = if float_mode { "f64" } else { "i64" };
    let zero = if float_mode { "0.0" } else { "0" };
    let mut sig = String::new();
    sig.push_str(&format!(
        "#[allow(unused_mut)]\nfn {}(g: &mut Globals, __d: u32",
        rust_fn_name(name)
    ));
    for p in params {
        sig.push_str(&format!(", mut c_{}: {}", sanitize(&p.name), num_ty));
    }
    sig.push_str(&format!(") -> {} {{\n", num_ty));
    sig.push_str(&format!(
        "    if (__d as usize) >= float_lang::limits::INT_CALL_DEPTH {{ g.err = 1; return {} }}\n",
        zero
    ));
    fcg.out.push_str(&sig);

    let mut decls: Vec<String> = Vec::new();
    for l in &locals {
        if params.iter().any(|p| &p.name == l) {
            continue; // 参数已经在签名里
        }
        decls.push(format!("    let mut {}: {} = {};", fmt_local(l), num_ty, zero));
    }
    decls.sort();
    for d in decls {
        fcg.out.push_str(&d);
        fcg.out.push('\n');
    }

    emit_stmt_slice(&mut fcg, body, false)?;
    fcg.out.push_str(&format!("    {}\n}}\n", zero));
    Ok(std::mem::take(&mut fcg.out))
}

/// 分块（增量）编译：返回整个 cargo workspace 的所有文件。
/// 调用方负责把文件写进构建目录（内容没变的不要重写，否则 cargo 缓存会失效）。
pub fn emit_split(prog: &Program, bin: &str, dep_path: &str) -> Result<Vec<SplitFile>, AotError> {
    if let Some(e) = prog.errors.first() {
        return Err(AotError::msg(format!("语法有错，先修语法：{}", e.message)));
    }

    let mut fns: Vec<(Rc<str>, Vec<crate::ast::Param>, Vec<Stmt>)> = Vec::new();
    scan_body(&prog.body, &mut fns)?;

    // 内联函数（fn(x) return …）：编译成隐藏函数并登记（分块模式同样支持）
    let top_stmts0: Vec<Stmt> = prog
        .body
        .iter()
        .filter(|s| !matches!(s.kind, StmtKind::FuncDef { .. }))
        .cloned()
        .collect();
    let lambda_map = collect_lambdas(&top_stmts0, &mut fns);

    let mut meta: HashMap<Rc<str>, FnMeta> = HashMap::new();
    let mut methods: Vec<(String, String, usize)> = Vec::new();
    let mut classes: HashSet<Rc<str>> = HashSet::new();
    for (name, params, _) in &fns {
        meta.insert(
            Rc::clone(name),
            FnMeta {
                arity: params.len(),
                defaults: params.iter().map(|p| p.default.clone()).collect(),
                int_fn: false,
        float_fn: false,
                has_lazy: params.iter().any(|p| p.lazy),
            },
        );
        if let Some(n) = name.strip_prefix("__m_") {
            if let Some(pos) = n.rfind('_') {
                let cls = n[..pos].to_string();
                let meth = n[pos + 1..].to_string();
                methods.push((cls.clone(), meth.clone(), params.len().saturating_sub(1)));
                classes.insert(Rc::from(cls.as_str()));
            }
        }
    }

    let globals = collect_global_names(prog)?;
    let top_stmts: Vec<Stmt> = prog
        .body
        .iter()
        .filter(|s| !matches!(s.kind, StmtKind::FuncDef { .. }))
        .cloned()
        .collect();
    let int_globals = classify_int_vars(&top_stmts);
    let mut gi_sorted: Vec<Rc<str>> = int_globals.iter().cloned().collect();
    gi_sorted.sort();
    let gi_list0 = gi_sorted.clone();
    let mut gi_slots0: HashMap<Rc<str>, usize> = HashMap::new();
    for (i, n) in gi_list0.iter().enumerate() {
        gi_slots0.insert(Rc::clone(n), i);
    }
    let mut gslots0: HashMap<Rc<str>, usize> = HashMap::new();
    for (i, n) in globals.iter().enumerate() {
        gslots0.insert(Rc::clone(n), i);
    }

    // 分块：按函数名哈希（稳定），每块 ≤ 60 个函数
    let nchunks = (((fns.len() + 199) / 200).max(1)).min(12);
    let hash_of = |n: &str| -> usize {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in n.as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100_0000_01b3);
        }
        (h as usize) % nchunks
    };
    let mut chunk_of: HashMap<Rc<str>, usize> = HashMap::new();
    for (n, _, _) in &fns {
        chunk_of.insert(Rc::clone(n), hash_of(n));
    }

    let mut files: Vec<SplitFile> = Vec::new();

    // ── 根 workspace ──
    let mut members: Vec<String> = vec!["rt".into()];
    for k in 0..nchunks {
        members.push(format!("chunk{}", k));
    }

    // ── 运行时 crate（永远不变，编一次就缓存）──
    files.push(SplitFile {
        rel: "rt/Cargo.toml".into(),
        content: format!(
            "[package]\nname = \"copper_rt\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nfloat-lang = {{ path = {:?} }}\n",
            dep_path
        ),
    });
    files.push(SplitFile {
        rel: "rt/src/lib.rs".into(),
        content: format!("{}\n{}\n", ALLOW, format!("{}{}", PRELUDE.replace("#![allow", "// #![allow"), RT_REGISTRY)),
    });

    // ── 函数块 ──
    for k in 0..nchunks {
        let mut list: Vec<&(Rc<str>, Vec<crate::ast::Param>, Vec<Stmt>)> =
            fns.iter().filter(|(n, _, _)| chunk_of[n] == k).collect();
        list.sort_by(|a, b| a.0.cmp(&b.0));

        let mut body = String::new();
        body.push_str(ALLOW);
        body.push_str("use copper_rt::*;\n\n");
        for (name, params, fbody) in &list {
            body.push_str(&render_fn_split(
                &meta,
                &globals,
                &int_globals,
                &gi_list0,
                &gi_slots0,
                &classes,
                &chunk_of,
                k,
                name,
                params,
                fbody,
                &lambda_map,
            )?);
            body.push('\n');
        }

        // 函数表（跨块调用用）
        body.push_str("pub static TABLE: &[(&str, usize, FnPtr)] = &[\n");
        for (i, (name, params, _)) in list.iter().enumerate() {
            body.push_str(&format!("    ({}, {}, __w{}),\n", lit_str(name), params.len(), i));
        }
        body.push_str("];\n\n");

        // 统一签名的包装（注册表里的函数都是 fn(&mut Globals, Vec<V>) -> R<V>）
        for (i, (name, params, _)) in list.iter().enumerate() {
            body.push_str(&format!("fn __w{}(g: &mut Globals, a: Vec<V>) -> R<V> {{\n", i));
            let mut call = format!("{}(g", rust_fn_name(name));
            for j in 0..params.len() {
                call.push_str(&format!(", a.get({}).cloned().unwrap_or(V::Null)", j));
            }
            call.push(')');
            body.push_str(&format!("    {}\n}}\n", call));
        }

        files.push(SplitFile {
            rel: format!("chunk{}/Cargo.toml", k),
            content: format!(
                "[package]\nname = \"copper_chunk{}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\ncopper_rt = {{ path = \"../rt\" }}\n",
                k
            ),
        });
        files.push(SplitFile { rel: format!("chunk{}/src/lib.rs", k), content: body });
    }

    // ── app：顶层语句 + main + 方法分派 ──
    let mut acg = Cg {
        meta: meta.clone(),
        globals: globals.clone(),
        gslots: gslots0.clone(),
        int_globals: int_globals.clone(),
        gi_list: gi_list0.clone(),
        gi_slots: gi_slots0.clone(),
        int_locals: None,
        classes: classes.clone(),
        out: String::new(),
        indent: 1,
        loop_depth: 0,
        locals: None,
        try_stack: Vec::new(),
            lambdas: lambda_map.clone(),
        uses_mcall: false,
        uses_dcall: false,
        whole_int: false,
        float_mode: false,
        split: true,
        my_chunk: usize::MAX,
        chunk_of: chunk_of.clone(),
    };
    /* 顶层语句的安置策略：
     * 1) 巨型语句（数据表那种几百上千个元素的字面量）单独生成一个 data crate ——
     *    rustc 在单个函数特别大时是超线性的（实测 3k 条语句要 8 分钟），拆开就回到线性；
     * 2) 剩下的小语句超过 120 条就按 120 一组，各生成一个 init crate；
     * 3) 都不满足就全放 app，一次调用搞定。 */
    let mut uses_mcall = !methods.is_empty();
    let mut uses_dcall = false;

    fn make_top_cg(
        meta: &HashMap<Rc<str>, FnMeta>,
        globals: &[Rc<str>],
        gslots0: &HashMap<Rc<str>, usize>,
        int_globals: &HashSet<Rc<str>>,
        gi_list0: &[Rc<str>],
        gi_slots0: &HashMap<Rc<str>, usize>,
        classes: &HashSet<Rc<str>>,
        chunk_of: &HashMap<Rc<str>, usize>,
        lambdas: &HashMap<(u32, u32), Rc<str>>,
    ) -> Cg {
        Cg {
            meta: meta.clone(),
            globals: globals.to_vec(),
            gslots: gslots0.clone(),
            int_globals: int_globals.clone(),
            gi_list: gi_list0.to_vec(),
            gi_slots: gi_slots0.clone(),
            int_locals: None,
            classes: classes.clone(),
            out: String::new(),
            indent: 1,
            loop_depth: 0,
            locals: None,
            try_stack: Vec::new(),
            lambdas: lambdas.clone(),
            uses_mcall: false,
            uses_dcall: false,
            whole_int: false,
        float_mode: false,
            split: true,
            my_chunk: usize::MAX,
            chunk_of: chunk_of.clone(),
        }
    }

    /// 顶层序列里的一项：普通语句，或者"调某个 data crate 把它算出来"
    enum TopItem {
        Inline(Stmt),
        Data(usize, String),
    }

    let mut items: Vec<TopItem> = Vec::new();
    let mut n_data = 0usize;
    for st in &top_stmts {
        let big = count_nodes(st) > 600;
        // 只有"声明 / 直接赋值 / 表达式语句"能抽成 builder
        let extractable = matches!(
            &st.kind,
            StmtKind::Decl { init: Some(_), .. }
                | StmtKind::Assign { op: AssignOp::Set, .. }
                | StmtKind::ExprStmt(_)
        );
        if big && extractable {
            let mut dcg = make_top_cg(
                &meta, &globals, &gslots0, &int_globals, &gi_list0, &gi_slots0, &classes, &chunk_of, &lambda_map,
            );
            // 取出要算的表达式
            let expr: &Expr = match &st.kind {
                StmtKind::Decl { init: Some(e), .. } => e,
                StmtKind::Assign { value, .. } => value,
                StmtKind::ExprStmt(e) => e,
                _ => unreachable!(),
            };
            let code = emit_expr(&mut dcg, expr)?;
            let lib = format!(
                "{}\nuse copper_rt::*;\n\n#[allow(unused_mut)]\npub fn build(g: &mut Globals) -> R<V> {{\n    Ok({})\n}}\n",
                ALLOW, code
            );
            files.push(SplitFile {
                rel: format!("data{}/Cargo.toml", n_data),
                content: format!(
                    "[package]\nname = \"copper_data{}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\ncopper_rt = {{ path = \"../rt\" }}\n",
                    n_data
                ),
            });
            files.push(SplitFile { rel: format!("data{}/src/lib.rs", n_data), content: lib });
            uses_mcall |= dcg.uses_mcall;
            uses_dcall |= dcg.uses_dcall;

            // app 里替换成一句"调 builder"
            let call = format!("copper_data{}::build(g)?", n_data);
            let line = match &st.kind {
                StmtKind::Decl { name, .. } => {
                    if acg.is_int_var(name) {
                        let slot = acg.gi_slot(name);
                        format!("g.gi[{}] = want_int_at(&{}, {}, {})?;", slot, call, st.span.line, st.span.col)
                    } else {
                        let slot = acg.slot_of(name);
                        format!("g.v[{}] = {};", slot, call)
                    }
                }
                StmtKind::Assign { target: Target::Var(name), .. } => {
                    if acg.is_int_var(name) {
                        let slot = acg.gi_slot(name);
                        format!("g.gi[{}] = want_int_at(&{}, {}, {})?;", slot, call, st.span.line, st.span.col)
                    } else {
                        let slot = acg.slot_of(name);
                        format!("g.v[{}] = {};", slot, call)
                    }
                }
                _ => format!("let _ = {};", call),
            };
            items.push(TopItem::Data(n_data, line));
            n_data += 1;
        } else {
            items.push(TopItem::Inline(st.clone()));
        }
    }

    // 剩下的小语句：超过 120 条就分组（每组一个 init crate）
    let inline_count = items.iter().filter(|i| matches!(i, TopItem::Inline(_))).count();
    let group_top = inline_count > 120;

    // 按顺序产出顶层代码：要么直接写进 app，要么分组写进 init crate
    let mut app_top = String::new();
    let mut init_code: Vec<String> = Vec::new();
    let mut cur: Option<Cg> = None;
    let mut cur_count = 0usize;
    let flush = |cg: &mut Option<Cg>, code: &mut Vec<String>, app_top: &mut String, uses_mcall: &mut bool, uses_dcall: &mut bool| {
        if let Some(mut c) = cg.take() {
            *uses_mcall |= c.uses_mcall;
            *uses_dcall |= c.uses_dcall;
            if group_top {
                code.push(std::mem::take(&mut c.out));
            } else {
                app_top.push_str(&c.out);
            }
        }
    };

    for item in &items {
        match item {
            TopItem::Data(_, line) => {
                if group_top {
                    if let Some(c) = cur.as_mut() {
                        c.w(line.clone());
                        cur_count += 1;
                        if cur_count >= 120 {
                            flush(&mut cur, &mut init_code, &mut app_top, &mut uses_mcall, &mut uses_dcall);
                            cur_count = 0;
                        }
                    } else {
                        let mut c = make_top_cg(&meta, &globals, &gslots0, &int_globals, &gi_list0, &gi_slots0, &classes, &chunk_of, &lambda_map);
                        c.w(line.clone());
                        cur = Some(c);
                        cur_count = 1;
                    }
                } else {
                    app_top.push_str("    ");
                    app_top.push_str(line);
                    app_top.push('\n');
                }
            }
            TopItem::Inline(st) => {
                if group_top {
                    if cur.is_none() {
                        cur = Some(make_top_cg(&meta, &globals, &gslots0, &int_globals, &gi_list0, &gi_slots0, &classes, &chunk_of, &lambda_map));
                        cur_count = 0;
                    }
                    {
                        let c = cur.as_mut().unwrap();
                        emit_stmt(c, st, true)?;
                    }
                    cur_count += 1;
                    if cur_count >= 120 {
                        flush(&mut cur, &mut init_code, &mut app_top, &mut uses_mcall, &mut uses_dcall);
                        cur_count = 0;
                    }
                } else {
                    {
                        let c = cur.get_or_insert_with(|| {
                            make_top_cg(&meta, &globals, &gslots0, &int_globals, &gi_list0, &gi_slots0, &classes, &chunk_of, &lambda_map)
                        });
                        emit_stmt(c, st, true)?;
                    }
                }
            }
        }
    }
    flush(&mut cur, &mut init_code, &mut app_top, &mut uses_mcall, &mut uses_dcall);

    if acg.globals.len() != globals.len() {
        let extra: Vec<String> = acg
            .globals
            .iter()
            .filter(|n| !globals.iter().any(|g| g == *n))
            .map(|n| n.to_string())
            .collect();
        return Err(AotError::msg(format!("内部错误：全局槽位在分块时不一致（多出来的：{:?}）", extra)));
    }

    // 分组后每块单独一个 crate（只依赖 rt，跨块/方法调用都走注册表）
    if group_top {
        for (k, code) in init_code.iter().enumerate() {
            let lib = format!(
                "{}\nuse copper_rt::*;\n\n#[allow(unused_mut)]\npub fn run(g: &mut Globals) -> R<V> {{\n{}    Ok(V::Null)\n}}\n",
                ALLOW, code
            );
            files.push(SplitFile {
                rel: format!("init{}/Cargo.toml", k),
                content: format!(
                    "[package]\nname = \"copper_init{}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\ncopper_rt = {{ path = \"../rt\" }}\n",
                    k
                ),
            });
            files.push(SplitFile { rel: format!("init{}/src/lib.rs", k), content: lib });
        }
    }

    let mut app = String::new();
    app.push_str(ALLOW);
    app.push_str("use copper_rt::*;\n\n");
    if uses_mcall {
        app.push_str(&gen_mcall(&methods, true));
        app.push('\n');
    }
    if uses_dcall {
        app.push_str(
            "fn dcall(g: &mut Globals, f: &V, args: Vec<V>, ln: u32, cl: u32) -> R<V> {\n    match f {\n        V::Native(nm) => call_named(g, nm, args).map_err(|m| fe(ln, cl, m)),\n        V::Func(u) => call_named(g, u.name.as_ref(), args).map_err(|m| fe(ln, cl, m)),\n        other => Err(fe(ln, cl, format!(\"{} 不是一个函数\", other.to_display()))),\n    }\n}\n\n",
        );
    }
    if !group_top {
        app.push_str(&format!(
            "#[allow(unused_mut)]\nfn run_top(g: &mut Globals) -> R<V> {{\n{}    Ok(V::Null)\n}}\n\n",
            app_top
        ));
    }
    app.push_str("fn main() {\n");
    // 大栈线程：和单文件路径一致，深递归不爆栈（虚拟预留、按需提交）
    app.push_str("    let __h = std::thread::Builder::new().stack_size(512 * 1024 * 1024).spawn(|| {\n");
    for k in 0..nchunks {
        app.push_str(&format!("    register(copper_chunk{}::TABLE);\n", k));
    }
    // 同上：分块路径也要按实际生成的代码开够槽，不然 chunk 里新增的全局会越界
    let mut need_v = globals.len();
    let mut need_gi = gi_list0.len();
    for f in &files {
        if f.rel.ends_with(".rs") {
            needed_slots(&f.content, &mut need_v, &mut need_gi);
        }
    }
    app.push_str(&format!(
        "    let mut g = Globals {{ v: vec![V::Null; {}], gi: vec![0i64; {}], err: 0 }};\n",
        need_v,
        need_gi
    ));
    if group_top {
        // 顶层语句按顺序跑：任意一块出错就整体停下（和单块 run_top 的语义一致）
        app.push_str("    let __r: R<()> = (|| -> R<()> {\n");
        for k in 0..init_code.len() {
            app.push_str(&format!("        copper_init{}::run(&mut g)?;\n", k));
        }
        app.push_str("        Ok(())\n    })();\n");
        app.push_str("    if let Err(e) = __r {\n        out_flush();\n        use std::io::Write as _; let _ = std::io::stdout().flush();\n        eprintln!(\"出错（第 {} 行）：{}\", e.span.line, e.message);\n        std::process::exit(1);\n    }\n    out_flush();\n    }).unwrap();\n    __h.join().unwrap();\n}\n");
    } else {
        app.push_str("    match run_top(&mut g) {\n        Ok(_) => {}\n        Err(e) => {\n            use std::io::Write as _; let _ = std::io::stdout().flush();\n            eprintln!(\"出错（第 {} 行）：{}\", e.span.line, e.message);\n            std::process::exit(1);\n        }\n    }\n    }).unwrap();\n    __h.join().unwrap();\n}\n");
    }

    let mut app_deps = String::from("copper_rt = { path = \"../rt\" }\n");
    for k in 0..nchunks {
        app_deps.push_str(&format!("copper_chunk{} = {{ path = \"../chunk{}\" }}\n", k, k));
    }
    if group_top {
        for k in 0..init_code.len() {
            app_deps.push_str(&format!("copper_init{} = {{ path = \"../init{}\" }}\n", k, k));
        }
    }
    for k in 0..n_data {
        app_deps.push_str(&format!("copper_data{} = {{ path = \"../data{}\" }}\n", k, k));
    }
    files.push(SplitFile {
        rel: "app/Cargo.toml".into(),
        content: format!(
            "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[[bin]]\nname = \"{}\"\npath = \"src/main.rs\"\n\n[dependencies]\n{}",
            bin, bin, app_deps
        ),
    });
    files.push(SplitFile { rel: "app/src/main.rs".into(), content: app });

    // 最后补 workspace 根清单（成员这时才全部确定）
    for k in 0..n_data {
        members.push(format!("data{}", k));
    }
    if group_top {
        for k in 0..init_code.len() {
            members.push(format!("init{}", k));
        }
    }
    members.push("app".into());
    files.push(SplitFile {
        rel: "Cargo.toml".into(),
        content: format!(
            /* opt-level 3 + thin LTO + 单 codegen 单元：运行性能优先。
             * thin LTO 比 fat 轻得多（内存峰值可控），配合构建侧 -j 2 限流；
             * 分块路径还有共享缓存兜底：只有改动的块会重编。 */
            "[workspace]\nresolver = \"2\"\nmembers = [{}]\n\n[profile.release]\nopt-level = 3\nlto = \"thin\"\ncodegen-units = 1\npanic = \"abort\"\nincremental = false\ndebug = false\nstrip = \"symbols\"\n",
            members.iter().map(|m| format!("\"{}\"", m)).collect::<Vec<_>>().join(", ")
        ),
    });

    Ok(files)
}

fn gen_mcall(methods: &[(String, String, usize)], split: bool) -> String {
    let mut s = String::new();
    s.push_str("fn mcall(g: &mut Globals, o: &V, name: &str, args: Vec<V>, ln: u32, cl: u32) -> R<V> {\n");
    s.push_str("    if let V::Dict(d) = o {\n");
    s.push_str("        if let Some(cv) = d.borrow().get(\"__cls\") {\n");
    s.push_str("            let mut cur = cv.to_display();\n");
    s.push_str("            loop {\n");
    s.push_str("                match (cur.as_str(), name) {\n");
    for (cls, meth, n) in methods {
        let fname = rust_fn_name(&format!("__m_{}_{}", cls, meth));
        if split {
            // 分块模式：方法函数在某个块里，走注册表
            s.push_str(&format!(
                "                    (\"{}\", \"{}\") if args.len() == {} => {{\n                        let mut __a = vec![o.clone()];\n                        __a.extend(args);\n                        return call_named(g, {}, __a).map_err(|m| fe(ln, cl, m));\n                    }}\n",
                cls,
                meth,
                n,
                lit_str(&format!("__m_{}_{}", cls, meth))
            ));
        } else {
            let mut call = String::from("g, o.clone()");
            for i in 0..*n {
                call.push_str(&format!(", args[{}].clone()", i));
            }
            s.push_str(&format!(
                "                    (\"{}\", \"{}\") if args.len() == {} => return {}({}),\n",
                cls, meth, n, fname, call
            ));
        }
    }
    s.push_str("                    _ => {}\n");
    s.push_str("                }\n");
    s.push_str("                match d.borrow().get(\"__base\") {\n");
    s.push_str("                    Some(b) => cur = b.to_display(),\n");
    s.push_str("                    None => break,\n");
    s.push_str("                }\n");
    s.push_str("            }\n");
    s.push_str("            return dict_method(d, name, &args).map_err(|m| fe(ln, cl, m));\n");
    s.push_str("        }\n");
    s.push_str("    }\n");
    s.push_str("    method_plain(o, name, &args).map_err(|m| fe(ln, cl, m))\n");
    s.push_str("}\n");
    s
}

fn gen_dcall(meta: &HashMap<Rc<str>, FnMeta>) -> String {
    let mut s = String::new();
    s.push_str("fn dcall(g: &mut Globals, f: &V, args: Vec<V>, ln: u32, cl: u32) -> R<V> {\n");
    s.push_str("    match f {\n");
    s.push_str("        V::Native(nm) => dcall_by_name(g, nm, args, ln, cl),\n");
    s.push_str("        V::Func(u) => dcall_by_name(g, u.name.as_ref(), args, ln, cl),\n");
    s.push_str("        other => Err(fe(ln, cl, format!(\"{} 不是一个函数\", other.to_display()))),\n");
    s.push_str("    }\n");
    s.push_str("}\n\n");
    s.push_str("fn dcall_by_name(g: &mut Globals, nm: &str, args: Vec<V>, ln: u32, cl: u32) -> R<V> {\n");
    s.push_str("    match (nm, args.len()) {\n");
    for (name, m) in meta {
        // lambda（__lambda_行_列）就是要被动态调用的，必须生成臂；
        // 其余 __ 开头的是类内部函数，不走动态调用
        if name.starts_with("__") && !name.starts_with("__lambda_") {
            continue;
        }
        let fname = rust_fn_name(name);
        if m.float_fn {
            // 浮点函数：V 收敛成 f64（Int 自动转，Float 直取——与 VM 的混合
            // 算术语义一致）。动态路实参不合法时 want_f64_at 明确报错
            let mut conv = String::new();
            for i in 0..m.arity {
                conv.push_str(&format!(", want_f64_at(&args[{}], ln, cl)?", i));
            }
            s.push_str(&format!(
                "        (\"{}\", {}) => {{ let r = {}(g, 0{}); if g.err != 0 {{ return Err(fe(ln, cl, String::from(\"递归太深\"))) }} Ok(V::Float(r)) }},\n",
                name, m.arity, fname, conv
            ));
            continue;
        }
        if m.int_fn {
            // 整型函数：V 收敛成 i64。实参万一不是整数（推断器已排除所有
            // 静态调用点，只有运行期函数值这条动态路可能遇到），
            // want_int_at 会明确报错而不是悄悄算错
            let mut conv = String::new();
            for i in 0..m.arity {
                conv.push_str(&format!(", want_int_at(&args[{}], ln, cl)?", i));
            }
            s.push_str(&format!(
                "        (\"{}\", {}) => {{ let r = {}(g, 0{}); if g.err != 0 {{ return Err(fe(ln, cl, if g.err == 2 {{ String::from(\"整数溢出（超出 i64 范围）\") }} else {{ String::from(\"递归太深\") }})) }} Ok(V::Int(r)) }},\n",
                name, m.arity, fname, conv
            ));
            continue;
        }
        let mut params = String::new();
        params.push_str("g");
        for i in 0..m.arity {
            params.push_str(&format!(", args[{}].clone()", i));
        }
        s.push_str(&format!("        (\"{}\", {}) => {}({}),\n", name, m.arity, fname, params));
    }
    // data.* 高阶函数臂：转发到 hof_dispatch（prelude 的运行时实现，
    // 回调经 hof_call → dcall_by_name 静态表调用）
    for (hn, harity) in [
        ("data.map", 2usize),
        ("data.filter", 2),
        ("data.reduce", 3),
        ("data.groupby", 2),
        ("data.index_by", 2),
        ("data.any", 2),
        ("data.all", 2),
        ("data.find", 2),
        ("data.find_index", 2),
    ] {
        s.push_str(&format!(
            "        (\"{}\", {}) => hof_dispatch(g, \"{}\", args.clone(), ln, cl).unwrap_or_else(|| call_builtin(\"{}\", &args)),\n",
            hn, harity, hn, hn
        ));
    }
    s.push_str("        _ => call_builtin(nm, &args).map_err(|m| fe(ln, cl, m)),\n");
    s.push_str("    }\n");
    s.push_str("}\n\n");
    s.push_str(
        "fn hof_call(g: &mut Globals, f: &V, a: Vec<V>, ln: u32, cl: u32) -> R<V> {\n    match f {\n        V::Native(nm) => dcall_by_name(g, nm, a, ln, cl),\n        V::Func(u) => dcall_by_name(g, u.name.as_ref(), a, ln, cl),\n        other => Err(fe(ln, cl, format!(\"{} 不是一个函数\", other.to_display()))),\n    }\n}\n\n",
    );
    s.push_str(
        "fn hof_dispatch(g: &mut Globals, name: &str, args: Vec<V>, ln: u32, cl: u32) -> Option<R<V>> {\n    let arity = if name == \"data.reduce\" { 3 } else { 2 };\n    if !name.starts_with(\"data.\") || args.len() != arity { return None; }\n    if hof_iter(&args[0]).is_none() { return None; }\n    Some(hof_run(g, name, args, ln, cl, hof_call))\n}\n\n",
    );
    s
}

/* ───────────────────────── 运行时前奏 ───────────────────────── */

const PRELUDE: &str = r#"// ── 运行时前奏：全部复用 float-lang 的实现，语义和 VM 完全一致 ──
pub use std::rc::Rc;
pub use float_lang::builtins::{call_builtin, str_method, list_method, dict_method};
pub use float_lang::value::Dict;
pub use float_lang::bytecode::Op;
pub use float_lang::vm::{arith, compare, compound_value, index_get, index_set, member_get, member_set, slice_value};

thread_local! {
    pub static DEPTH: std::cell::Cell<u32> = std::cell::Cell::new(0);
}

/// 全局变量容器：v 放任意值，gi 放能证明是整数的变量（不碰 Value 枚举），
/// err 是整型特化函数的错误标记（非 0 = 递归超限，向上传播，run_top 出口转成报错）
pub struct Globals {
    pub v: Vec<V>,
    pub gi: Vec<i64>,
    pub err: u8,
}

pub struct DepthGuard;
impl Drop for DepthGuard {
    fn drop(&mut self) {
        DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

/// 递归上限：默认跟 AOT 整型特化路径一致（100 万层），可用 COPPER_MAX_DEPTH 覆盖。
/// 生成程序本来就跑在 512MB 虚拟预留的大栈线程上，所以这里不能再用 400 层的老限。
pub fn depth_limit() -> usize {
    static LIM: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *LIM.get_or_init(|| {
        std::env::var("COPPER_MAX_DEPTH")
            .ok()
            .and_then(|s| s.trim().parse::<usize>().ok())
            .filter(|n| *n > 0)
            .unwrap_or(float_lang::limits::INT_CALL_DEPTH)
    })
}

/// 进入一个 Copper 函数：V 路径的递归上限见 depth_limit()，退栈时自动减回去
pub fn enter(fname: &str) -> R<DepthGuard> {
    let lim = depth_limit();
    if DEPTH.with(|d| d.get() as usize) >= lim {
        return Err(fe(
            0,
            0,
            format!(
                "递归太深了（当前 {} 层，上限 {} 层），多半是 {} 的结束条件没写对",
                DEPTH.with(|d| d.get()),
                lim,
                fname
            ),
        ));
    }
    DEPTH.with(|d| d.set(d.get() + 1));
    Ok(DepthGuard)
}

pub type V = float_lang::Value;
pub type FE = float_lang::FloatError;
pub type R<T> = Result<T, FE>;

/// 方法调用的统一入口：按接收者类型分派到 VM 的同一套实现。
/// 这样 AOT/自举发射器都不用各自猜方法语义
pub fn method_call(o: &V, name: &str, args: &[V], ln: u32) -> R<V> {
    match o {
        V::List(l) => list_method(l, name, args).map_err(|m| fe(ln, 1, m)),
        V::Dict(d) => dict_method(d, name, args).map_err(|m| fe(ln, 1, m)),
        V::Str(s) => str_method(s, name, args).map_err(|m| fe(ln, 1, m)),
        other => Err(fe(ln, 1, format!("「{}」没有方法 {}", show(&[other.clone()]), name))),
    }
}

pub fn fe(line: u32, col: u32, msg: impl Into<String>) -> FE {
    FE::new(msg, float_lang::error::Span::new(&Rc::from("main.co"), line, col, col + 1))
}
#[inline(always)]
pub fn t(v: &V) -> bool { v.truthy() }
pub fn disp(v: &V) -> String { v.to_display() }
#[inline(always)]
pub fn show(items: &[V]) -> String {
    let mut s = String::new();
    for (i, v) in items.iter().enumerate() {
        if i > 0 { s.push(' '); }
        s.push_str(&v.to_display());
    }
    s
}
pub fn num_to_v(x: f64) -> V {
    if x.fract() == 0.0 && x.abs() < 9.0e18 { V::Int(x as i64) } else { V::Float(x) }
}
#[inline(always)]
pub fn want_int(v: &V) -> Result<i64, String> {
    match v {
        V::Int(n) => Ok(*n),
        V::Float(f) => Ok(*f as i64),
        V::Bool(b) => Ok(if *b { 1 } else { 0 }),
        other => Err(format!("这里要整数，得到 {}", other.type_name())),
    }
}
#[inline(always)]
pub fn want_int_at(v: &V, ln: u32, cl: u32) -> R<i64> {
    want_int(v).map_err(|m| fe(ln, cl, m))
}

/// 字符串就地追加：`s = s + x` 的快速路径。不是字符串就返回 false 让调用方走通用路径
pub fn str_append_in_place(v: &mut V, add: &str) -> bool {
    if let V::Str(r) = v {
        Rc::make_mut(r).push_str(add);
        true
    } else {
        false
    }
}

/// 同上，但追加的东西已经是 Value（`s = s + 变量` 的写法）
pub fn str_append_val(v: &mut V, add: &V) -> bool {
    match (v, add) {
        (V::Str(r), V::Str(a)) => {
            Rc::make_mut(r).push_str(a);
            true
        }
        _ => false,
    }
}

/// 输出缓冲：VM 是全收在 out 里最后一次写，AOT 要是每行 println 就是每行一次 syscall，
/// 大批量输出（比如游戏刷屏）会慢一个量级。这里攒到 64KB 再写，语义完全一样。
thread_local! {
    pub static OUTBUF: std::cell::RefCell<String> = std::cell::RefCell::new(String::new());
    pub static OUTTOTAL: std::cell::Cell<u64> = std::cell::Cell::new(0);
}
#[inline(always)]
pub fn out_line(s: &str) {
    OUTBUF.with(|b| {
        let mut b = b.borrow_mut();
        b.push_str(s);
        b.push('\n');
        OUTTOTAL.with(|t| t.set(t.get() + s.len() as u64 + 1));
        // 和 VM 同一条闸门：输出超过 32MB 说明循环里一直在 print，直接停，不把内存吃光
        OUTTOTAL.with(|t| {
            if t.get() > 32 * 1024 * 1024 {
                print!("{}", *b);
                b.clear();
                use std::io::Write as _;
                let _ = std::io::stdout().flush();
                eprintln!("出错：输出超过 32 MB，停下来了（多半是循环里一直在 print）");
                std::process::exit(1);
            }
        });
        if b.len() >= 65536 {
            print!("{}", *b);
            b.clear();
        }
    });
}
pub fn out_flush() {
    OUTBUF.with(|b| {
        let mut b = b.borrow_mut();
        if !b.is_empty() {
            print!("{}", *b);
            b.clear();
        }
    });
}

/// 读一行输入（`ask`）：和 VM 的控制台风格一致——回声非空输入，EOF 给空串。
/// 只有在终端里才每行 flush（管道/重定向时 flush 是纯 syscall 开销）
pub fn ask_line() -> R<V> {
    use std::io::{BufRead, IsTerminal, Write};
    if std::io::stdin().is_terminal() {
        out_flush();
        let _ = std::io::stdout().flush();
    }
    let mut s = String::new();
    let _ = std::io::stdin().lock().read_line(&mut s);
    let t = s.trim_end_matches(['\r', '\n']).to_string();
    if !t.is_empty() {
        out_line(&t);
    }
    Ok(V::string(t))
}
/// 浮点函数特化的实参收敛：Int 自动转 f64、Float 直取，语义与 VM 的混合
/// 算术（整数和浮点一起算自动升浮点）一致；其他类型明确报错，不静默算错
pub fn want_f64(v: &V) -> Result<f64, String> {
    match v {
        V::Int(n) => Ok(*n as f64),
        V::Float(f) => Ok(*f),
        V::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
        other => Err(format!("这里要数字，得到 {}", other.type_name())),
    }
}
pub fn want_f64_at(v: &V, ln: u32, cl: u32) -> R<f64> {
    want_f64(v).map_err(|m| fe(ln, cl, m))
}
pub fn as_i64(v: &V, ln: u32, cl: u32) -> R<i64> {
    v.as_int().ok_or_else(|| fe(ln, cl, "这里要一个整数"))
}
// ── 算术快路径：整数直接算，绕开 VM 的通用 arith（这是 AOT 比解释器快的关键）──
#[inline(always)]
#[inline(always)]
pub fn v_add(a: &V, b: &V) -> Result<V, String> {
    if let (V::Int(x), V::Int(y)) = (a, b) {
        if let Some(v) = x.checked_add(*y) { return Ok(V::Int(v)); }
    }
    arith(Op::Add, a, b)
}
#[inline(always)]
#[inline(always)]
pub fn v_sub(a: &V, b: &V) -> Result<V, String> {
    if let (V::Int(x), V::Int(y)) = (a, b) {
        if let Some(v) = x.checked_sub(*y) { return Ok(V::Int(v)); }
    }
    arith(Op::Sub, a, b)
}
#[inline(always)]
#[inline(always)]
pub fn v_mul(a: &V, b: &V) -> Result<V, String> {
    if let (V::Int(x), V::Int(y)) = (a, b) {
        if let Some(v) = x.checked_mul(*y) { return Ok(V::Int(v)); }
    }
    arith(Op::Mul, a, b)
}
#[inline(always)]
#[inline(always)]
pub fn v_cmp(op: Op, a: &V, b: &V) -> Result<V, String> {
    if let (V::Int(x), V::Int(y)) = (a, b) {
        return Ok(V::Bool(match op {
            Op::Eq => x == y,
            Op::Ne => x != y,
            Op::Lt => x < y,
            Op::Le => x <= y,
            Op::Gt => x > y,
            Op::Ge => x >= y,
            _ => return Ok(V::Bool(compare(op, a, b)?)),
        }));
    }
    Ok(V::Bool(compare(op, a, b)?))
}
#[inline(always)]
#[inline(always)]
pub fn v_bit(op: Op, a: &V, b: &V) -> Result<V, String> {
    if let (V::Int(x), V::Int(y)) = (a, b) {
        let (x, y) = (*x, *y);
        return Ok(V::Int(match op {
            Op::BitAnd => x & y,
            Op::BitOr => x | y,
            Op::BitXor => x ^ y,
            Op::Shl => x << (y & 63),
            Op::Shr => x >> (y & 63),
            _ => unreachable!(),
        }));
    }
    Err(format!("位运算要整数，得到 {} 和 {}", a.type_name(), b.type_name()))
}
#[inline(always)]
pub fn neg(v: &V) -> Result<V, String> {
    match v {
        V::Int(n) => Ok(V::Int(-n)),
        V::Float(f) => Ok(V::Float(-f)),
        other => Err(format!("负号后面得是数字，得到 {}", other.to_display())),
    }
}
pub fn sand(a: impl FnOnce() -> R<V>, b: impl FnOnce() -> R<V>) -> R<V> {
    let l = a()?;
    if t(&l) { b() } else { Ok(l) }
}
pub fn sor(a: impl FnOnce() -> R<V>, b: impl FnOnce() -> R<V>) -> R<V> {
    let l = a()?;
    if t(&l) { Ok(l) } else { b() }
}
/* ── 流式 for-in ──
 * 以前 for x in xs 先把整个容器 clone 成 Vec 再遍历：10 万元素 = 10 万次
 * 元素克隆 × 2（一次在 Vec 里、一次给循环变量）。现在只持有容器的引用
 * 按需取第 i 个（对齐 VM 的 IterSrc——也是引用实时取），每圈只克隆
 * 循环变量那一次，10 万元素从 ~8ms 降到 <1ms。 */
pub enum CopperIter {
    List(float_lang::value::ListRef, usize),
    Dict(float_lang::value::DictRef, Vec<Rc<str>>, usize),
    Str { s: Rc<String>, i: usize },
}

impl Iterator for CopperIter {
    type Item = V;
    fn next(&mut self) -> Option<V> {
        match self {
            CopperIter::List(l, i) => {
                let r = l.borrow().get(*i).cloned();
                *i += 1;
                r
            }
            CopperIter::Dict(_, ks, i) => {
                let r = ks.get(*i).map(|k| V::string(k.to_string()));
                *i += 1;
                r
            }
            CopperIter::Str { s, i } => {
                let rest = s.get(*i..)?;
                let c = rest.chars().next()?;
                *i += c.len_utf8();
                Some(V::string(c.to_string()))
            }
        }
    }
}

pub fn iter_src(v: &V) -> Result<CopperIter, String> {
    match v {
        V::List(l) => Ok(CopperIter::List(Rc::clone(l), 0)),
        V::Dict(d) => Ok(CopperIter::Dict(Rc::clone(d), d.borrow().keys().to_vec(), 0)),
        V::Str(s) => Ok(CopperIter::Str { s: Rc::clone(s), i: 0 }),
        other => Err(format!("{} 不能 for-in 迭代", other.to_display())),
    }
}

pub enum CopperPairIter {
    List(float_lang::value::ListRef, usize),
    Dict(float_lang::value::DictRef, Vec<(Rc<str>, V)>, usize),
}

impl Iterator for CopperPairIter {
    type Item = (V, V);
    fn next(&mut self) -> Option<(V, V)> {
        match self {
            CopperPairIter::List(l, i) => {
                let r = l.borrow().get(*i).cloned().map(|x| (V::Int(*i as i64), x));
                *i += 1;
                r
            }
            CopperPairIter::Dict(_, ks, i) => {
                let r = ks
                    .get(*i)
                    .map(|(k, val)| (V::string(k.to_string()), val.clone()));
                *i += 1;
                r
            }
        }
    }
}

pub fn iter_pair_src(v: &V) -> Result<CopperPairIter, String> {
    match v {
        V::List(l) => Ok(CopperPairIter::List(Rc::clone(l), 0)),
        V::Dict(d) => Ok(CopperPairIter::Dict(
            Rc::clone(d),
            d.borrow()
                .entries()
                .map(|(k, val)| (Rc::clone(k), val.clone()))
                .collect(),
            0,
        )),
        other => Err(format!("{} 不能双变量 for-in 迭代", other.to_display())),
    }
}
/* ── 大表格的"紧凑数据"解码 ──
 * 上千行的地图/敌人表如果逐条生成 V::list(...) 语句，rustc 会慢到几分钟。
 * 所以这类纯字面量在编译期被编码成一小段 JSON 子集，运行时一次解析成 Value。
 * 解析 2000 行大约 0.5ms，比编译器省下几分钟。 */
pub fn parse_data(s: &str) -> Result<V, String> {
    let mut p = DataParser { b: s.as_bytes(), i: 0 };
    let v = p.value()?;
    if p.i != p.b.len() {
        return Err(format!("数据表解析后还剩 {} 个字符", p.b.len() - p.i));
    }
    Ok(v)
}

struct DataParser<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> DataParser<'a> {
    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ') | Some(b'\n') | Some(b'\r') | Some(b'\t')) {
            self.i += 1;
        }
    }

    fn expect(&mut self, c: u8) -> Result<(), String> {
        if self.peek() == Some(c) {
            self.i += 1;
            Ok(())
        } else {
            Err(format!("数据表格式错误：第 {} 个字符处应该是 {}", self.i, c as char))
        }
    }

    fn value(&mut self) -> Result<V, String> {
        self.skip_ws();
        match self.peek() {
            Some(b'[') => self.list(),
            Some(b'{') => self.dict(),
            Some(b'"') => Ok(V::string(self.string()?)),
            Some(b't') => {
                self.i += 4;
                Ok(V::Bool(true))
            }
            Some(b'f') => {
                self.i += 5;
                Ok(V::Bool(false))
            }
            Some(b'n') => {
                self.i += 4;
                Ok(V::Null)
            }
            Some(c) if c == b'-' || c.is_ascii_digit() => self.number(),
            other => Err(format!("数据表里遇到看不懂的字符：{:?}", other)),
        }
    }

    fn list(&mut self) -> Result<V, String> {
        self.expect(b'[')?;
        let mut items: Vec<V> = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.i += 1;
            return Ok(V::list(items));
        }
        loop {
            items.push(self.value()?);
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.i += 1;
                }
                Some(b']') => {
                    self.i += 1;
                    break;
                }
                _ => return Err(format!("数据表里列表没闭合（第 {} 个字符）", self.i)),
            }
        }
        Ok(V::list(items))
    }

    fn dict(&mut self) -> Result<V, String> {
        self.expect(b'{')?;
        let mut d = Dict::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.i += 1;
            return Ok(V::Dict(Rc::new(std::cell::RefCell::new(d))));
        }
        loop {
            self.skip_ws();
            let k = self.string()?;
            self.skip_ws();
            self.expect(b':')?;
            let v = self.value()?;
            d.set(Rc::from(k.as_str()), v);
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.i += 1;
                }
                Some(b'}') => {
                    self.i += 1;
                    break;
                }
                _ => return Err(format!("数据表里字典没闭合（第 {} 个字符）", self.i)),
            }
        }
        Ok(V::Dict(Rc::new(std::cell::RefCell::new(d))))
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            let c = self.peek().ok_or_else(|| "数据表里字符串没闭合".to_string())?;
            self.i += 1;
            match c {
                b'"' => break,
                b'\\' => {
                    let e = self.peek().ok_or_else(|| "数据表里转义不完整".to_string())?;
                    self.i += 1;
                    match e {
                        b'n' => out.push('\n'),
                        b't' => out.push('\t'),
                        b'r' => out.push('\r'),
                        b'0' => out.push('\0'),
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'u' => {
                            // \uXXXX
                            let hex = self
                                .b
                                .get(self.i..self.i + 4)
                                .ok_or_else(|| "数据表里 \\u 转义不完整".to_string())?;
                            let s = std::str::from_utf8(hex).map_err(|e| e.to_string())?;
                            let cp = u32::from_str_radix(s, 16).map_err(|e| e.to_string())?;
                            if let Some(ch) = char::from_u32(cp) {
                                out.push(ch);
                            }
                            self.i += 4;
                        }
                        other => out.push(other as char),
                    }
                }
                _ => {
                    // 多字节 UTF-8：按原始字节收集，最后整体转字符串
                    let start = self.i - 1;
                    let mut end = self.i;
                    while end < self.b.len() && (self.b[end] & 0b1100_0000) == 0b1000_0000 {
                        end += 1;
                    }
                    self.i = end;
                    out.push_str(std::str::from_utf8(&self.b[start..end]).map_err(|e| e.to_string())?);
                }
            }
        }
        Ok(out)
    }

    fn number(&mut self) -> Result<V, String> {
        let start = self.i;
        let mut is_float = false;
        if self.peek() == Some(b'-') {
            self.i += 1;
        }
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                self.i += 1;
            } else if c == b'.' || c == b'e' || c == b'E' || c == b'+' || c == b'-' {
                is_float = true;
                self.i += 1;
            } else {
                break;
            }
        }
        let text = std::str::from_utf8(&self.b[start..self.i]).map_err(|e| e.to_string())?;
        if is_float {
            text.parse::<f64>().map(V::Float).map_err(|e| e.to_string())
        } else {
            text.parse::<i64>().map(V::Int).map_err(|e| e.to_string())
        }
    }
}

pub fn method_plain(o: &V, name: &str, args: &[V]) -> Result<V, String> {
    match o {
        V::Str(s) => str_method(s, name, args),
        V::List(l) => list_method(l, name, args),
        V::Dict(d) => dict_method(d, name, args),
        other => Err(format!("{} 没有方法「{}」", other.to_display(), name)),
    }
}

/* ── data.* 高阶函数运行时 ──
 * VM 里这 9 个是编译期内联展开（bytecode::emit_hof），stdlib 没有运行时版。
 * AOT 在这里补运行时实现，语义逐条对齐 emit_hof：
 *   map/filter/reduce(3参)/any、all(短路)/find/find_index(找不到 -1)/
 *   groupby(键→列表桶)/index_by(键→item)。
 * 迭代源对齐 VM 的 iter_source：列表→元素、字典→键、字符串→字符。
 * 回调调用由宿主注入（单文件走 dcall 静态表，分块走运行时注册表）。 */
pub fn hof_iter(src: &V) -> Option<Vec<V>> {
    match src {
        V::List(l) => Some(l.borrow().clone()),
        V::Str(s) => Some(s.chars().map(|c| V::string(c.to_string())).collect()),
        V::Dict(d) => Some(
            d.borrow()
                .keys()
                .iter()
                .map(|k| V::string(k.to_string()))
                .collect(),
        ),
        _ => None,
    }
}

pub type HofCall = fn(&mut Globals, &V, Vec<V>, u32, u32) -> R<V>;

pub fn hof_run(g: &mut Globals, name: &str, args: Vec<V>, ln: u32, cl: u32, callf: HofCall) -> R<V> {
    let arity = if name == "data.reduce" { 3 } else { 2 };
    if args.len() != arity {
        return Err(fe(
            ln,
            cl,
            format!("{} 需要 {} 个参数（收到 {} 个）", name, arity, args.len()),
        ));
    }
    let mut it = match hof_iter(&args[0]) {
        Some(v) => v,
        None => return Err(fe(ln, cl, format!("{} 的第一个参数要能迭代", name))),
    };
    let f = args[1].clone();
    match name {
        "data.map" => {
            let mut out: Vec<V> = Vec::with_capacity(it.len());
            for item in it {
                out.push(callf(g, &f, vec![item], ln, cl)?);
            }
            Ok(V::list(out))
        }
        "data.filter" => {
            let mut out: Vec<V> = Vec::new();
            for item in it {
                if callf(g, &f, vec![item.clone()], ln, cl)?.truthy() {
                    out.push(item);
                }
            }
            Ok(V::list(out))
        }
        "data.reduce" => {
            let mut acc = args[2].clone();
            for item in it {
                acc = callf(g, &f, vec![acc, item], ln, cl)?;
            }
            Ok(acc)
        }
        "data.any" | "data.all" => {
            // 短路语义（对齐 VM 的 JumpIfTrueKeep / JumpIfFalseKeep）：
            // any 已经 true / all 已经 false 就不再调回调
            let want = name == "data.any";
            let mut res = V::Bool(!want);
            for item in it {
                if res.truthy() == want {
                    break;
                }
                res = callf(g, &f, vec![item], ln, cl)?;
            }
            Ok(res)
        }
        "data.find" => {
            for item in it {
                if callf(g, &f, vec![item.clone()], ln, cl)?.truthy() {
                    return Ok(item);
                }
            }
            Ok(V::Null)
        }
        "data.find_index" => {
            for (i, item) in it.into_iter().enumerate() {
                if callf(g, &f, vec![item], ln, cl)?.truthy() {
                    return Ok(V::Int(i as i64));
                }
            }
            Ok(V::Int(-1))
        }
        "data.groupby" | "data.index_by" => {
            let by_group = name == "data.groupby";
            let out = V::dict();
            if let V::Dict(od) = &out {
                for item in it {
                    let key = callf(g, &f, vec![item.clone()], ln, cl)?;
                    let ks = key.to_display();
                    let cur = od.borrow().get(&ks);
                    match cur {
                        Some(bucket) => {
                            // 已有桶：bucket.push(item)
                            if let V::List(bl) = &bucket {
                                bl.borrow_mut().push(item);
                            }
                        }
                        None => {
                            // 新桶：groupby 是 [item]，index_by 直接放 item
                            let nl = if by_group { V::list(vec![item]) } else { item };
                            od.borrow_mut().set(Rc::from(ks.as_str()), nl);
                        }
                    }
                }
            }
            Ok(out)
        }
        _ => Err(fe(ln, cl, format!("{} 不是高阶函数", name))),
    }
}
"#;
