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
        let i = self.globals.len();
        self.globals.push(Rc::from(name));
        self.gslots.insert(Rc::from(name), i);
        i
    }

    fn is_local(&self, name: &str) -> bool {
        self.locals.as_ref().map(|l| l.contains(name)).unwrap_or(false)
    }

    fn mark_local(&mut self, name: &str) {
        if let Some(l) = self.locals.as_mut() {
            l.insert(Rc::from(name));
        }
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
    fn assign_var(&mut self, name: &str, val: &str) {
        if self.is_local(name) {
            let l = fmt_local(name);
            self.w(format!("{} = {};", l, val));
        } else {
            let i = self.slot_of(name);
            self.w(format!("g.v[{}] = {};", i, val));
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

fn collect_locals(body: &[Stmt], out: &mut HashSet<Rc<str>>) -> Result<(), AotError> {
    for s in body {
        match &s.kind {
            StmtKind::Decl { name, .. } => {
                out.insert(Rc::clone(name));
            }
            StmtKind::ForRange { var, .. } => {
                out.insert(Rc::clone(var));
            }
            StmtKind::ForIn { var, value_name, .. } => {
                out.insert(Rc::clone(var));
                if let Some(v) = value_name {
                    out.insert(Rc::clone(v));
                }
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
                StmtKind::Decl { ty: crate::TypeName::Int, name, .. } => {
                    out.insert(Rc::clone(name));
                }
                // for 变量是 range 产生的整数；for-in 的变量来源不定，不算
                StmtKind::ForRange { var, .. } => {
                    out.insert(Rc::clone(var));
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
                StmtKind::ForRange { var, body, .. } => {
                    out.insert(Rc::clone(var));
                    find_cands(body, out);
                }
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
                    StmtKind::ForIn { var, value_name, .. } => {
                        if cands.contains(var) {
                            bad.insert(Rc::clone(var));
                        }
                        if let Some(v) = value_name {
                            if cands.contains(v) {
                                bad.insert(Rc::clone(v));
                            }
                        }
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
                    StmtKind::ForRange { body, .. } | StmtKind::ForIn { body, .. } => scan_assigns(body, cands, bad),
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
                if needs_control_flow(body)
                    || needs_control_flow(catch_body.as_deref().unwrap_or(&[]))
                    || needs_control_flow(finally.as_deref().unwrap_or(&[]))
                {
                    return Err(AotError::unsupported("try / catch / finally 里带 return / break / continue", s.span.line));
                }
                scan_body(body, fns)?;
                scan_body(catch_body.as_deref().unwrap_or(&[]), fns)?;
                scan_body(finally.as_deref().unwrap_or(&[]), fns)?;
            }
            StmtKind::Ask(_) => return Err(AotError::unsupported("ask（需要交互宿主）", s.span.line)),
            StmtKind::WaitUntil { .. } => return Err(AotError::unsupported("wait until", s.span.line)),
            StmtKind::TimeRead | StmtKind::TimeReset => return Err(AotError::unsupported("time read / time reset", s.span.line)),
            _ => {}
        }
    }
    Ok(())
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

    let mut meta: HashMap<Rc<str>, FnMeta> = HashMap::new();
    let mut methods: Vec<(String, String, usize)> = Vec::new();
    let mut classes: HashSet<Rc<str>> = HashSet::new();
    for (name, params, _) in &fns {
        meta.insert(
            Rc::clone(name),
            FnMeta { arity: params.len(), defaults: params.iter().map(|p| p.default.clone()).collect() },
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
        split: false,
        my_chunk: 0,
        chunk_of: HashMap::new(),
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
    let uses_mcall = cg.uses_mcall;
    let uses_dcall = cg.uses_dcall;
    let globals = std::mem::take(&mut cg.globals);
    let n_globals = globals.len();
    let n_gi = gi_list0.len();

    // ── 各函数 ──
    let mut fn_code = Vec::new();
    for (name, params, body) in &fns {
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
            split: false,
            my_chunk: 0,
            chunk_of: HashMap::new(),
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
    }

    /* ── 组装 ── */
    let mut doc: Vec<String> = Vec::new();
    doc.push("// 本文件由 Float IDE AOT 自动生成（Copper → Rust）。要改逻辑请改源 .co 再编译。".into());
    doc.push("#![allow(dead_code, unused_imports, unused_variables, unused_mut, unused_parens, non_snake_case, unreachable_patterns, unused_assignments)]".into());
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
    doc.push(format!(
        "    let mut g = Globals {{ v: vec![V::Null; {}], gi: vec![0i64; {}] }};",
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
    doc.push("}".into());
    doc.push(String::new());
    doc.push("#[allow(unused_mut)]".into());
    doc.push("fn run_top(g: &mut Globals) -> R<V> {".into());
    doc.push(body_code);
    doc.push("    Ok(V::Null)".into());
    doc.push("}".into());
    doc.push(String::new());
    doc.push(fn_code.join(""));

    let out = doc.join("\n");

    /* 体积闸门：生成代码太大时 rustc 会吃几个 G 内存（实测把整台机器顶崩过）。
     * 超过阈值就直接拒绝，让上层回落 VM —— 结果一样，但不会把用户的电脑拖死。 */
    const MAX_RUST_BYTES: usize = 400_000;
    const MAX_RUST_LINES: usize = 4_000;
    let lines = out.lines().count();
    if out.len() > MAX_RUST_BYTES || lines > MAX_RUST_LINES {
        return Err(AotError::msg(format!(
            "脚本太大，AOT 跳过（生成 {} 行代码；超过 {} 行或 {} KB 就改用 VM 跑，免得编译吃爆内存）",
            lines,
            MAX_RUST_LINES,
            MAX_RUST_BYTES / 1024
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
    match &s.kind {
        StmtKind::Nop => Ok(()),
        StmtKind::Group(b) => emit_stmts_owned(cg, b, top_level),
        StmtKind::Decl { name, init, .. } => {
            let is_int = cg.is_int_var(name);
            match (is_int, init) {
                (true, Some(e)) => {
                    if let Some(x) = crate::aot::int_expr(cg, e) {
                        cg.write_int(name, &x);
                    } else {
                        let v = emit_expr(cg, e)?;
                        let expr = format!("want_int_at(&({}), {}, {})?", v, s.span.line, s.span.col);
                        cg.write_int(name, &expr);
                    }
                }
                (true, None) => cg.write_int(name, "0"),
                (false, Some(e)) => {
                    let v = emit_expr(cg, e)?;
                    cg.assign_var(name, &v);
                }
                (false, None) => cg.assign_var(name, "V::Null"),
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
            cg.w(format!("println!(\"{{}}\", show(&[{}]));", cloned.join(", ")));
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
                    cg.w(format!("for __item in iter_one(&({})){} {{", it, em(iter)));
                    cg.assign_var(var, "__item.clone()");
                }
                Some(vn) => {
                    cg.w(format!("for (__k, __v) in iter_pair(&({})){} {{", it, em(iter)));
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
            cg.w("break;".into());
            Ok(())
        }
        StmtKind::Continue => {
            if cg.loop_depth == 0 {
                return Err(AotError::unsupported("循环外的 continue", s.span.line));
            }
            cg.w("continue;".into());
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
            cg.w(format!("return Ok({});", v));
            Ok(())
        }
        StmtKind::Throw(e) => {
            let v = emit_expr(cg, e)?;
            cg.w(format!("return Err(fe({}, {}, disp(&({}))));", s.span.line, s.span.col, v));
            Ok(())
        }
        StmtKind::Try { body, catch_name, catch_body, finally } => {
            cg.w("let __tr: R<V> = (|| -> R<V> {".into());
            cg.indent += 1;
            emit_stmts_owned(cg, body, top_level)?;
            cg.indent -= 1;
            cg.w("Ok(V::Null)".into());
            cg.w("})();".into());
            cg.w("match __tr {".into());
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
            cg.indent -= 1;
            cg.w("}".into());
            if let Some(f) = finally {
                emit_stmts_owned(cg, f, top_level)?;
            }
            Ok(())
        }
        StmtKind::Ask(_) => Err(AotError::unsupported("ask（需要交互宿主）", s.span.line)),
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
    let val = emit_expr(cg, value)?;
    let compound = op.to_op();
    match target {
        Target::Var(name) => {
            if cg.is_int_var(name) {
                let cur = cg.read_int(name);
                match compound {
                    Some(o) => {
                        if let Some(x) = crate::aot::int_expr(cg, value) {
                            let sym = match o {
                                Op::Add => "+",
                                Op::Sub => "-",
                                Op::Mul => "*",
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
                    None => cg.w(format!("{} = {};", l, val)),
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
                    None => cg.w(format!("g.v[{}] = {};", i, val)),
                }
            }
            Ok(())
        }
        Target::Member(obj, name) => {
            let o = emit_expr(cg, obj)?;
            match compound {
                Some(o2) => cg.w(format!(
                    "{{ let __o = ({}); let __cur = member_get(&__o, {}){}; let __nv = arith(Op::{:?}, &__cur, &({})){}; member_set(&__o, {}, __nv){}; }}",
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
                    "{{ let __o = ({}); let __i = ({}); let __cur = index_get(&__o, &__i){}; let __nv = arith(Op::{:?}, &__cur, &({})){}; index_set(&__o, &__i, __nv){}; }}",
                    o, ix, em_ln(line, 1), o2, val, em_ln(line, 1), em_ln(line, 1)
                )),
                None => cg.w(format!(
                    "index_set(&({}), &({}), ({})){};",
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
    match &e.kind {
        ExprKind::Num(f) => {
            if f.fract() == 0.0 && f.abs() < 9.0e15 {
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
        ExprKind::Bin { op, a, b } => match op {
            // 只挑没有"溢出退化成浮点"歧义的（除法和幂会变浮点，走通用路径）
            Op::Add | Op::Sub | Op::Mul => {
                let x = int_expr(cg, a)?;
                let y = int_expr(cg, b)?;
                let sym = match op {
                    Op::Add => "+",
                    Op::Sub => "-",
                    _ => "*",
                };
                Some(format!("(({}) {} ({}))", x, sym, y))
            }
            Op::BitAnd | Op::BitOr | Op::BitXor | Op::Shl | Op::Shr => {
                let x = int_expr(cg, a)?;
                let y = int_expr(cg, b)?;
                let sym = match op {
                    Op::BitAnd => "&",
                    Op::BitOr => "|",
                    Op::BitXor => "^",
                    Op::Shl => "<<",
                    _ => ">>",
                };
                Some(format!("(({}) {} ({}))", x, sym, y))
            }
            _ => None,
        },
        _ => None,
    }
}

/// 条件表达式 → bool 代码。都是整数就直接比，省掉 Value 包装。
fn emit_cond(cg: &mut Cg, e: &Expr) -> Result<String, AotError> {
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
                Op::Neg => Ok(format!("neg(&({})){}", aa, em(e))),
                Op::Not => Ok(format!("V::Bool(!t(&({})))", aa)),
                Op::Addr | Op::Deref => Err(AotError::unsupported("指针（& / *）", e.span.line)),
                Op::BitNot => Err(AotError::unsupported("按位取反 ~", e.span.line)),
                _ => Ok(format!("arith(Op::{:?}, &V::Int(0), &({})){}", op, aa, em(e))),
            }
        }
        ExprKind::Bin { op, a, b } => match op {
            Op::And => {
                let aa = emit_expr(cg, a)?;
                let bb = emit_expr(cg, b)?;
                Ok(format!("sand(|| -> R<V> {{ Ok({}) }}, || -> R<V> {{ Ok({}) }})", aa, bb))
            }
            Op::Or => {
                let aa = emit_expr(cg, a)?;
                let bb = emit_expr(cg, b)?;
                Ok(format!("sor(|| -> R<V> {{ Ok({}) }}, || -> R<V> {{ Ok({}) }})", aa, bb))
            }
            Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge => {
                let aa = emit_expr(cg, a)?;
                let bb = emit_expr(cg, b)?;
                Ok(format!("v_cmp(Op::{:?}, &({}), &({})){}", op, aa, bb, em(e)))
            }
            Op::Add | Op::Sub | Op::Mul => {
                let aa = emit_expr(cg, a)?;
                let bb = emit_expr(cg, b)?;
                let f = match op {
                    Op::Add => "v_add",
                    Op::Sub => "v_sub",
                    _ => "v_mul",
                };
                Ok(format!("{}(&({}), &({})){}", f, aa, bb, em(e)))
            }
            Op::BitAnd | Op::BitOr | Op::BitXor | Op::Shl | Op::Shr => {
                let aa = emit_expr(cg, a)?;
                let bb = emit_expr(cg, b)?;
                Ok(format!("v_bit(Op::{:?}, &({}), &({})){}", op, aa, bb, em(e)))
            }
            _ => {
                let aa = emit_expr(cg, a)?;
                let bb = emit_expr(cg, b)?;
                Ok(format!("arith(Op::{:?}, &({}), &({})){}", op, aa, bb, em(e)))
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
                if cg.split {
                    // 分块模式：方法分派在运行时里（块里不知道有哪些类）
                    return Ok(format!(
                        "{{ let __recv = ({}); call_method(g, &__recv, {}, vec![{}], {}, {})? }}",
                        oo,
                        lit_str(name),
                        cloned.join(", "),
                        e.span.line,
                        e.span.col
                    ));
                }
                Ok(format!(
                    "{{ let __recv = ({}); mcall(g, &__recv, {}, vec![{}], {}, {})? }}",
                    oo,
                    lit_str(name),
                    cloned.join(", "),
                    e.span.line,
                    e.span.col
                ))
            }
            ExprKind::Var(name) => emit_call(cg, name, args, e),
            _ => {
                let ff = emit_expr(cg, callee)?;
                let aa = emit_args(cg, args)?;
                cg.uses_dcall = true;
                Ok(format!(
                    "dcall(g, &({}), vec![{}], {}, {})?",
                    ff,
                    aa.join(", "),
                    e.span.line,
                    e.span.col
                ))
            }
        },
    }
}

/// 调用：用户函数（补默认值）→ 内建 → 动态（一等公民函数值）
fn emit_call(cg: &mut Cg, name: &str, args: &[Expr], e: &Expr) -> Result<String, AotError> {
    let aa = emit_args(cg, args)?;
    let call_args = aa.join(", ");

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
        let mut call = format!("{}(g", fname);
        for a in aa.iter() {
            call.push_str(&format!(", ({}).clone()", a));
        }
        // 缺的参数用默认值表达式在调用点求值（和 VM 的 defaults 同一来源）；
        // 没写默认值的补 Null。
        for i in aa.len().min(m.arity)..m.arity {
            match m.defaults.get(i).and_then(|d| d.clone()) {
                Some(d) => {
                    let dv = emit_expr(cg, &d)?;
                    call.push_str(&format!(", {}", dv));
                }
                None => call.push_str(", V::Null"),
            }
        }
        call.push(')');
        return Ok(format!("{}?", call));
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
    let cloned: Vec<String> = aa.iter().map(|a| format!("({}).clone()", a)).collect();
    Ok(format!(
        "dcall(g, &g.v[{}].clone(), vec![{}], {}, {})?",
        i,
        cloned.join(", "),
        e.span.line,
        e.span.col
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

const ALLOW: &str = "#![allow(dead_code, unused_imports, unused_variables, unused_mut, unused_parens, non_snake_case, unreachable_patterns, unused_assignments)]\n";

/// 注册表 + 跨块调用：分块模式下所有跨块调用都落到这里
const RT_REGISTRY: &str = r#"
pub type FnPtr = fn(&mut Globals, Vec<V>) -> R<V>;

thread_local! {
    static REG: std::cell::RefCell<Vec<(&'static str, usize, FnPtr)>> =
        std::cell::RefCell::new(Vec::new());
}

/// 每个块启动时把自己的函数表登记进来
pub fn register(t: &'static [(&'static str, usize, FnPtr)]) {
    REG.with(|r| r.borrow_mut().extend_from_slice(t));
}

/// 按名字调用：跨块调用、一等公民函数值都走这里；查不到就试内建函数
pub fn call_named(g: &mut Globals, name: &str, args: Vec<V>) -> Result<V, String> {
    let f = REG.with(|r| r.borrow().iter().rev().find(|(n, _, _)| *n == name).map(|(_, _, f)| *f));
    match f {
        Some(f) => f(g, args).map_err(|e| e.message),
        None => call_builtin(name, &args),
    }
}

/// 注册表里有没有这个名字（类方法分派用）
pub fn has_fn(name: &str) -> bool {
    REG.with(|r| r.borrow().iter().any(|(n, _, _)| *n == name))
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
        uses_mcall: false,
        uses_dcall: false,
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

/// 分块（增量）编译：返回整个 cargo workspace 的所有文件。
/// 调用方负责把文件写进构建目录（内容没变的不要重写，否则 cargo 缓存会失效）。
pub fn emit_split(prog: &Program, bin: &str, dep_path: &str) -> Result<Vec<SplitFile>, AotError> {
    if let Some(e) = prog.errors.first() {
        return Err(AotError::msg(format!("语法有错，先修语法：{}", e.message)));
    }

    let mut fns: Vec<(Rc<str>, Vec<crate::ast::Param>, Vec<Stmt>)> = Vec::new();
    scan_body(&prog.body, &mut fns)?;

    let mut meta: HashMap<Rc<str>, FnMeta> = HashMap::new();
    let mut methods: Vec<(String, String, usize)> = Vec::new();
    let mut classes: HashSet<Rc<str>> = HashSet::new();
    for (name, params, _) in &fns {
        meta.insert(
            Rc::clone(name),
            FnMeta { arity: params.len(), defaults: params.iter().map(|p| p.default.clone()).collect() },
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
        uses_mcall: false,
        uses_dcall: false,
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
            uses_mcall: false,
            uses_dcall: false,
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
                &meta, &globals, &gslots0, &int_globals, &gi_list0, &gi_slots0, &classes, &chunk_of,
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
    let mut flush = |cg: &mut Option<Cg>, code: &mut Vec<String>, app_top: &mut String, uses_mcall: &mut bool, uses_dcall: &mut bool| {
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
                        let mut c = make_top_cg(&meta, &globals, &gslots0, &int_globals, &gi_list0, &gi_slots0, &classes, &chunk_of);
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
                        cur = Some(make_top_cg(&meta, &globals, &gslots0, &int_globals, &gi_list0, &gi_slots0, &classes, &chunk_of));
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
                            make_top_cg(&meta, &globals, &gslots0, &int_globals, &gi_list0, &gi_slots0, &classes, &chunk_of)
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
    let body_code = String::new();

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
    for k in 0..nchunks {
        app.push_str(&format!("    register(copper_chunk{}::TABLE);\n", k));
    }
    app.push_str(&format!(
        "    let mut g = Globals {{ v: vec![V::Null; {}], gi: vec![0i64; {}] }};\n",
        globals.len(),
        gi_list0.len()
    ));
    if group_top {
        // 顶层语句按顺序跑：任意一块出错就整体停下（和单块 run_top 的语义一致）
        app.push_str("    let __r: R<()> = (|| -> R<()> {\n");
        for k in 0..init_code.len() {
            app.push_str(&format!("        copper_init{}::run(&mut g)?;\n", k));
        }
        app.push_str("        Ok(())\n    })();\n");
        app.push_str("    if let Err(e) = __r {\n        use std::io::Write as _; let _ = std::io::stdout().flush();\n        eprintln!(\"出错（第 {} 行）：{}\", e.span.line, e.message);\n        std::process::exit(1);\n    }\n}\n");
    } else {
        app.push_str("    match run_top(&mut g) {\n        Ok(_) => {}\n        Err(e) => {\n            use std::io::Write as _; let _ = std::io::stdout().flush();\n            eprintln!(\"出错（第 {} 行）：{}\", e.span.line, e.message);\n            std::process::exit(1);\n        }\n    }\n}\n");
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
            "[workspace]\nresolver = \"2\"\nmembers = [{}]\n\n[profile.release]\nopt-level = 3\nlto = false\ncodegen-units = 16\npanic = \"abort\"\nincremental = false\ndebug = false\nstrip = \"symbols\"\n",
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
        if name.starts_with("__") {
            continue; // 类内部函数不走动态调用
        }
        let fname = rust_fn_name(name);
        let mut params = String::new();
        params.push_str("g");
        for i in 0..m.arity {
            params.push_str(&format!(", args[{}].clone()", i));
        }
        s.push_str(&format!("        (\"{}\", {}) => {}({}),\n", name, m.arity, fname, params));
    }
    s.push_str("        _ => call_builtin(nm, &args).map_err(|m| fe(ln, cl, m)),\n");
    s.push_str("    }\n");
    s.push_str("}\n");
    s
}

/* ───────────────────────── 运行时前奏 ───────────────────────── */

const PRELUDE: &str = r#"// ── 运行时前奏：全部复用 float-lang 的实现，语义和 VM 完全一致 ──
pub use std::rc::Rc;
pub use float_lang::builtins::{call_builtin, str_method, list_method, dict_method};
pub use float_lang::value::Dict;
pub use float_lang::bytecode::Op;
pub use float_lang::vm::{arith, compare, compound_value, index_get, index_set, member_get, member_set};

thread_local! {
    pub static DEPTH: std::cell::Cell<u32> = std::cell::Cell::new(0);
}

/// 全局变量容器：v 放任意值，gi 放能证明是整数的变量（不碰 Value 枚举）
pub struct Globals {
    pub v: Vec<V>,
    pub gi: Vec<i64>,
}

pub struct DepthGuard;
impl Drop for DepthGuard {
    fn drop(&mut self) {
        DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

/// 进入一个 Copper 函数：递归 400 层上限，退栈时自动减回去
pub fn enter(fname: &str) -> R<DepthGuard> {
    if DEPTH.with(|d| d.get()) >= 400 {
        return Err(fe(
            0,
            0,
            format!("递归太深了（当前 400 层，上限 400 层），多半是 {} 的结束条件没写对", fname),
        ));
    }
    DEPTH.with(|d| d.set(d.get() + 1));
    Ok(DepthGuard)
}

pub type V = float_lang::Value;
pub type FE = float_lang::FloatError;
pub type R<T> = Result<T, FE>;

pub fn fe(line: u32, col: u32, msg: impl Into<String>) -> FE {
    FE::new(msg, float_lang::error::Span::new(&Rc::from("main.co"), line, col, col + 1))
}
pub fn t(v: &V) -> bool { v.truthy() }
pub fn disp(v: &V) -> String { v.to_display() }
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
pub fn want_int(v: &V) -> Result<i64, String> {
    match v {
        V::Int(n) => Ok(*n),
        V::Float(f) => Ok(*f as i64),
        V::Bool(b) => Ok(if *b { 1 } else { 0 }),
        other => Err(format!("这里要整数，得到 {}", other.type_name())),
    }
}
pub fn want_int_at(v: &V, ln: u32, cl: u32) -> R<i64> {
    want_int(v).map_err(|m| fe(ln, cl, m))
}
pub fn as_i64(v: &V, ln: u32, cl: u32) -> R<i64> {
    v.as_int().ok_or_else(|| fe(ln, cl, "这里要一个整数"))
}
// ── 算术快路径：整数直接算，绕开 VM 的通用 arith（这是 AOT 比解释器快的关键）──
#[inline(always)]
pub fn v_add(a: &V, b: &V) -> Result<V, String> {
    if let (V::Int(x), V::Int(y)) = (a, b) {
        if let Some(v) = x.checked_add(*y) { return Ok(V::Int(v)); }
    }
    arith(Op::Add, a, b)
}
#[inline(always)]
pub fn v_sub(a: &V, b: &V) -> Result<V, String> {
    if let (V::Int(x), V::Int(y)) = (a, b) {
        if let Some(v) = x.checked_sub(*y) { return Ok(V::Int(v)); }
    }
    arith(Op::Sub, a, b)
}
#[inline(always)]
pub fn v_mul(a: &V, b: &V) -> Result<V, String> {
    if let (V::Int(x), V::Int(y)) = (a, b) {
        if let Some(v) = x.checked_mul(*y) { return Ok(V::Int(v)); }
    }
    arith(Op::Mul, a, b)
}
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
pub fn neg(v: &V) -> Result<V, String> {
    match v {
        V::Int(n) => Ok(V::Int(-n)),
        V::Float(f) => Ok(V::Float(-f)),
        other => Err(format!("负号后面得是数字，得到 {}", other.to_display())),
    }
}
pub fn sand(a: impl FnOnce() -> R<V>, b: impl FnOnce() -> R<V>) -> R<V> {
    let l = a()?;
    if t(&l) { Ok(l) } else { b() }
}
pub fn sor(a: impl FnOnce() -> R<V>, b: impl FnOnce() -> R<V>) -> R<V> {
    let l = a()?;
    if t(&l) { Ok(l) } else { b() }
}
pub fn iter_one(v: &V) -> Result<Vec<V>, String> {
    match v {
        V::List(l) => Ok(l.borrow().clone()),
        V::Str(s) => Ok(s.chars().map(|c| V::string(c.to_string())).collect()),
        V::Dict(d) => { let d = d.borrow(); Ok(d.keys().iter().map(|k| V::string(k.to_string())).collect()) }
        other => Err(format!("{} 不能 for-in 迭代", other.to_display())),
    }
}
pub fn iter_pair(v: &V) -> Result<Vec<(V, V)>, String> {
    match v {
        V::Dict(d) => { let d = d.borrow(); Ok(d.entries().map(|(k, val)| (V::string(k.to_string()), val.clone())).collect()) }
        V::List(l) => { let l = l.borrow(); Ok(l.iter().enumerate().map(|(i, x)| (V::Int(i as i64), x.clone())).collect()) }
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
"#;
