//! 字节码与编译器。
//!
//! 为什么不做树遍历：TS 版解释器每条语句都要走一遍函数调用 + 状态机，fib(30) 要 35 秒。
//! 这里先把 AST 编成紧凑字节码，再让 VM 用一个 `match` 循环执行——分支预测友好，
//! 也为将来做内联缓存（inline cache）和类型特化留了位置。
//!
//! 指令集设计取向：
//! * 栈式（stack machine）：实现简单、正确性容易验证，先赢在"能跑 + 够快"
//! * 变量在编译期就解析成槽位号（local / global），运行时不做名字查找
//! * `for i in range(n)` 在语法层就特化成计数循环，不建列表

use crate::ast::Op as AOp;
use crate::ast::*;
use crate::error::{FloatError, Span};
use crate::value::{UserFn, Value};
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    Const(u32),
    Nil,
    True,
    False,
    Pop,
    Dup,

    GetLocal(u16),
    SetLocal(u16),
    GetGlobal(u16),
    SetGlobal(u16),

    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Neg,
    Not,
    /// 按位与 / 或 / 异或 / 左右移（先按 i64 走，够用）
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    BitNot,

    Jump(u32),
    /// 弹栈，假则跳
    JumpIfFalse(u32),
    /// 看栈顶，假则跳（不弹）—— && 用
    JumpIfFalseKeep(u32),
    /// 看栈顶，真则跳（不弹）—— || 用
    JumpIfTrueKeep(u32),

    /* ── 计数循环：一条指令跑完一圈的维护工作 ── */
    /// `for i in range(...)` 的初始化：弹掉栈上的 [起, 止, 步长]，压一个循环状态
    RangeInit { var: u16, is_global: bool, inclusive: bool },
    /// 每圈一次：测当前值 → 写循环变量 → 步进；测不过就收掉状态跳 fail
    RangeNext { fail: u32 },
    /// break 提前退出时收掉循环状态
    RangeEnd,

    /* ── 融合的变量更新：读-算-写一步到位 ── */
    /// dst op= src（两边都是变量）。bop：0 加 1 减 2 乘 3 除 4 取余
    CompoundVar { bop: u8, dst: u16, src: u16, flags: u8 },
    /// dst op= 常量
    CompoundVarConst { bop: u8, dst: u16, flags: u8, k: u32 },
    /// 自增自减
    IncVar { slot: u16, flags: u8, delta: i8 },
    /// 实参个数 ≥ n 就跳过（用来给"默认值"打桩：没传才执行默认表达式）
    JumpIfArgc { n: u8, target: u32 },
    /// 变量 op 变量 → 压结果（两个操作数都在槽里，不用来回压栈）
    BinLocalLocal { op: u8, a: u16, aflags: u8, b: u16, bflags: u8 },
    /// 变量 op 常量 → 压结果。op：0 加 1 减 2 乘 3 除 4 取余 5 幂
    /// 6 相等 7 不等 8 小于 9 小于等于 10 大于 11 大于等于
    /// `n - 1` / `i < 10` 这类到处都是的写法压成一条指令
    BinLocalK { op: u8, slot: u16, flags: u8, k: u32 },
    /// 直接返回一个变量：省掉 GetLocal + Return 两条
    ReturnLocal { slot: u16, flags: u8 },
    /// 调用全局槽里的函数：省掉 GetGlobal + Call，而且参数不用挪位
    CallGlobal { slot: u16, nargs: u8 },

    Call(u8),
    /// obj.name(args…)，栈布局：[obj, arg1..argN]
    CallMethod(u32, u8),
    Return,

    BuildList(u16),
    BuildDict(u16),
    Index,
    IndexSet,
    Member(u32),
    MemberSet(u32),

    Print(u16),
    /// 读一行输入，压栈（没有输入时给空串）
    Ask,

    IterInit,
    /// 迭代器取下一个：取到压栈，取完跳到目标
    IterNext(u32),
    IterEnd,
}

#[derive(Clone, Debug)]
pub struct Chunk {
    pub code: Vec<Op>,
    /// 和 code 一一对应的源码行号：报错能指到真正那一行
    pub lines: Vec<u32>,
    pub consts: Vec<Value>,
    /// 成员名 / 方法名内联表
    pub names: Vec<Rc<str>>,
    /// 函数名表：报错的调用链靠它取名字（帧里只存编号）
    pub fn_names: Vec<Rc<str>>,
    pub n_globals: u16,
    pub file: Rc<str>,
}

impl Chunk {
    pub fn const_at(&self, i: u32) -> Value {
        self.consts.get(i as usize).cloned().unwrap_or(Value::Null)
    }

    pub fn name_at(&self, i: u32) -> Rc<str> {
        self.names
            .get(i as usize)
            .cloned()
            .unwrap_or_else(|| Rc::from("?"))
    }
}

#[derive(Clone, Copy)]
enum Slot {
    Local(u16),
    Global(u16),
}

struct Scope {
    vars: HashMap<Rc<str>, u16>,
    next: u16,
}

impl Scope {
    fn new() -> Scope {
        Scope { vars: HashMap::new(), next: 0 }
    }
    fn declare(&mut self, name: &Rc<str>) -> u16 {
        if let Some(s) = self.vars.get(name) {
            return *s;
        }
        let s = self.next;
        self.next += 1;
        self.vars.insert(Rc::clone(name), s);
        s
    }
    fn find(&self, name: &str) -> Option<u16> {
        self.vars.get(name).copied()
    }
}

struct LoopCtx {
    iter_depth: usize,
    /// 进这层循环时计数循环的深度（break 要按层数收状态）
    range_depth: usize,
    breaks: Vec<usize>,
    continues: Vec<usize>,
}

pub struct Compiler {
    code: Vec<Op>,
    lines: Vec<u32>,
    consts: Vec<Value>,
    names: Vec<Rc<str>>,
    scopes: Vec<Scope>,
    file: Rc<str>,
    /// 当前正在编译的语句行号（emit 时记进 lines）
    line: u32,
    iters: usize,
    /// 当前嵌了几层计数循环（break 时按层收状态）
    ranges: usize,
    loops: Vec<LoopCtx>,
    pub errors: Vec<FloatError>,
    /// 编译产出：函数体登记在这里，靠 entry 地址回到字节码
    funcs: Vec<Rc<UserFn>>,
    fn_names: Vec<Rc<str>>,
    temp_seq: usize,
}

impl Compiler {
    pub fn new(file: Rc<str>) -> Compiler {
        Compiler {
            code: Vec::new(),
            lines: Vec::new(),
            consts: Vec::new(),
            names: Vec::new(),
            scopes: vec![Scope::new()],
            file,
            line: 0,
            iters: 0,
            ranges: 0,
            loops: Vec::new(),
            errors: Vec::new(),
            funcs: Vec::new(),
            fn_names: Vec::new(),
            temp_seq: 0,
        }
    }

    fn err(&mut self, msg: impl Into<String>, span: &Span) {
        self.errors.push(FloatError::new(msg, span.clone()));
    }

    fn emit(&mut self, op: Op) -> usize {
        self.code.push(op);
        self.lines.push(self.line);
        self.code.len() - 1
    }

    fn emit_const(&mut self, v: Value) -> usize {
        let idx = self.konst(v);
        self.emit(Op::Const(idx))
    }

    fn here(&self) -> usize {
        self.code.len()
    }

    fn patch(&mut self, at: usize) {
        let target = self.code.len() as u32;
        self.set_jump(at, target);
    }

    fn patch_to(&mut self, at: usize, target: usize) {
        self.set_jump(at, target as u32);
    }

    fn set_jump(&mut self, at: usize, t: u32) {
        match &mut self.code[at] {
            Op::Jump(x)
            | Op::JumpIfFalse(x)
            | Op::JumpIfFalseKeep(x)
            | Op::JumpIfTrueKeep(x)
            | Op::IterNext(x) => *x = t,
            Op::RangeNext { fail } => *fail = t,
            Op::JumpIfArgc { target, .. } => *target = t,
            other => panic!("patch 打在了非跳转指令上：{:?}", other),
        }
    }

    fn konst(&mut self, v: Value) -> u32 {
        let idx = self.consts.len() as u32;
        self.consts.push(v);
        idx
    }

    fn intern(&mut self, name: &Rc<str>) -> u32 {
        if let Some(i) = self.names.iter().position(|n| n == name) {
            return i as u32;
        }
        self.names.push(Rc::clone(name));
        (self.names.len() - 1) as u32
    }

    fn in_function(&self) -> bool {
        self.scopes.len() > 1
    }

    fn lookup(&self, name: &str) -> Option<Slot> {
        if self.in_function() {
            if let Some(s) = self.scopes.last().and_then(|sc| sc.find(name)) {
                return Some(Slot::Local(s));
            }
            if let Some(s) = self.scopes[0].find(name) {
                return Some(Slot::Global(s));
            }
            return None;
        }
        self.scopes[0].find(name).map(Slot::Global)
    }

    fn resolve(&mut self, name: &Rc<str>) -> Slot {
        if self.in_function() {
            if let Some(s) = self.scopes.last().and_then(|s| s.find(name)) {
                return Slot::Local(s);
            }
            if let Some(s) = self.scopes[0].find(name) {
                return Slot::Global(s);
            }
            // 函数里引用一个还没声明的名字：当成全局，运行时是 null
            let s = self.scopes[0].declare(name);
            return Slot::Global(s);
        }
        let s = self.scopes[0].declare(name);
        Slot::Global(s)
    }

    fn declare_local(&mut self, name: &Rc<str>) -> Slot {
        if self.in_function() {
            let s = self.scopes.last_mut().unwrap().declare(name);
            Slot::Local(s)
        } else {
            let s = self.scopes[0].declare(name);
            Slot::Global(s)
        }
    }

    /// ans / __last_ans 这类运行时会写、脚本里直接读的"特殊变量"。
    /// 它们一定在全局作用域，函数里也能看到。
    fn store_special(&mut self, name: &str) {
        let n: Rc<str> = Rc::from(name);
        let s = self.scopes[0].declare(&n);
        self.emit(Op::SetGlobal(s));
    }

    fn new_temp(&mut self) -> Slot {
        self.temp_seq += 1;
        let name: Rc<str> = Rc::from(format!("$t{}", self.temp_seq).as_str());
        self.declare_local(&name)
    }

    pub fn compile_program(&mut self, prog: &Program) -> Chunk {
        for s in &prog.body {
            self.stmt(s);
        }
        self.emit(Op::Nil);
        self.emit(Op::Return);
        Chunk {
            code: std::mem::take(&mut self.code),
            lines: std::mem::take(&mut self.lines),
            consts: std::mem::take(&mut self.consts),
            names: std::mem::take(&mut self.names),
            fn_names: std::mem::take(&mut self.fn_names),
            n_globals: self.scopes[0].next,
            file: Rc::clone(&self.file),
        }
    }

    /* ───────────────────────── 语句 ───────────────────────── */

    fn stmt(&mut self, s: &Stmt) {
        if s.span.line > 0 {
            self.line = s.span.line;
        }
        match &s.kind {
            StmtKind::Nop => {}
            StmtKind::Decl { name, init, .. } => {
                let slot = self.declare_local(name);
                match init {
                    Some(e) => self.expr(e),
                    None => {
                        self.emit(Op::Nil);
                    }
                }
                self.store(slot);
            }
            StmtKind::Assign { target, op, value } => {
                if *op == AssignOp::Set {
                    self.push_ref(target);
                    self.expr(value);
                    self.emit_set(target);
                } else {
                    let bop = binop_to_op(op.to_op().unwrap());
                    // 融合快路径：`x op= y` / `x op= 常量` 一条指令搞定，
                    // 省掉"读变量→算→写变量"三条指令来回倒栈
                    if let (Target::Var(name), Some(cbop)) = (target, compound_bop(op.to_op().unwrap())) {
                        let slot = self.resolve(name);
                        let (is_global, dst) = slot_parts(slot);
                        let flags = is_global as u8;

                        if let ExprKind::Var(src_name) = &value.kind {
                            if let Some(sslot) = self.lookup(src_name) {
                                let (sg, si) = slot_parts(sslot);
                                self.emit(Op::CompoundVar {
                                    bop: cbop,
                                    dst,
                                    src: si,
                                    flags: flags | ((sg as u8) << 1),
                                });
                                return;
                            }
                        }
                        if let Some(v) = literal_of(value) {
                            let k = self.konst(v);
                            self.emit(Op::CompoundVarConst { bop: cbop, dst, flags, k });
                            return;
                        }
                    }
                    // 变量目标走快路径：读 → 算 → 写，不占用临时槽
                    if let Target::Var(name) = target {
                        let slot = self.resolve(name);
                        self.load(slot);
                        self.expr(value);
                        self.emit(bop);
                        self.store(slot);
                    } else {
                        let temp = self.new_temp();
                        self.load_target(target);
                        self.expr(value);
                        self.emit(bop);
                        self.store(temp);
                        self.push_ref(target);
                        self.load(temp);
                        self.emit_set(target);
                    }
                }
            }
            StmtKind::IncDec { target, delta, .. } => {
                if let Target::Var(name) = target {
                    let slot = self.resolve(name);
                    let (is_global, idx) = slot_parts(slot);
                    self.emit(Op::IncVar { slot: idx, flags: is_global as u8, delta: *delta as i8 });
                } else {
                    let bop = if *delta > 0 { Op::Add } else { Op::Sub };
                    let temp = self.new_temp();
                    self.load_target(target);
                    self.emit_const(Value::Int(1));
                    self.emit(bop);
                    self.store(temp);
                    self.push_ref(target);
                    self.load(temp);
                    self.emit_set(target);
                }
            }
            StmtKind::ExprStmt(e) => {
                self.expr(e);
                self.emit(Op::Pop);
            }
            StmtKind::Print(args) => {
                for a in args {
                    self.expr(a);
                }
                self.emit(Op::Print(args.len() as u16));
            }
            StmtKind::Ask(prompt) => {
                // 有提示就先打出来（控制台风格），再读一行写进 ans / __last_ans
                if let Some(e) = prompt {
                    self.expr(e);
                    self.emit(Op::Print(1));
                }
                self.emit(Op::Ask);
                self.emit(Op::Dup);
                self.store_special("__last_ans");
                self.store_special("ans");
            }
            StmtKind::If { branches, else_body } => {
                let mut end_jumps: Vec<usize> = Vec::new();
                for (cond, body) in branches {
                    match cond {
                        Some(c) => {
                            self.expr(c);
                            let jf = self.emit(Op::JumpIfFalse(0));
                            for st in body {
                                self.stmt(st);
                            }
                            end_jumps.push(self.emit(Op::Jump(0)));
                            self.patch(jf);
                        }
                        None => {
                            for st in body {
                                self.stmt(st);
                            }
                            end_jumps.push(self.emit(Op::Jump(0)));
                        }
                    }
                }
                if let Some(eb) = else_body {
                    for st in eb {
                        self.stmt(st);
                    }
                }
                for j in end_jumps {
                    self.patch(j);
                }
            }
            StmtKind::While { cond, body } => {
                let start = self.here();
                self.expr(cond);
                let jf = self.emit(Op::JumpIfFalse(0));
                self.loops.push(LoopCtx { iter_depth: self.iters, range_depth: self.ranges, breaks: vec![], continues: vec![] });
                for st in body {
                    self.stmt(st);
                }
                let back = self.emit(Op::Jump(0));
                self.patch_to(back, start);
                let ctx = self.loops.pop().unwrap();
                let after = self.here();
                self.patch_to(jf, after);
                for b in ctx.breaks {
                    self.patch_to(b, after);
                }
                for c in ctx.continues {
                    self.patch_to(c, start);
                }
            }
            StmtKind::Repeat { count, body } => {
                // repeat n → 先算 n 存进隐藏槽，循环里递减
                let temp = self.new_temp();
                self.expr(count);
                self.store(temp);
                let start = self.here();
                self.load(temp);
                self.emit_const(Value::Int(0));
                self.emit(Op::Gt);
                let jf = self.emit(Op::JumpIfFalse(0));
                self.loops.push(LoopCtx { iter_depth: self.iters, range_depth: self.ranges, breaks: vec![], continues: vec![] });
                for st in body {
                    self.stmt(st);
                }
                self.load(temp);
                self.emit_const(Value::Int(1));
                self.emit(Op::Sub);
                self.store(temp);
                let back = self.emit(Op::Jump(0));
                self.patch_to(back, start);
                let ctx = self.loops.pop().unwrap();
                let after = self.here();
                self.patch_to(jf, after);
                for b in ctx.breaks {
                    self.patch_to(b, after);
                }
                for c in ctx.continues {
                    self.patch_to(c, start);
                }
            }
            StmtKind::ForRange { var, from, to, step, inclusive, body } => {
                let slot = self.declare_local(var);
                let (is_global, var_idx) = slot_parts(slot);

                // 栈上依次压：起点、终点、步长，RangeInit 一次收走
                match from {
                    Some(f) => self.expr(f),
                    None => {
                        self.emit_const(Value::Int(0));
                    }
                }
                self.expr(to);
                match step {
                    Some(st) => self.expr(st),
                    None => {
                        self.emit_const(Value::Int(1));
                    }
                }
                self.emit(Op::RangeInit { var: var_idx, is_global, inclusive: *inclusive });

                // 循环入口就是 RangeNext：测值 → 写循环变量 → 步进，一条指令一圈。
                // 首圈也要过这道检查，所以 RangeInit 不直接跳进循环体。
                let start = self.here();
                let next = self.emit(Op::RangeNext { fail: 0 });

                self.ranges += 1;
                self.loops.push(LoopCtx {
                    iter_depth: self.iters,
                    range_depth: self.ranges - 1,
                    breaks: vec![],
                    continues: vec![],
                });
                for st in body {
                    self.stmt(st);
                }
                let back = self.emit(Op::Jump(0));
                self.patch_to(back, start);

                let ctx = self.loops.pop().unwrap();
                self.ranges -= 1;

                // 正常运行结束：RangeNext 已经自己收掉状态
                let after = self.here();
                self.patch_to(next, after);
                for b in ctx.breaks {
                    self.patch_to(b, after);
                }
                for c in ctx.continues {
                    self.patch_to(c, start);
                }
            }
            StmtKind::ForIn { var, value_name, iter, body } => {
                let slot = self.declare_local(var);
                let vslot = value_name.as_ref().map(|n| self.declare_local(n));
                // for k, v in dict 需要回头拿 v：先留一份原集合
                let src = if vslot.is_some() { Some(self.new_temp()) } else { None };
                self.expr(iter);
                if let Some(s) = src {
                    self.emit(Op::Dup);
                    self.store(s);
                }
                self.emit(Op::IterInit);
                self.iters += 1;

                let start = self.here();
                let next_jump = self.emit(Op::IterNext(0));
                self.store(slot);
                if let (Some(vs), Some(s)) = (vslot, src) {
                    // 栈顺序：obj 在前，index 在后
                    self.load(s);
                    self.load(slot);
                    self.emit(Op::Index);
                    self.store(vs);
                }
                self.loops.push(LoopCtx { iter_depth: self.iters - 1, range_depth: self.ranges, breaks: vec![], continues: vec![] });
                for st in body {
                    self.stmt(st);
                }
                let back = self.emit(Op::Jump(0));
                self.patch_to(back, start);

                let ctx = self.loops.pop().unwrap();
                // 迭代器取完的落点：先收掉迭代器，再往下走
                let iter_end_label = self.here();
                self.patch_to(next_jump, iter_end_label);
                self.emit(Op::IterEnd);
                self.iters -= 1;
                let done = self.here();
                // break 跳到这里（迭代器已经在 break 前收掉）
                for b in ctx.breaks {
                    self.patch_to(b, done);
                }
                for c in ctx.continues {
                    self.patch_to(c, start);
                }
            }
            StmtKind::Break => {
                let need = self.iters - self.loops.last().map(|l| l.iter_depth).unwrap_or(self.iters);
                for _ in 0..need {
                    self.emit(Op::IterEnd);
                }
                // 跳出计数循环时，把已经压进去的循环状态也收掉
                let need_ranges = self.ranges - self.loops.last().map(|l| l.range_depth).unwrap_or(self.ranges);
                for _ in 0..need_ranges {
                    self.emit(Op::RangeEnd);
                }
                let j = self.emit(Op::Jump(0));
                if let Some(ctx) = self.loops.last_mut() {
                    ctx.breaks.push(j);
                } else {
                    self.err("break 不在循环里", &s.span);
                }
            }
            StmtKind::Continue => {
                // continue 只回到本圈开头，循环状态必须保留，不能弹
                let need = self.iters - self.loops.last().map(|l| l.iter_depth).unwrap_or(self.iters);
                for _ in 0..need {
                    self.emit(Op::IterEnd);
                }
                let j = self.emit(Op::Jump(0));
                if let Some(ctx) = self.loops.last_mut() {
                    ctx.continues.push(j);
                } else {
                    self.err("continue 不在循环里", &s.span);
                }
            }
            StmtKind::Stop => {
                self.emit(Op::Nil);
                self.emit(Op::Return);
            }
            StmtKind::Return(v) => {
                // `return n` 这种最常见：一条指令直接返回
                if let Some(e) = v {
                    if let ExprKind::Var(name) = &e.kind {
                        if let Some(slot) = self.lookup(name) {
                            let (is_global, idx) = slot_parts(slot);
                            self.emit(Op::ReturnLocal { slot: idx, flags: is_global as u8 });
                            return;
                        }
                    }
                }
                match v {
                    Some(e) => self.expr(e),
                    None => {
                        self.emit(Op::Nil);
                    }
                }
                self.emit(Op::Return);
            }
            StmtKind::FuncDef { name, params, body, .. } => self.func_def(name, params, body, &s.span),
            StmtKind::Destructure { names, value } => {
                let temp = self.new_temp();
                self.expr(value);
                self.store(temp);
                for (i, n) in names.iter().enumerate() {
                    let slot = self.declare_local(n);
                    self.load(temp);
                    self.emit_const(Value::Int(i as i64));
                    self.emit(Op::Index);
                    self.store(slot);
                }
            }
            StmtKind::Unsupported { what } => {
                self.err(format!("{}", what), &s.span);
            }
        }
    }

    fn func_def(&mut self, name: &Rc<str>, params: &[Param], body: &[Stmt], span: &Span) {
        // 入口地址先占一个 Jump，跳过函数体
        let skip = self.emit(Op::Jump(0));
        let entry = self.here();

        self.scopes.push(Scope::new());
        for p in params {
            let _ = self.scopes.last_mut().unwrap().declare(&p.name);
        }
        // 默认值在**调用时**算，而不是编译期算常量：
        // 这样 `fn area(w, h = w)` 这种"默认值引用前面参数"才成立。
        for (k, p) in params.iter().enumerate() {
            if let Some(d) = &p.default {
                let j = self.emit(Op::JumpIfArgc { n: (k + 1) as u8, target: 0 });
                self.expr(d);
                let slot = self.scopes.last().and_then(|sc| sc.find(&p.name));
                if let Some(slot) = slot {
                    self.emit(Op::SetLocal(slot));
                }
                self.patch(j);
            }
        }
        for st in body {
            self.stmt(st);
        }
        self.emit(Op::Nil);
        self.emit(Op::Return);
        let nlocals = self.scopes.last().unwrap().next;
        self.scopes.pop();
        self.patch(skip);

        let id = self.fn_names.len() as u16;
        self.fn_names.push(Rc::clone(name));
        let f = Rc::new(UserFn {
            name: Rc::clone(name),
            id,
            params: params.to_vec(),
            body: Rc::new(body.to_vec()),
            file: Rc::clone(&self.file),
            entry,
            nlocals: nlocals as usize,
            defaults: params.iter().map(|p| const_eval(p.default.as_ref())).collect(),
        });
        self.funcs.push(Rc::clone(&f));

        // 函数名作为常量存进去，赋值给同名变量
        let slot = self.declare_local(name);
        self.emit_const(Value::Func(f));
        self.store(slot);
        let _ = span;
    }

    /* ───────────────────────── 变量读写 ───────────────────────── */

    fn store(&mut self, slot: Slot) {
        self.emit(match slot {
            Slot::Local(i) => Op::SetLocal(i),
            Slot::Global(i) => Op::SetGlobal(i),
        });
    }

    fn load(&mut self, slot: Slot) {
        self.emit(match slot {
            Slot::Local(i) => Op::GetLocal(i),
            Slot::Global(i) => Op::GetGlobal(i),
        });
    }

    fn load_target(&mut self, t: &Target) {
        match t {
            Target::Var(name) => {
                let slot = self.resolve(name);
                self.load(slot);
            }
            Target::Member(obj, name) => {
                self.expr(obj);
                let idx = self.intern(name);
                self.emit(Op::Member(idx));
            }
            Target::Idx(obj, index) => {
                self.expr(obj);
                self.expr(index);
                self.emit(Op::Index);
            }
        }
    }

    /// 压入赋值目标的"引用部分"：    /// * 变量：什么都不压（*Set 指令直接吃值）
    /// * 成员：压 obj
    /// * 下标：压 obj + index
    fn push_ref(&mut self, t: &Target) {
        match t {
            Target::Var(_) => {}
            Target::Member(obj, _) => self.expr(obj),
            Target::Idx(obj, index) => {
                self.expr(obj);
                self.expr(index);
            }
        }
    }

    /// 收尾写入：此时栈顶是值，下面是 push_ref 压的 obj/index
    fn emit_set(&mut self, t: &Target) {
        match t {
            Target::Var(name) => {
                let slot = self.resolve(name);
                self.store(slot);
            }
            Target::Member(_, name) => {
                let idx = self.intern(name);
                self.emit(Op::MemberSet(idx));
            }
            Target::Idx(..) => {
                self.emit(Op::IndexSet);
            }
        }
    }

    /// 把 `变量 op 常量` / `变量 op 变量` 各压成一条指令。
    /// 命中返回 true（已经 emit 完，调用方不要再走通用路径）。
    fn try_bin_fused(&mut self, op: AOp, a: &Expr, b: &Expr) -> bool {
        let code = match binop_code(op) {
            Some(c) => c,
            None => return false,
        };
        let name = match &a.kind {
            ExprKind::Var(n) => n,
            _ => return false,
        };
        let slot = match self.lookup(name) {
            Some(s) => s,
            None => return false,
        };
        let (ag, ai) = slot_parts(slot);

        // 变量 op 常量
        if let Some(kv) = literal_of(b) {
            let k = self.konst(kv);
            self.emit(Op::BinLocalK { op: code, slot: ai, flags: ag as u8, k });
            return true;
        }
        // 变量 op 变量
        if let ExprKind::Var(bn) = &b.kind {
            if let Some(bs) = self.lookup(bn) {
                let (bg, bi) = slot_parts(bs);
                self.emit(Op::BinLocalLocal {
                    op: code,
                    a: ai,
                    aflags: ag as u8,
                    b: bi,
                    bflags: bg as u8,
                });
                return true;
            }
        }
        false
    }

    /* ───────────────────────── 表达式 ───────────────────────── */

    fn expr(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Num(n) => {
                let v = if n.fract() == 0.0 && n.abs() < 9.007_199_254_740_992e15 {
                    Value::Int(*n as i64)
                } else {
                    Value::Float(*n)
                };
                self.emit_const(v);
            }
            ExprKind::Str(s) => {
                self.emit_const(Value::string(s.as_ref().to_string()));
            }
            ExprKind::Bool(b) => {
                self.emit(if *b { Op::True } else { Op::False });
            }
            ExprKind::Null => {
                self.emit(Op::Nil);
            }
            ExprKind::Var(name) => {
                // 顺序：已声明的变量 > 内建函数 > 兜底当全局（null）
                match self.lookup(name) {
                    Some(slot) => self.load(slot),
                    None => match crate::builtins::builtin_static(name) {
                        Some(b) => {
                            self.emit_const(Value::Native(b));
                        }
                        None => {
                            let slot = self.resolve(name);
                            self.load(slot);
                        }
                    },
                }
            }
            ExprKind::SelfRef => {
                self.err("self 只能用在类方法里，这版 VM 还没实现类", &e.span);
                self.emit(Op::Nil);
            }
            ExprKind::List(items) => {
                for it in items {
                    self.expr(it);
                }
                self.emit(Op::BuildList(items.len() as u16));
            }
            ExprKind::Dict(entries) => {
                for (k, v) in entries {
                    self.expr(k);
                    self.expr(v);
                }
                self.emit(Op::BuildDict(entries.len() as u16));
            }
            ExprKind::Un { op, a } => {
                self.expr(a);
                self.emit(match op {
                    AOp::Neg => Op::Neg,
                    AOp::Not => Op::Not,
                    AOp::BitNot => Op::BitNot,
                    _ => Op::Not,
                });
            }
            ExprKind::Bin { op, a, b } => {
                match op {
                    AOp::And => {
                        self.expr(a);
                        let j = self.emit(Op::JumpIfFalseKeep(0));
                        self.emit(Op::Pop);
                        self.expr(b);
                        self.patch(j);
                    }
                    AOp::Or => {
                        self.expr(a);
                        let j = self.emit(Op::JumpIfTrueKeep(0));
                        self.emit(Op::Pop);
                        self.expr(b);
                        self.patch(j);
                    }
                    _ => {
                        // ① 编译期能算的直接算出来（常量折叠）
                        if let (Some(x), Some(y)) = (literal_of(a), literal_of(b)) {
                            if let Some(v) = fold_const(*op, &x, &y) {
                                self.emit_const(v);
                                return;
                            }
                        }
                        // ② 变量 op 变量 / 变量 op 常量 → 一条指令
                        if self.try_bin_fused(*op, a, b) {
                            return;
                        }
                        self.expr(a);
                        self.expr(b);
                        self.emit(binop_to_op(*op));
                    }
                }
            }
            ExprKind::Idx { obj, index } => {
                self.expr(obj);
                self.expr(index);
                self.emit(Op::Index);
            }
            ExprKind::Member { obj, name } => {
                self.expr(obj);
                let idx = self.intern(name);
                self.emit(Op::Member(idx));
            }
            ExprKind::Call { callee, args } => {
                // obj.method(args) 走 CallMethod，其余走 Call
                if let ExprKind::Member { obj, name } = &callee.kind {
                    self.expr(obj);
                    for a in args {
                        self.expr(a);
                    }
                    let idx = self.intern(name);
                    self.emit(Op::CallMethod(idx, args.len() as u8));
                } else {
                    // f(args)：f 是全局变量时走 CallGlobal（省一条 GetGlobal，参数也不用挪位）
                    if let ExprKind::Var(name) = &callee.kind {
                        if let Some(Slot::Global(idx)) = self.lookup(name) {
                            for a in args {
                                self.expr(a);
                            }
                            if args.len() <= 255 {
                                self.emit(Op::CallGlobal { slot: idx, nargs: args.len() as u8 });
                                return;
                            }
                        }
                    }
                    self.expr(callee);
                    for a in args {
                        self.expr(a);
                    }
                    self.emit(Op::Call(args.len() as u8));
                }
            }
            ExprKind::New { class, .. } => {
                self.err(
                    format!("new {}：这版 VM 还没实现类与实例", class),
                    &e.span,
                );
                self.emit(Op::Nil);
            }
        }
    }
}

/// 参数默认值在**编译期**求值。
///
/// 这样调用时不用再回到字节码里跑一遍，代价是默认值必须是字面量
/// （`b = 1` / `s = "x"` / 空）。这条限制对脚本语言完全够用，
/// 真需要"默认值是个表达式"时再改成就地插桩。
fn const_eval(e: Option<&Expr>) -> Value {
    match e {
        None => Value::Null,
        Some(expr) => match &expr.kind {
            ExprKind::Num(n) => {
                if n.fract() == 0.0 && n.abs() < 9.007_199_254_740_992e15 {
                    Value::Int(*n as i64)
                } else {
                    Value::Float(*n)
                }
            }
            ExprKind::Str(s) => Value::string(s.as_ref().to_string()),
            ExprKind::Bool(b) => Value::Bool(*b),
            ExprKind::Null => Value::Null,
            ExprKind::Un { op: AOp::Neg, a } => match const_eval(Some(a)) {
                Value::Int(i) => Value::Int(-i),
                Value::Float(f) => Value::Float(-f),
                other => other,
            },
            ExprKind::List(items) => Value::list(items.iter().map(|x| const_eval(Some(x))).collect()),
            _ => Value::Null,
        },
    }
}

/// 编译期常量折叠：两个操作数都是字面量就直接算出来。
/// 只处理数字/字符串这种"算错也没副作用"的情况，除零之类一律不折叠（留给运行时报错）。
fn fold_const(op: AOp, a: &Value, b: &Value) -> Option<Value> {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => {
            let (x, y) = (*x, *y);
            Some(match op {
                AOp::Add => Value::Int(x.checked_add(y)?),
                AOp::Sub => Value::Int(x.checked_sub(y)?),
                AOp::Mul => Value::Int(x.checked_mul(y)?),
                AOp::Div if y != 0 && x % y == 0 => Value::Int(x / y),
                AOp::Mod if y != 0 => Value::Int(x % y),
                AOp::Pow if (0..63).contains(&y) => Value::Int(x.checked_pow(y as u32)?),
                AOp::Eq => Value::Bool(x == y),
                AOp::Ne => Value::Bool(x != y),
                AOp::Lt => Value::Bool(x < y),
                AOp::Le => Value::Bool(x <= y),
                AOp::Gt => Value::Bool(x > y),
                AOp::Ge => Value::Bool(x >= y),
                _ => return None,
            })
        }
        (Value::Int(x), Value::Float(y)) => fold_const(op, &Value::Float(*x as f64), &Value::Float(*y)),
        (Value::Float(x), Value::Float(y)) => {
            let (x, y) = (*x, *y);
            Some(match op {
                AOp::Add => Value::Float(x + y),
                AOp::Sub => Value::Float(x - y),
                AOp::Mul => Value::Float(x * y),
                AOp::Div if y != 0.0 => Value::Float(x / y),
                AOp::Lt => Value::Bool(x < y),
                AOp::Le => Value::Bool(x <= y),
                AOp::Gt => Value::Bool(x > y),
                AOp::Ge => Value::Bool(x >= y),
                AOp::Eq => Value::Bool(x == y),
                AOp::Ne => Value::Bool(x != y),
                _ => return None,
            })
        }
        (Value::Float(x), Value::Int(y)) => {
            fold_const(op, &Value::Float(*x), &Value::Float(*y as f64))
        }
        (Value::Str(x), Value::Str(y)) if op == AOp::Add => {
            let mut s = x.as_str().to_string();
            s.push_str(y.as_str());
            Some(Value::string(s))
        }
        (Value::Bool(x), Value::Bool(y)) => match op {
            AOp::Eq => Some(Value::Bool(x == y)),
            AOp::Ne => Some(Value::Bool(x != y)),
            _ => None,
        },
        _ => None,
    }
}

/// 二元运算 → `BinLocalK` 里的操作码
fn binop_code(op: AOp) -> Option<u8> {
    Some(match op {
        AOp::Add => 0,
        AOp::Sub => 1,
        AOp::Mul => 2,
        AOp::Div => 3,
        AOp::Mod => 4,
        AOp::Pow => 5,
        AOp::Eq => 6,
        AOp::Ne => 7,
        AOp::Lt => 8,
        AOp::Le => 9,
        AOp::Gt => 10,
        AOp::Ge => 11,
        _ => return None,
    })
}

/// 槽位 → (是不是全局, 槽号)，给融合指令编码用
fn slot_parts(s: Slot) -> (bool, u16) {
    match s {
        Slot::Local(i) => (false, i),
        Slot::Global(i) => (true, i),
    }
}

/// 复合赋值指令的操作码（0 加 1 减 2 乘 3 除 4 取余 5 幂）
fn compound_bop(op: AOp) -> Option<u8> {
    Some(match op {
        AOp::Add => 0,
        AOp::Sub => 1,
        AOp::Mul => 2,
        AOp::Div => 3,
        AOp::Mod => 4,
        AOp::Pow => 5,
        _ => return None,
    })
}

/// 编译期就能定下来的字面量（给 `x += 1` 这类模式用）
fn literal_of(e: &Expr) -> Option<Value> {
    match &e.kind {
        ExprKind::Num(n) => Some(if n.fract() == 0.0 && n.abs() < 9.007_199_254_740_992e15 {
            Value::Int(*n as i64)
        } else {
            Value::Float(*n)
        }),
        ExprKind::Str(s) => Some(Value::string(s.as_ref().to_string())),
        ExprKind::Bool(b) => Some(Value::Bool(*b)),
        ExprKind::Null => Some(Value::Null),
        ExprKind::Un { op: AOp::Neg, a } => literal_of(a).map(|v| match v {
            Value::Int(i) => Value::Int(-i),
            Value::Float(f) => Value::Float(-f),
            other => other,
        }),
        _ => None,
    }
}

fn binop_to_op(op: AOp) -> Op {    match op {        AOp::Add => Op::Add,
        AOp::Sub => Op::Sub,
        AOp::Mul => Op::Mul,
        AOp::Div => Op::Div,
        AOp::Mod => Op::Mod,
        AOp::Pow => Op::Pow,
        AOp::Eq => Op::Eq,
        AOp::Ne => Op::Ne,
        AOp::Lt => Op::Lt,
        AOp::Le => Op::Le,
        AOp::Gt => Op::Gt,
        AOp::Ge => Op::Ge,
        AOp::And => Op::BitAnd,
        AOp::Or => Op::BitOr,
        AOp::BitAnd => Op::BitAnd,
        AOp::BitOr => Op::BitOr,
        AOp::BitXor => Op::BitXor,
        AOp::Shl => Op::Shl,
        AOp::Shr => Op::Shr,
        AOp::Neg => Op::Neg,
        AOp::Not => Op::Not,
        AOp::BitNot => Op::BitNot,
    }
}

/// 编译整段程序。返回 (chunk, 函数表, 错误)。
pub fn compile(prog: &Program) -> (Chunk, Vec<Rc<UserFn>>, Vec<FloatError>) {
    let mut c = Compiler::new(Rc::clone(&prog.file));
    let chunk = c.compile_program(prog);
    let mut errs = prog.errors.clone();
    errs.extend(c.errors);
    (chunk, c.funcs, errs)
}

