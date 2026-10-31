//! 语义分析：编译之前先走一遍 AST，把"一看就不对"的地方挑出来。
//!
//! 编译器本身是宽容的（读未定义的变量给 null），这是脚本语言的脾气；
//! 但对写代码的人来说，静默给 null 最坑。所以这里单独做一层检查，
//! 结果给 IDE 的「问题」面板用，也可以给 `check` 命令行看。
//!
//! 检查项：
//! 1. 读了没定义的变量（警告：读出来是 null）
//! 2. 同一层重复定义（警告）
//! 3. 调用已知函数时参数个数不对（错误/警告）
//! 4. 定义了但从没用过（提示）
//! 5. return / break / continue 之后的死代码（提示）
//!
//! 这里只做静态能确定的判断，不猜类型——猜错比不猜更烦人。

use crate::ast::*;
use crate::builtins::is_builtin;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Hint,
}

#[derive(Debug, Clone)]
pub struct Diag {
    pub severity: Severity,
    pub message: String,
    pub line: u32,
    pub col: u32,
}

impl Diag {
    fn new(severity: Severity, message: impl Into<String>, line: u32, col: u32) -> Diag {
        Diag { severity, message: message.into(), line, col }
    }

    /// 给人看的一行（`第 3 行：……`）
    pub fn render(&self) -> String {
        let tag = match self.severity {
            Severity::Error => "错误",
            Severity::Warning => "警告",
            Severity::Hint => "提示",
        };
        format!("第 {} 行{:>4}：{}", self.line, tag, self.message)
    }
}

/// 一个作用域里声明过的东西
#[derive(Default, Clone)]
struct Scope {
    vars: HashMap<Rc<str>, u32>,
    used: HashSet<Rc<str>>,
}

/// 已知函数（用来查参数个数）
struct FnSig {
    params: Vec<(Rc<str>, bool)>, // (名字, 有没有默认值)
}

struct Checker {
    diags: Vec<Diag>,
    globals: Scope,
    locals: Vec<Scope>,
    funcs: HashMap<Rc<str>, FnSig>,
}

const SPECIALS: [&str; 4] = ["ans", "__last_ans", "__last_read", "__last_cmp"];

/// 检查整段程序
pub fn analyze(prog: &Program) -> Vec<Diag> {
    let mut c = Checker {
        diags: Vec::new(),
        globals: Scope::default(),
        locals: Vec::new(),
        funcs: HashMap::new(),
    };
    // 先把所有函数签名收进来，函数可以互相调用（包括先用后定义）
    for st in &prog.body {
        if let StmtKind::FuncDef { name, params, .. } = &st.kind {
            c.funcs.insert(
                Rc::clone(name),
                FnSig {
                    params: params.iter().map(|p| (Rc::clone(&p.name), p.default.is_some())).collect(),
                },
            );
        }
    }
    let body = prog.body.clone();
    for st in &body {
        c.stmt(st);
    }
    // 全局里没被用过的
    c.report_unused(&c.globals.clone());
    c.diags.sort_by_key(|d| (d.line, d.col));
    c.diags
}

impl Checker {
    fn err(&mut self, msg: impl Into<String>, line: u32, col: u32) {
        self.diags.push(Diag::new(Severity::Error, msg, line, col));
    }
    fn warn(&mut self, msg: impl Into<String>, line: u32, col: u32) {
        self.diags.push(Diag::new(Severity::Warning, msg, line, col));
    }
    fn hint(&mut self, msg: impl Into<String>, line: u32, col: u32) {
        self.diags.push(Diag::new(Severity::Hint, msg, line, col));
    }

    fn scope(&mut self) -> &mut Scope {
        if self.locals.is_empty() {
            &mut self.globals
        } else {
            self.locals.last_mut().unwrap()
        }
    }

    fn declare(&mut self, name: &Rc<str>, line: u32, col: u32) {
        let existed = self.scope().vars.contains_key(name);
        if existed {
            self.warn(format!("「{}」在同一个作用域里定义了两次，后面那次会覆盖前面", name), line, col);
        }
        self.scope().vars.insert(Rc::clone(name), line);
    }

    fn is_defined(&self, name: &str) -> bool {
        if self.locals.iter().any(|s| s.vars.contains_key(name)) {
            return true;
        }
        if self.globals.vars.contains_key(name) {
            return true;
        }
        is_builtin(name) || SPECIALS.contains(&name)
    }

    fn mark_used(&mut self, name: &str) {
        for s in self.locals.iter_mut().rev() {
            if s.vars.contains_key(name) {
                s.used.insert(Rc::from(name));
                return;
            }
        }
        if self.globals.vars.contains_key(name) {
            self.globals.used.insert(Rc::from(name));
        }
    }

    fn report_unused(&mut self, scope: &Scope) {
        let mut items: Vec<(&Rc<str>, u32)> = scope
            .vars
            .iter()
            .filter(|(k, _)| !scope.used.contains(*k) && !k.starts_with('$'))
            .map(|(k, v)| (k, *v))
            .collect();
        items.sort_by_key(|(_, line)| *line);
        for (name, line) in items {
            self.hint(format!("「{}」定义了但一直没用过", name), line, 1);
        }
    }

    /* ───────────── 语句 ───────────── */

    fn stmt(&mut self, s: &Stmt) {
        let (line, col) = (s.span.line, s.span.col);
        match &s.kind {
            StmtKind::Decl { name, init, .. } => {
                if let Some(e) = init {
                    self.expr(e);
                } else {
                    self.declare(name, line, col);
                    return;
                }
                self.declare(name, line, col);
            }
            StmtKind::Assign { target, value, .. } => {
                self.target_write(target);
                self.expr(value);
            }
            StmtKind::IncDec { target, .. } => {
                self.target_write(target);
                // 自增也算"读 + 写"
                self.target_read(target);
            }
            StmtKind::ExprStmt(e) => self.expr(e),
            StmtKind::Print(args) | StmtKind::Report(args) => {
                for a in args {
                    self.expr(a);
                }
            }
            StmtKind::Ask(prompt) => {
                if let Some(e) = prompt {
                    self.expr(e);
                }
            }
            StmtKind::If { branches, else_body } => {
                for (cond, body) in branches {
                    if let Some(c) = cond {
                        self.expr(c);
                    }
                    self.block(body);
                }
                if let Some(eb) = else_body {
                    self.block(eb);
                }
            }
            StmtKind::While { cond, body } => {
                self.expr(cond);
                self.block(body);
            }
            StmtKind::Repeat { count, body } => {
                self.expr(count);
                self.block(body);
            }
            StmtKind::ForRange { var, from, to, step, body, .. } => {
                if let Some(f) = from {
                    self.expr(f);
                }
                self.expr(to);
                if let Some(st) = step {
                    self.expr(st);
                }
                self.declare(var, line, col);
                self.block(body);
            }
            StmtKind::ForIn { var, value_name, iter, body } => {
                self.expr(iter);
                self.declare(var, line, col);
                if let Some(v) = value_name {
                    self.declare(v, line, col);
                }
                self.block(body);
            }
            StmtKind::Destructure { names, value } => {
                self.expr(value);
                for n in names {
                    self.declare(n, line, col);
                }
            }
            StmtKind::Break | StmtKind::Continue | StmtKind::Stop => {}
            StmtKind::Return(v) => {
                if let Some(e) = v {
                    self.expr(e);
                }
            }
            StmtKind::FuncDef { name, params, body, .. } => {
                // 函数名本身也是一个值（存在同名变量里），先登记
                self.declare(name, line, col);
                self.locals.push(Scope::default());
                for p in params {
                    let name = Rc::clone(&p.name);
                    self.locals.last_mut().unwrap().vars.insert(name, p.line);
                    if let Some(d) = &p.default {
                        self.expr(d);
                    }
                }
                let before = self.diags.len();
                // 函数体：逐条走，遇到 return/break/continue 后面的语句提醒一次
                self.block_with_dead_code(body);
                let _ = before;
                let sc = self.locals.pop().unwrap();
                self.report_unused(&sc);
            }
            StmtKind::Nop => {}
            StmtKind::Unsupported { what } => {
                self.warn(format!("{}", what), line, col);
            }
        }
    }

    /// 块里一个变量都不新开作用域（和编译器一致：只有函数边界才是新作用域）
    fn block(&mut self, body: &[Stmt]) {
        for st in body {
            self.stmt(st);
        }
    }

    fn block_with_dead_code(&mut self, body: &[Stmt]) {
        let mut dead = false;
        for st in body {
            if dead {
                self.hint("这行代码永远执行不到（前面已经 return / break / continue 了）", st.span.line, st.span.col);
                dead = false; // 只提醒第一行，别刷屏
            }
            self.stmt(st);
            if matches!(
                st.kind,
                StmtKind::Return(_) | StmtKind::Break | StmtKind::Continue | StmtKind::Stop
            ) {
                dead = true;
            }
        }
    }

    /* ───────────── 表达式 ───────────── */

    fn target_write(&mut self, t: &Target) {
        match t {
            Target::Var(name) => {
                // 直接赋值给没定义的名字是允许的（等于新建全局），不报
                if !self.is_defined(name) {
                    self.declare(name, 0, 1);
                }
            }
            Target::Member(obj, _) => self.expr(obj),
            Target::Idx(obj, idx) => {
                self.expr(obj);
                self.expr(idx);
            }
        }
    }

    fn target_read(&mut self, t: &Target) {
        match t {
            Target::Var(name) => {
                if !self.is_defined(name) {
                    self.warn(format!("「{}」还没定义过，读出来会是 null", name), 0, 1);
                } else {
                    self.mark_used(name);
                }
            }
            Target::Member(obj, _) => self.expr(obj),
            Target::Idx(obj, idx) => {
                self.expr(obj);
                self.expr(idx);
            }
        }
    }

    fn expr(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Num(_) | ExprKind::Str(_) | ExprKind::Bool(_) | ExprKind::Null => {}
            ExprKind::Var(name) => {
                if self.is_defined(name) {
                    self.mark_used(name);
                } else {
                    self.warn(
                        format!("「{}」还没定义过，读出来会是 null", name),
                        e.span.line,
                        e.span.col,
                    );
                }
            }
            ExprKind::SelfRef => {}
            ExprKind::List(items) => {
                for it in items {
                    self.expr(it);
                }
            }
            ExprKind::Dict(entries) => {
                for (k, v) in entries {
                    self.expr(k);
                    self.expr(v);
                }
            }
            ExprKind::Un { a, .. } => self.expr(a),
            ExprKind::Bin { a, b, .. } => {
                self.expr(a);
                self.expr(b);
            }
            ExprKind::Idx { obj, index } => {
                self.expr(obj);
                self.expr(index);
            }
            ExprKind::Member { obj, .. } => self.expr(obj),
            ExprKind::Call { callee, args } => {
                // 先查参数个数（对已知函数）
                if let ExprKind::Var(name) = &callee.kind {
                    if let Some(sig) = self.funcs.get(name) {
                        let total = sig.params.len();
                        let required = sig.params.iter().filter(|(_, d)| !d).count();
                        let given = args.len();
                        if given > total {
                            let msg = format!(
                                "{} 只收 {} 个参数，这里给了 {} 个",
                                name, total, given
                            );
                            self.err(msg, e.span.line, e.span.col);
                        } else if given < required {
                            let msg = format!(
                                "{} 至少需要 {} 个参数，这里只给了 {} 个",
                                name, required, given
                            );
                            self.err(msg, e.span.line, e.span.col);
                        }
                    }
                }
                self.expr(callee);
                for a in args {
                    self.expr(a);
                }
            }
            ExprKind::New { args, .. } => {
                for a in args {
                    self.expr(a);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser;

    fn diags(src: &str) -> Vec<Diag> {
        let prog = parser::parse_program("t.cw", src);
        analyze(&prog)
    }

    fn has(ds: &[Diag], sev: Severity, needle: &str) -> bool {
        ds.iter().any(|d| d.severity == sev && d.message.contains(needle))
    }

    #[test]
    fn 读出未定义变量会提醒() {
        let ds = diags("print(x)\n");
        assert!(has(&ds, Severity::Warning, "还没定义过"), "{:?}", ds);
    }

    #[test]
    fn 先声明后使用不报() {
        let ds = diags("int x = 1\nprint(x)\n");
        assert!(!has(&ds, Severity::Warning, "x"), "{:?}", ds);
    }

    #[test]
    fn 重复定义会提醒() {
        let ds = diags("int a = 1\nint a = 2\nprint(a)\n");
        assert!(has(&ds, Severity::Warning, "定义了两次"), "{:?}", ds);
    }

    #[test]
    fn 参数给多了报错() {
        let src = "fn f(a)\n    return a\nprint(f(1, 2))\n";
        let ds = diags(src);
        assert!(has(&ds, Severity::Error, "只收 1 个参数"), "{:?}", ds);
    }

    #[test]
    fn 参数给少了报错() {
        let src = "fn f(a, b)\n    return a\nprint(f(1))\n";
        let ds = diags(src);
        assert!(has(&ds, Severity::Error, "至少需要 2 个"), "{:?}", ds);
    }

    #[test]
    fn 有默认值的参数可以省略() {
        let src = "fn f(a, b = 2)\n    return a\nprint(f(1))\n";
        let ds = diags(src);
        assert!(!has(&ds, Severity::Error, "参数"), "{:?}", ds);
    }

    #[test]
    fn 函数里的参数和局部变量认得() {
        let src = "fn f(n)\n    int s = 0\n    for i in range(n)\n        s += i\n    return s\nprint(f(3))\n";
        let ds = diags(src);
        let bad: Vec<&Diag> = ds.iter().filter(|d| d.severity == Severity::Warning).collect();
        assert!(bad.is_empty(), "{:?}", bad);
    }

    #[test]
    fn return_之后是死代码会被提一句() {
        let src = "fn f()\n    return 1\n    print(2)\nprint(f())\n";
        let ds = diags(src);
        assert!(has(&ds, Severity::Hint, "永远执行不到"), "{:?}", ds);
    }

    #[test]
    fn 定义了没用会提示() {
        let ds = diags("int unused = 1\nprint(2)\n");
        assert!(has(&ds, Severity::Hint, "没用过"), "{:?}", ds);
    }

    #[test]
    fn 内建函数和特殊变量不算未定义() {
        let ds = diags("ask \"名字\"\nprint(ans)\nprint(len(\"ab\"))\n");
        assert!(!has(&ds, Severity::Warning, "还没定义过"), "{:?}", ds);
    }
}
