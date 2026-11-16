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
use crate::TypeName;
use crate::error::{FloatError, Span};
use crate::value::{UserFn, Value};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

/// 闭包上值的来源：造闭包时从当前帧的哪个格子取 Rc<RefCell> 引用。
/// 定义在最前面，Scope / Chunk 都要用。
#[derive(Clone, Debug)]
pub enum UpSrc {
    /// 当前帧的逃逸局部变量 base+slot（已装箱成 cell）
    Local(u16),
    /// 当前帧的第 i 个上值（跨层传递捕获）
    Upval(u16),
}

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

    /* ── 闭包上值：cell 是 Rc<RefCell<Value>>，跨栈帧共享同一个变量 ── */
    /// 先建一个空 cell（值是 null）放到 frame.cells[slot]。
    /// 声明处先建 cell 再算初值，这样递归闭包能在造自己时就捕到自己的格子。
    CellNew(u16),
    /// 逃逸参数在函数入口的装箱：把 base+slot 的普通值移进 cell 存到 frame.cells[slot]
    BoxParam(u16),
    /// 读 frame.cells[slot] 里的值
    GetBox(u16),
    /// 写 frame.cells[slot]
    SetBox(u16),
    /// 读 frame.upvals[i]（捕获的外上值）里的值
    GetUpval(u16),
    /// 写 frame.upvals[i]
    SetUpval(u16),
    /// 造闭包：常量表 fidx 处的 UserFn + upval_sources[srcs] 描述的 cell 引用 → Value::Closure
    MakeClosure { fidx: u32, srcs: u32 },

    Add,
    Sub,
    Mul,
    Div,
    /// 整除：向下取整
    IDiv,
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
    /// 切片 xs[a:b]：栈上是 [obj, from, to]（缺的一边是 Nil）
    Slice,
    /// 取局部变量的地址（&x），槽位相对当前帧
    AddrLocal(u16),
    /// 取全局变量的地址（&x）
    AddrGlobal(u16),
    /// 取元素地址（&xs[i] / &p[i]）：栈上是 [容器, 下标]
    AddrIdx,
    /// 解引用 *p
    Deref,
    /// *p = 值：栈上是 [指针, 值]
    StoreDeref,
    IndexSet,
    Member(u32),
    MemberSet(u32),

    Print(u16),
    /// 读一行输入，压栈（没有输入时给空串）
    Ask,
    /// 报告：弹出 n 个值，拼成一行写进「报告」通道（0 个 = 报告当前所有变量）
    Report(u16),
    /// 调试输出：弹出 n 个值拼成一行，按 level 带颜色级别写给开发者看（不影响程序）
    /// level：1 = 报告（蓝），2 = 警告（黄），3 = 报错（红）
    Debug { n: u16, level: u8 },
    /// 压一个 float：距上次 time reset 的秒数
    TimeRead,
    /// 把计时器归零
    TimeReset,

    IterInit,
    /// 迭代器取下一个：取到压栈，取完跳到目标
    IterNext(u32),
    IterEnd,

    /* ── try / catch ── */
    /// 记一个"出错就往 handler 跳"的现场（handler 地址先写 0，后面 patch）
    TryBegin(u32),
    /// 正常跑完 try 块，撤掉现场
    TryEnd,
    /// 弹栈顶当错误抛出去
    Throw,
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
    /// 全局槽位 → 变量名（`报告` 不带参数时要列出来）
    pub global_names: Vec<Rc<str>>,
    pub n_globals: u16,
    pub file: Rc<str>,
    /// 参与编译的文件（[0] 是主文件）。用了 import 时不止一个。
    pub files: Vec<Rc<str>>,
    /// 每一行（下标 = 行号 - 1）属于哪个文件；空 = 全算主文件
    pub line_file: Vec<u16>,
    /// 每一行在它自己那个文件里的行号
    pub line_local: Vec<u32>,
    /// 顶层函数提升表：(全局槽号, 常量表里的 Value::Func 下标)。
    /// 运行开始前 VM 按它把函数先放进全局槽——这样「先调用后定义」才成立，
    /// 和 TS 引擎的函数提升语义一致。
    pub hoists: Vec<(u16, u32)>,
    /// 闭包的 upvalue 来源表：MakeClosure.srcs 指向这里的一项。
    /// 每项按本函数捕获顺序，说明每个上值从当前帧的哪个格子取引用。
    pub upval_sources: Vec<Vec<UpSrc>>,
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
    /// 普通局部变量：栈槽直读直写，热路径零开销
    Local(u16),
    /// 逃逸的局部变量：值存在 cell 里（GetBox / SetBox）
    Cell(u16),
    Global(u16),
    /// 捕获的外层变量：读 frame 的 upval（GetUpval / SetUpval）
    Upval(u16),
    /// lazy 参数：槽里是零参 thunk 闭包，读的时候 Call(0) 求值（by-name）
    Lazy(u16),
}

struct Scope {
    vars: HashMap<Rc<str>, u16>,
    next: u16,
    /// 变量名 → 声明时写的类型（`var` / 类名 这类"不强制"的不记）
    types: HashMap<Rc<str>, TypeName>,
}

impl Scope {
    fn new() -> Scope {
        Scope { vars: HashMap::new(), next: 0, types: HashMap::new() }
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
    /// 进这层循环时 try 的深度（break/continue 跳出 try 要把现场撤掉）
    try_depth: usize,
    /// 进这层循环时"带 finally 的 try"的深度（break 要把循环里那几层 finally 跑掉）
    fin_depth: usize,
    breaks: Vec<usize>,
    continues: Vec<usize>,
}

/* ───────────────────── 闭包自由变量分析 ─────────────────────
 * 字节码是"边走边发"的：等编译到 `int base = 100` 时，还不知道后面的
 * 内层函数会不会捕获它，没法当场决定要不要装箱。所以编译前先跑一遍
 * 纯分析，把每个函数「要捕获哪些名字」「哪些局部变量会被内层捕获」算好。
 *
 * 算法是经典的词法作用域解析：维护一层层 scope，遇到变量引用就从里往外找；
 * 找到外层函数（不是全局）的局部变量时，把它记进中间每一层的 free（逐层转发），
 * 并把目标层那个局部变量标成 celled（要装箱）。名字即可当作身份——本语言的
 * 局部变量作用域是"每函数一张平表"，同一个函数里不允许同名重声明。
 */
#[derive(Clone, Debug, Default)]
struct FnInfo {
    /// 本函数要捕获的自由变量，顺序 = upvalue 下标
    free: Vec<Rc<str>>,
    /// 本函数的局部变量里被内层捕获的名字（要装箱成 cell）
    celled: Vec<Rc<str>>,
}

#[derive(Default)]
struct AScope {
    names: Vec<Rc<str>>,
    info: FnInfo,
}

#[derive(Default)]
struct Analyzer {
    scopes: Vec<AScope>,
    /// (行, 列, 函数名) → 该函数的捕获信息
    infos: HashMap<(u32, u32, Rc<str>), FnInfo>,
    /// 具名函数的 lazy 参数位图，用来识别调用处的 thunk 实参
    lazy_sigs: HashMap<Rc<str>, Vec<bool>>,
}

/// 预扫描整段程序，收集每个具名函数哪些参数是 lazy。
/// 调用点靠它把实参编成零参闭包——必须提前全量收集，因为「先调用后定义」是允许的。
fn collect_lazy_sigs(prog: &Program) -> HashMap<Rc<str>, Vec<bool>> {
    fn walk(body: &[Stmt], map: &mut HashMap<Rc<str>, Vec<bool>>) {
        for st in body {
            match &st.kind {
                StmtKind::FuncDef { name, params, body, .. } => {
                    map.insert(Rc::clone(name), params.iter().map(|p| p.lazy).collect());
                    walk(body, map);
                }
                StmtKind::Group(xs) => walk(xs, map),
                StmtKind::If { branches, else_body } => {
                    for (_, b) in branches { walk(b, map); }
                    if let Some(b) = else_body { walk(b, map); }
                }
                StmtKind::While { body, .. } | StmtKind::WaitUntil { body, .. }
                | StmtKind::Repeat { body, .. } => walk(body, map),
                StmtKind::ForRange { body, .. } | StmtKind::ForIn { body, .. } => walk(body, map),
                StmtKind::Try { body, catch_body, finally, .. } => {
                    walk(body, map);
                    if let Some(b) = catch_body { walk(b, map); }
                    if let Some(b) = finally { walk(b, map); }
                }
                _ => {}
            }
        }
    }
    let mut map = HashMap::new();
    walk(&prog.body, &mut map);
    map
}

impl Analyzer {
    fn new() -> Analyzer {
        let mut a = Analyzer::default();
        a.scopes.push(AScope::default());
        a
    }

    fn declare(&mut self, name: &Rc<str>) {
        let sc = self.scopes.last_mut().unwrap();
        if !sc.names.iter().any(|n| n == name) {
            sc.names.push(Rc::clone(name));
        }
    }

    /// 解析一次变量引用；只处理"引用了外层函数的局部变量"这一种情况。
    fn use_name(&mut self, name: &Rc<str>) {
        let cur = self.scopes.len() - 1;
        let mut target = None;
        for i in (0..=cur).rev() {
            if self.scopes[i].names.iter().any(|n| n == name) {
                target = Some(i);
                break;
            }
        }
        let target = match target {
            // 0 = 全局；cur = 当前函数自己的局部变量；都不用捕获
            Some(i) if i > 0 && i < cur => i,
            _ => return,
        };
        // 中间每一层都要转发这个上值
        for j in target + 1..=cur {
            let info = &mut self.scopes[j].info;
            if !info.free.iter().any(|n| n == name) {
                info.free.push(Rc::clone(name));
            }
        }
        let info = &mut self.scopes[target].info;
        if !info.celled.iter().any(|n| n == name) {
            info.celled.push(Rc::clone(name));
        }
    }

    fn block(&mut self, body: &[Stmt]) {
        for st in body {
            self.stmt(st);
        }
    }

    fn target(&mut self, t: &Target) {
        match t {
            Target::Var(n) => self.use_name(n),
            Target::Member(o, _) => self.expr(o),
            Target::Idx(o, i) => { self.expr(o); self.expr(i); }
            Target::Deref(p) => self.expr(p),
        }
    }

    fn expr(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Var(n) => self.use_name(n),
            ExprKind::SelfRef => self.use_name(&Rc::from("self")),
            ExprKind::Num(_) | ExprKind::Str(_) | ExprKind::Bool(_) | ExprKind::Null => {}
            ExprKind::List(xs) => for x in xs { self.expr(x); },
            ExprKind::Dict(xs) => for (k, v) in xs { self.expr(k); self.expr(v); },
            ExprKind::Un { a, .. } => self.expr(a),
            ExprKind::Bin { a, b, .. } => { self.expr(a); self.expr(b); }
            ExprKind::Ternary { cond, a, b } => { self.expr(cond); self.expr(a); self.expr(b); }
            ExprKind::Lambda { params, body } => {
                // lambda 自成一个函数作用域：参数是它的局部，body 里的自由变量向上捕获
                self.scopes.push(AScope::default());
                for p in params {
                    self.declare(&p.name);
                    if let Some(d) = &p.default { self.expr(d); }
                }
                self.expr(body);
                let sc = self.scopes.pop().unwrap();
                self.infos.insert((e.span.line, e.span.col, Rc::from("(lambda)")), sc.info);
            }
            ExprKind::Idx { obj, index } => { self.expr(obj); self.expr(index); }
            ExprKind::Slice { obj, from, to } => {
                self.expr(obj);
                if let Some(x) = from { self.expr(x); }
                if let Some(x) = to { self.expr(x); }
            }
            ExprKind::Member { obj, .. } => self.expr(obj),
            ExprKind::Call { callee, args } => {
                self.expr(callee);
                let mask = match &callee.kind {
                    ExprKind::Var(n) => self.lazy_sigs.get(n.as_ref()).cloned(),
                    _ => None,
                };
                for (i, a) in args.iter().enumerate() {
                    if mask.as_ref().map(|m| m.get(i).copied().unwrap_or(false)).unwrap_or(false) {
                        // lazy 实参会被编成零参 thunk，它自成一个捕获作用域
                        self.scopes.push(AScope::default());
                        self.expr(a);
                        let sc = self.scopes.pop().unwrap();
                        let key = (a.span.line, a.span.col, Rc::from(format!("(thunk_{}_{})", a.span.line, i)));
                        self.infos.insert(key, sc.info);
                    } else {
                        self.expr(a);
                    }
                }
            }
            ExprKind::New { args, .. } => for a in args { self.expr(a); },
        }
    }

    fn stmt(&mut self, s: &Stmt) {
        match &s.kind {
            StmtKind::Group(xs) => self.block(xs),
            StmtKind::Nop | StmtKind::Break | StmtKind::Continue | StmtKind::Stop
            | StmtKind::TimeRead | StmtKind::TimeReset | StmtKind::Unsupported { .. } => {}
            StmtKind::Decl { name, init, .. } => {
                self.declare(name);
                if let Some(e) = init { self.expr(e); }
            }
            StmtKind::Assign { target, value, .. } => { self.target(target); self.expr(value); }
            StmtKind::IncDec { target, .. } => self.target(target),
            StmtKind::ExprStmt(e) | StmtKind::IfOut(e) | StmtKind::Throw(e) => self.expr(e),
            StmtKind::Print(xs) | StmtKind::Report(xs) | StmtKind::Debug { args: xs, .. } => {
                for x in xs { self.expr(x); }
            }
            StmtKind::Ask(e) => if let Some(e) = e { self.expr(e); },
            StmtKind::If { branches, else_body } => {
                for (c, b) in branches {
                    if let Some(c) = c { self.expr(c); }
                    self.block(b);
                }
                if let Some(b) = else_body { self.block(b); }
            }
            StmtKind::While { cond, body } | StmtKind::WaitUntil { cond, body } => {
                self.expr(cond); self.block(body);
            }
            StmtKind::Repeat { count, body } => { self.expr(count); self.block(body); }
            StmtKind::ForRange { from, to, step, body, var, .. } => {
                if let Some(e) = from { self.expr(e); }
                self.expr(to);
                if let Some(e) = step { self.expr(e); }
                self.declare(var);
                self.block(body);
            }
            StmtKind::ForIn { var, value_name, iter, body } => {
                self.expr(iter);
                self.declare(var);
                if let Some(v) = value_name { self.declare(v); }
                self.block(body);
            }
            StmtKind::Destructure { names, value } => {
                self.expr(value);
                for n in names { self.declare(n); }
            }
            StmtKind::Return(v) => if let Some(e) = v { self.expr(e); },
            StmtKind::FuncDef { name, params, body, .. } => {
                // 先在本层声明函数名：内层递归要能解析到它
                self.declare(name);
                self.scopes.push(AScope::default());
                for p in params {
                    self.declare(&p.name);
                    if let Some(d) = &p.default { self.expr(d); }
                }
                self.block(body);
                let sc = self.scopes.pop().unwrap();
                self.infos.insert((s.span.line, s.span.col, Rc::clone(name)), sc.info);
            }
            StmtKind::Try { body, catch_name, catch_body, finally } => {
                self.block(body);
                if let Some(n) = catch_name { self.declare(n); }
                if let Some(b) = catch_body { self.block(b); }
                if let Some(b) = finally { self.block(b); }
            }
        }
    }
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
    /// 当前嵌了几层 try（break/continue 出门时要撤掉)
    tries: usize,
    /// 外层每个 try 的 finally 块：return / break 提前出门时要把它们跑掉
    finallies: Vec<Rc<Vec<Stmt>>>,
    loops: Vec<LoopCtx>,
    pub errors: Vec<FloatError>,
    /// 编译产出：函数体登记在这里，靠 entry 地址回到字节码
    funcs: Vec<Rc<UserFn>>,
    fn_names: Vec<Rc<str>>,
    temp_seq: usize,
    /// 顶层函数提升：(全局槽号, Value::Func 的常量下标)。见 Chunk.hoists。
    hoists: Vec<(u16, u32)>,
    /// 每个 MakeClosure 的 upvalue 来源描述
    upval_sources: Vec<Vec<UpSrc>>,
    /// 闭包分析结果，key = (行, 列, 函数名)
    infos: HashMap<(u32, u32, Rc<str>), FnInfo>,
    /// 与 scopes 平行的捕获信息：active[0] 是顶层（空），active[i] 对应 scopes[i]
    active: Vec<FnInfo>,
    /// 与 scopes 平行：当前函数哪些局部槽是 lazy 参数（读时要求值 thunk）
    lazy_slots: Vec<HashSet<u16>>,
    /// 具名函数的 lazy 参数位图
    lazy_sigs: HashMap<Rc<str>, Vec<bool>>,
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
            tries: 0,
            finallies: Vec::new(),
            loops: Vec::new(),
            errors: Vec::new(),
            funcs: Vec::new(),
            fn_names: Vec::new(),
            temp_seq: 0,
            hoists: Vec::new(),
            upval_sources: Vec::new(),
            infos: HashMap::new(),
            active: vec![FnInfo::default()],
            lazy_slots: vec![HashSet::new()],
            lazy_sigs: HashMap::new(),
        }
    }

    /// 当前函数是否把某个名字装箱了
    fn is_celled(&self, name: &str) -> bool {
        self.active
            .last()
            .map(|f| f.celled.iter().any(|n| &**n == name))
            .unwrap_or(false)
    }

    /// 当前函数的某个局部槽是不是 lazy 参数
    fn is_lazy_slot(&self, slot: u16) -> bool {
        self.lazy_slots.last().map(|s| s.contains(&slot)).unwrap_or(false)
    }

    /// 当前函数的自由变量下标
    fn upval_index(&self, name: &str) -> Option<u16> {
        self.active.last().and_then(|f| {
            f.free.iter().position(|n| &**n == name).map(|i| i as u16)
        })
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
            | Op::IterNext(x)
            | Op::TryBegin(x) => *x = t,
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
                if self.is_lazy_slot(s) {
                    return Some(Slot::Lazy(s));
                }
                return Some(if self.is_celled(name) { Slot::Cell(s) } else { Slot::Local(s) });
            }
            // 自由变量优先于同名全局：分析器按词法链解析，这里必须同序
            if let Some(i) = self.upval_index(name) {
                return Some(Slot::Upval(i));
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
                if self.is_lazy_slot(s) {
                    return Slot::Lazy(s);
                }
                return if self.is_celled(name) { Slot::Cell(s) } else { Slot::Local(s) };
            }
            if let Some(i) = self.upval_index(name) {
                return Slot::Upval(i);
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

    /// `int a = "x"` 这种一眼能看出来的类型错，编译期就拦掉。
    /// 只查"右边是字面量"的情况：变量、函数返回值这些静态看不出类型的，留给运行时。
    fn check_decl_type(&mut self, name: &Rc<str>, ty: &TypeName, init: Option<&Expr>, span: &Span) {
        let e = match init {
            Some(e) => e,
            None => return,
        };
        let got = match &e.kind {
            ExprKind::Str(_) => "str",
            ExprKind::Bool(_) => "bool",
            ExprKind::List(_) => "list",
            ExprKind::Dict(_) => "dict",
            ExprKind::Num(v) => {
                if v.fract() == 0.0 && v.is_finite() { "int" } else { "float" }
            }
            _ => return,
        };
        let want = match ty {
            TypeName::Int => "int",
            TypeName::Float => "float",
            TypeName::Str => "str",
            TypeName::Bool => "bool",
            TypeName::List => "list",
            TypeName::Dict => "dict",
            _ => return,
        };
        let ok = want == got || (want == "float" && got == "int");
        if !ok {
            self.err(
                format!("{} 是 {}，但右边给的是 {}", name, want, got),
                span,
            );
        }
    }

    /// 赋值时检查：这个变量是声明成什么类型的
    fn check_assign_type(&mut self, target: &Target, value: &Expr, span: &Span) {
        let name = match target {
            Target::Var(n) => n,
            _ => return,
        };
        let ty = self
            .scopes
            .last()
            .and_then(|sc| sc.types.get(name).copied())
            .or_else(|| self.scopes[0].types.get(name).copied());
        let ty = match ty {
            Some(t) => t,
            None => return,
        };
        self.check_decl_type(name, &ty, Some(value), span);
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

    /// 声明一个变量；被内层函数捕获的局部变量当场建 cell。
    fn declare_var(&mut self, name: &Rc<str>) -> Slot {
        let raw = self.declare_local(name);
        let slot = match raw {
            Slot::Local(i) => self.boxed_slot(name, i),
            other => other,
        };
        if let Slot::Cell(i) = slot {
            self.emit(Op::CellNew(i));
        }
        slot
    }

    /// declare_local 的结果按捕获信息升级成 Cell（不额外发指令，给参数入口用）
    fn boxed_slot(&self, name: &Rc<str>, s: u16) -> Slot {
        if self.is_celled(name) { Slot::Cell(s) } else { Slot::Local(s) }
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
        // 先跑一遍闭包分析：编译到局部变量时才知道它会不会被内层捕获
        let mut analyzer = Analyzer::new();
        analyzer.lazy_sigs = collect_lazy_sigs(prog);
        analyzer.block(&prog.body);
        self.lazy_sigs = analyzer.lazy_sigs.clone();
        self.infos = analyzer.infos;
        for s in &prog.body {
            self.stmt(s);
        }
        // 先把 槽号 → 名字 反过来存一份，运行时的"报告当前变量"要用
        let mut global_names = vec![Rc::from(""); self.scopes[0].next as usize];
        for (name, slot) in self.scopes[0].vars.iter() {
            if (*slot as usize) < global_names.len() {
                global_names[*slot as usize] = Rc::clone(name);
            }
        }
        self.emit(Op::Nil);
        self.emit(Op::Return);
        Chunk {
            code: std::mem::take(&mut self.code),
            lines: std::mem::take(&mut self.lines),
            consts: std::mem::take(&mut self.consts),
            names: std::mem::take(&mut self.names),
            fn_names: std::mem::take(&mut self.fn_names),
            global_names,
            n_globals: self.scopes[0].next,
            file: Rc::clone(&self.file),
            files: vec![Rc::clone(&self.file)],
            line_file: Vec::new(),
            line_local: Vec::new(),
            hoists: std::mem::take(&mut self.hoists),
            upval_sources: std::mem::take(&mut self.upval_sources),
        }
    }

    /* ───────────────────────── 语句 ───────────────────────── */

    fn stmt(&mut self, s: &Stmt) {
        if s.span.line > 0 {
            self.line = s.span.line;
        }
        match &s.kind {
            StmtKind::Group(list) => {
                for st in list {
                    self.stmt(st);
                }
            }
            StmtKind::Nop => {}
            StmtKind::Decl { name, ty, init } => {
                self.check_decl_type(name, ty, init.as_ref(), &s.span);
                let slot = self.declare_var(name);
                if matches!(ty, TypeName::Int | TypeName::Float | TypeName::Str | TypeName::Bool | TypeName::List | TypeName::Dict) {
                    self.scopes.last_mut().unwrap().types.insert(Rc::clone(name), *ty);
                }
                match init {
                    Some(e) => self.expr(e),
                    // `list xs` / `dict d` 不给初值时给个空容器，
                    // 不然一句 add … to 就崩在 null 上（老脚本大量这么写）
                    None => {
                        // 空变量：`int c` 这种只声明不赋值的写法。
                        // 按类型给个中性初值（而不是一律 null），这样
                        // `int 总数` + `总数 += x` 这种累加写法直接就能用。
                        match ty {
                            TypeName::List => self.emit(Op::BuildList(0)),
                            TypeName::Dict => self.emit(Op::BuildDict(0)),
                            TypeName::Int => self.emit_const(Value::Int(0)),
                            TypeName::Float => self.emit_const(Value::Float(0.0)),
                            TypeName::Str => self.emit_const(Value::string(String::new())),
                            TypeName::Bool => self.emit(Op::False),
                            _ => self.emit(Op::Nil),
                        };
                    }
                }
                self.store(slot);
            }
            StmtKind::Assign { target, op, value } => {
                if *op == AssignOp::Set {
                    self.check_assign_type(target, value, &s.span);
                    self.push_ref(target);
                    self.expr(value);
                    self.emit_set(target);
                } else {
                    let bop = binop_to_op(op.to_op().unwrap());
                    // 融合快路径：`x op= y` / `x op= 常量` 一条指令搞定，
                    // 省掉"读变量→算→写变量"三条指令来回倒栈
                    if let (Target::Var(name), Some(cbop)) = (target, compound_bop(op.to_op().unwrap())) {
                        let slot = self.resolve(name);
                        let (is_global, dst) = match slot_parts(slot) {
                            Some(x) => x,
                            None => {
                                // 捕获变量没有融合指令：走下面的读-算-写通用路径
                                self.load(slot);
                                self.expr(value);
                                self.emit(bop);
                                self.store(slot);
                                return;
                            }
                        };
                        let flags = is_global as u8;

                        if let ExprKind::Var(src_name) = &value.kind {
                            if let Some(sslot) = self.lookup(src_name) {
                                if let Some((sg, si)) = slot_parts(sslot) {
                                    self.emit(Op::CompoundVar {
                                        bop: cbop,
                                        dst,
                                        src: si,
                                        flags: flags | ((sg as u8) << 1),
                                    });
                                    return;
                                }
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
                    match slot_parts(slot) {
                        Some((is_global, idx)) => {
                            self.emit(Op::IncVar { slot: idx, flags: is_global as u8, delta: *delta as i8 });
                        }
                        None => {
                            // 捕获变量：读 → ±1 → 写，没有融合指令
                            self.load(slot);
                            self.emit_const(Value::Int(1));
                            self.emit(if *delta > 0 { Op::Add } else { Op::Sub });
                            self.store(slot);
                        }
                    }
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
            StmtKind::Report(args) => {
                for a in args {
                    self.expr(a);
                }
                self.emit(Op::Report(args.len() as u16));
            }
            StmtKind::Debug { level, args } => {
                for a in args {
                    self.expr(a);
                }
                self.emit(Op::Debug { n: args.len() as u16, level: *level });
            }
            // wait until (条件)：条件成立就往下走，否则一直跑块（带上限防死循环）
            StmtKind::WaitUntil { cond, body } => {
                let start = self.here();
                self.expr(cond);
                self.emit(Op::Not);
                let jf = self.emit(Op::JumpIfFalse(0));
                self.loops.push(LoopCtx { iter_depth: self.iters, range_depth: self.ranges, try_depth: self.tries, fin_depth: self.finallies.len(), breaks: vec![], continues: vec![] });
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
            // if out (条件)：条件成立就跳出当前循环（等于"条件式 break"）
            StmtKind::IfOut(cond) => {
                self.expr(cond);
                let jf = self.emit(Op::JumpIfFalse(0));
                self.stmt(&Stmt::new(StmtKind::Break, s.span.clone()));
                let after = self.here();
                self.patch_to(jf, after);
            }
            // time read：把秒数写进 __last_time
            StmtKind::TimeRead => {
                self.emit(Op::TimeRead);
                self.store_special("__last_time");
            }
            StmtKind::TimeReset => {
                self.emit(Op::TimeReset);
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
                self.loops.push(LoopCtx { iter_depth: self.iters, range_depth: self.ranges, try_depth: self.tries, fin_depth: self.finallies.len(), breaks: vec![], continues: vec![] });
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
                self.loops.push(LoopCtx { iter_depth: self.iters, range_depth: self.ranges, try_depth: self.tries, fin_depth: self.finallies.len(), breaks: vec![], continues: vec![] });
                for st in body {
                    self.stmt(st);
                }
                // continue 必须接在"递减"这一步上：直接跳回循环头的话计数永远不减，
                // 写成 `if (...) continue` 就是死循环（对拍抓到的真 bug）
                let dec = self.here();
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
                    self.patch_to(c, dec);
                }
            }
            StmtKind::ForRange { var, from, to, step, inclusive, body } => {
                let slot = self.declare_var(var);
                // 捕获的循环变量：RangeInit/RangeNext 照旧写栈槽，
                // 每圈开头再把它同步进 cell 让内层闭包看到最新值
                let (is_global, var_idx, sync_cell) = match slot {
                    Slot::Local(i) => (false, i, None),
                    Slot::Cell(i) => (false, i, Some(i)),
                    Slot::Global(i) => (true, i, None),
                    Slot::Upval(_) | Slot::Lazy(_) => unreachable!(),
                };

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
                if let Some(i) = sync_cell {
                    self.emit(Op::GetLocal(i));
                    self.emit(Op::SetBox(i));
                }

                self.ranges += 1;
                self.loops.push(LoopCtx {
                    iter_depth: self.iters,
                    range_depth: self.ranges - 1,
                    try_depth: self.tries,
                    fin_depth: self.finallies.len(),
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
                let slot = self.declare_var(var);
                let vslot = value_name.as_ref().map(|n| self.declare_var(n));
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
                self.loops.push(LoopCtx { iter_depth: self.iters - 1, range_depth: self.ranges, try_depth: self.tries, fin_depth: self.finallies.len(), breaks: vec![], continues: vec![] });
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
            StmtKind::Try { body, catch_name, catch_body, finally } => {
                let fin: Option<Rc<Vec<Stmt>>> = finally.as_ref().map(|f| Rc::new(f.clone()));
                if let Some(f) = &fin {
                    self.finallies.push(Rc::clone(f));
                }
                self.tries += 1;
                let tb = self.emit(Op::TryBegin(0));
                for st in body {
                    self.stmt(st);
                }
                self.emit(Op::TryEnd);
                self.tries -= 1;
                if fin.is_some() {
                    self.finallies.pop();
                }
                if let Some(f) = finally {
                    for st in f {
                        self.stmt(st);
                    }
                }
                let j_end = self.emit(Op::Jump(0));

                // ── 出错时从这里接上：错误值已经在栈顶 ──
                self.patch(tb);
                let has_catch = catch_body.is_some() || catch_name.is_some();
                if has_catch {
                    match catch_name {
                        Some(name) => {
                            let slot = self.declare_var(name);
                            self.store(slot);
                        }
                        None => {
                            self.emit(Op::Pop);
                        }
                    }
                    if let Some(cb) = catch_body {
                        if let Some(f) = &fin {
                            self.finallies.push(Rc::clone(f));
                        }
                        for st in cb {
                            self.stmt(st);
                        }
                        if fin.is_some() {
                            self.finallies.pop();
                        }
                    }
                    if let Some(f) = finally {
                        for st in f {
                            self.stmt(st);
                        }
                    }
                } else {
                    // 只有 finally：先把它跑完，再把错误原样抛给外层。
                    // 错误值一直留在栈顶，finally 语句是平衡的，跑完还在。
                    if let Some(f) = finally {
                        for st in f {
                            self.stmt(st);
                        }
                    }
                    self.emit(Op::Throw);
                }
                self.patch(j_end);
            }
            StmtKind::Throw(v) => {
                self.expr(v);
                self.emit(Op::Throw);
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
                // 跳出 try 时要撤掉它的处理器，不然错误会跳进已经跳出去的 catch
                let need_try = self.tries - self.loops.last().map(|l| l.try_depth).unwrap_or(self.tries);
                for _ in 0..need_try {
                    self.emit(Op::TryEnd);
                }
                // 循环里那几层 finally 也得跑（循环外面的不管）
                let fd = self.loops.last().map(|l| l.fin_depth).unwrap_or(0);
                self.run_finallies(fd);
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
                let need_try = self.tries - self.loops.last().map(|l| l.try_depth).unwrap_or(self.tries);
                for _ in 0..need_try {
                    self.emit(Op::TryEnd);
                }
                let j = self.emit(Op::Jump(0));
                if let Some(ctx) = self.loops.last_mut() {
                    ctx.continues.push(j);
                } else {
                    self.err("continue 不在循环里", &s.span);
                }
            }
            StmtKind::Stop => {
                self.run_finallies(0);
                self.emit(Op::Nil);
                self.emit(Op::Return);
            }
            StmtKind::Return(v) => {
                // `return n` 这种最常见：一条指令直接返回
                if let Some(e) = v {
                    if let ExprKind::Var(name) = &e.kind {
                        if let Some(slot) = self.lookup(name) {
                            if let Some((is_global, idx)) = slot_parts(slot) {
                                self.emit(Op::ReturnLocal { slot: idx, flags: is_global as u8 });
                                return;
                            }
                        }
                    }
                }
                match v {
                    Some(e) => self.expr(e),
                    None => {
                        self.emit(Op::Nil);
                    }
                }
                // return 从 try 里"提前出门"：外面那几层 finally 得先跑完
                // （返回值压在栈底，finally 语句是平衡的，不会碰它）
                self.run_finallies(0);
                self.emit(Op::Return);
            }
            StmtKind::FuncDef { name, params, body, .. } => self.func_def(name, params, body, &s.span),
            StmtKind::Destructure { names, value } => {
                let temp = self.new_temp();
                self.expr(value);
                self.store(temp);
                for (i, n) in names.iter().enumerate() {
                    let slot = self.declare_var(n);
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

    /// 把 return / break 出门前该跑的 finally 就地摊开。
    /// 从最内层开始（倒序），`from` 之下的是循环外面那几层，不该跑。
    fn run_finallies(&mut self, from: usize) {
        if self.finallies.len() <= from {
            return;
        }
        let blocks: Vec<Rc<Vec<Stmt>>> = self.finallies[from..].iter().rev().cloned().collect();
        let saved = std::mem::take(&mut self.finallies);
        for b in &blocks {
            for st in b.iter() {
                self.stmt(st);
            }
        }
        self.finallies = saved;
    }

    /// 编译一个函数体，产出 (函数原型, 常量下标, 捕获信息)。
    /// func_def 和内联 lambda 共用；函数体代码落在当前 code 里，前面一条 Jump 跳过。
    /// 调用方负责把函数值压栈（Const 或 MakeClosure）。
    fn emit_fn(
        &mut self,
        name: &Rc<str>,
        params: &[Param],
        body: &[Stmt],
        span: &Span,
    ) -> (Rc<UserFn>, u32, FnInfo) {
        let info = self
            .infos
            .get(&(span.line, span.col, Rc::clone(name)))
            .cloned()
            .unwrap_or_default();

        // 入口地址先占一个 Jump，跳过函数体
        let skip = self.emit(Op::Jump(0));
        let entry = self.here();

        self.scopes.push(Scope::new());
        self.active.push(info.clone());
        let mut lazy_set = HashSet::new();
        for p in params {
            let s = self.scopes.last_mut().unwrap().declare(&p.name);
            if p.lazy {
                lazy_set.insert(s);
            }
            // 逃逸参数入口装箱：默认值可能在它前面引用，所以必须在默认值之前
            if info.celled.iter().any(|n| n.as_ref() == p.name.as_ref()) {
                self.emit(Op::BoxParam(s));
            }
        }
        self.lazy_slots.push(lazy_set);
        // 默认值在**调用时**算，而不是编译期算常量：
        // 这样 `fn area(w, h = w)` 这种"默认值引用前面参数"才成立。
        for (k, p) in params.iter().enumerate() {
            if let Some(d) = &p.default {
                let j = self.emit(Op::JumpIfArgc { n: (k + 1) as u8, target: 0 });
                self.expr(d);
                if let Some(s) = self.scopes.last().and_then(|sc| sc.find(&p.name)) {
                    let dst = self.boxed_slot(&p.name, s);
                    self.store(dst);
                }
                self.patch(j);
            }
        }
        for st in body {
            self.stmt(st);
        }
        self.emit(Op::Nil);
        self.emit(Op::Return);
        let scope = self.scopes.pop().unwrap();
        self.active.pop();
        self.lazy_slots.pop();
        let nlocals = scope.next;
        self.patch(skip);

        // 把"被捕获的局部变量名"换成它们的实际槽号
        let celled = info
            .celled
            .iter()
            .filter_map(|n| scope.vars.get(n).map(|s| (*s, Rc::clone(n))))
            .collect();
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
            nupvals: info.free.len(),
            celled,
        });
        self.funcs.push(Rc::clone(&f));

        // 函数原型放常量表
        let cidx = self.konst(Value::Func(f.clone()));
        (f, cidx, info)
    }

    /// 在当前位置压入一个函数值：无上值直接压原型，有上值则造闭包。
    fn emit_fn_value(&mut self, cidx: u32, info: &FnInfo, span: &Span) {
        if info.free.is_empty() {
            self.emit(Op::Const(cidx));
        } else {
            let mut srcs = Vec::with_capacity(info.free.len());
            for n in &info.free {
                let src = match self.lookup(n) {
                    Some(Slot::Cell(i)) => UpSrc::Local(i),
                    Some(Slot::Upval(i)) => UpSrc::Upval(i),
                    // 分析器保证自由变量能在外层函数链上找到；到这里还没找到
                    // 说明分析和代码生成不一致，退化成 null 格子，绝不崩 VM
                    _ => {
                        self.err(
                            format!("闭包变量「{}」解析失败（编译内部错误）", n),
                            span,
                        );
                        UpSrc::Local(0)
                    }
                };
                srcs.push(src);
            }
            let sidx = self.upval_sources.len() as u32;
            self.upval_sources.push(srcs);
            self.emit(Op::MakeClosure { fidx: cidx, srcs: sidx });
        }
    }

    /// 调一个函数值：栈上先压 callee，再依次压参数。
    fn emit_call_slot(&mut self, fun: Slot, args: &[Slot]) {
        self.load(fun);
        for a in args {
            self.load(*a);
        }
        self.emit(Op::Call(args.len() as u8));
    }

    /// 对 src 列表的每一项跑一次 body，body 拿到的是"当前项"的槽。
    /// 纯字节码循环，不占编译器的 break/continue 栈（HOF 体内没有用户 break）。
    fn emit_iter_body(&mut self, src: Slot, body: impl FnOnce(&mut Compiler, Slot)) {
        self.load(src);
        self.emit(Op::IterInit);
        let start = self.here();
        let next = self.emit(Op::IterNext(0));
        let item = self.new_temp();
        self.store(item);
        body(self, item);
        let back = self.emit(Op::Jump(0));
        self.patch_to(back, start);
        let end = self.here();
        self.patch_to(next, end);
        self.emit(Op::IterEnd);
    }

    /// 把 `data.map/filter/…` 展开成内联循环字节码。回调是普通函数值，走现成的 Call 指令。
    /// 返回 false 表示参数个数不对，让调用方走普通路径报错。
    fn emit_hof(&mut self, name: &str, args: &[Expr]) -> bool {
        let arity = if name == "data.reduce" { 3 } else { 2 };
        if args.len() != arity {
            return false;
        }
        // 源列表 + 回调函数都先算一次，放进临时槽
        let src = self.new_temp();
        self.expr(&args[0]);
        self.store(src);
        let fun = self.new_temp();
        self.expr(&args[1]);
        self.store(fun);
        let push_idx = self.intern(&Rc::from("push"));

        match name {
            "data.map" => {
                let out = self.new_temp();
                self.emit(Op::BuildList(0));
                self.store(out);
                self.emit_iter_body(src, |c, item| {
                    c.emit_call_slot(fun, &[item]);
                    let res = c.new_temp();
                    c.store(res);
                    c.load(out);
                    c.load(res);
                    c.emit(Op::CallMethod(push_idx, 1));
                    c.emit(Op::Pop);
                });
                self.load(out);
            }
            "data.filter" => {
                let out = self.new_temp();
                self.emit(Op::BuildList(0));
                self.store(out);
                self.emit_iter_body(src, |c, item| {
                    c.emit_call_slot(fun, &[item]);
                    let jf = c.emit(Op::JumpIfFalse(0));
                    c.load(out);
                    c.load(item);
                    c.emit(Op::CallMethod(push_idx, 1));
                    c.emit(Op::Pop);
                    c.patch(jf);
                });
                self.load(out);
            }
            "data.reduce" => {
                let acc = self.new_temp();
                self.expr(&args[2]);
                self.store(acc);
                self.emit_iter_body(src, |c, item| {
                    c.emit_call_slot(fun, &[acc, item]);
                    c.store(acc);
                });
                self.load(acc);
            }
            "data.any" | "data.all" => {
                let want = name == "data.any";
                let res = self.new_temp();
                self.emit(if want { Op::False } else { Op::True });
                self.store(res);
                self.emit_iter_body(src, |c, item| {
                    // 已经有结果就不再调回调（保留短路语义）
                    c.load(res);
                    let skip = c.emit(if want { Op::JumpIfTrueKeep(0) } else { Op::JumpIfFalseKeep(0) });
                    c.emit(Op::Pop);
                    c.emit_call_slot(fun, &[item]);
                    c.store(res);
                    c.patch(skip);
                });
                self.load(res);
            }
            "data.find" => {
                let found = self.new_temp();
                let res = self.new_temp();
                self.emit(Op::False);
                self.store(found);
                self.emit(Op::Nil);
                self.store(res);
                self.emit_iter_body(src, |c, item| {
                    // 找到了就跳过这一圈（注意 Keep 版本不弹栈，跳过去后要 Pop）
                    c.load(found);
                    let skip = c.emit(Op::JumpIfTrueKeep(0));
                    c.emit(Op::Pop);
                    c.emit_call_slot(fun, &[item]);
                    let yes = c.emit(Op::JumpIfFalse(0));
                    c.load(item);
                    c.store(res);
                    c.emit(Op::True);
                    c.store(found);
                    c.patch(yes);
                    let end = c.emit(Op::Jump(0));
                    c.patch_to(skip, c.here());
                    c.emit(Op::Pop);
                    c.patch(end);
                });
                self.load(res);
            }
            "data.find_index" => {
                let idx = self.new_temp();
                let found = self.new_temp();
                self.emit_const(Value::Int(-1));
                self.store(idx);
                self.emit(Op::False);
                self.store(found);
                let counter = self.new_temp();
                self.emit_const(Value::Int(0));
                self.store(counter);
                self.emit_iter_body(src, |c, item| {
                    c.load(found);
                    let skip = c.emit(Op::JumpIfTrueKeep(0));
                    c.emit(Op::Pop);
                    c.emit_call_slot(fun, &[item]);
                    let yes = c.emit(Op::JumpIfFalse(0));
                    c.load(counter);
                    c.store(idx);
                    c.emit(Op::True);
                    c.store(found);
                    c.patch(yes);
                    // 每圈计数器 +1
                    c.load(counter);
                    c.emit_const(Value::Int(1));
                    c.emit(Op::Add);
                    c.store(counter);
                    let end = c.emit(Op::Jump(0));
                    c.patch_to(skip, c.here());
                    c.emit(Op::Pop);
                    c.patch(end);
                });
                self.load(idx);
            }
            "data.groupby" | "data.index_by" => {
                let out = self.new_temp();
                self.emit(Op::BuildDict(0));
                self.store(out);
                let by_group = name == "data.groupby";
                self.emit_iter_body(src, |c, item| {
                    c.emit_call_slot(fun, &[item]);
                    let key = c.new_temp();
                    c.store(key);
                    if by_group {
                        // 老 key 存在就往里 push，否则建一个新列表
                        c.load(out);
                        c.load(key);
                        c.emit(Op::Index);
                        let bucket = c.new_temp();
                        c.store(bucket);
                        c.load(bucket);
                        let old = c.emit(Op::JumpIfFalse(0)); // null → 走新建分支
                        // 已有桶：bucket.push(item)
                        c.load(bucket);
                        c.load(item);
                        c.emit(Op::CallMethod(push_idx, 1));
                        c.emit(Op::Pop);
                        let done = c.emit(Op::Jump(0));
                        c.patch(old);
                        // 新桶：[item]，写回 out[key]
                        c.load(item);
                        c.emit(Op::BuildList(1));
                        let nl = c.new_temp();
                        c.store(nl);
                        c.load(out);
                        c.load(key);
                        c.load(nl);
                        c.emit(Op::IndexSet);
                        c.patch(done);
                    } else {
                        c.load(out);
                        c.load(key);
                        c.load(item);
                        c.emit(Op::IndexSet);
                    }
                });
                self.load(out);
            }
            _ => return false,
        }
        true
    }

    /// 编译实参列表。lazy 位置的实参包成零参 thunk（闭包），调用方立刻求值由被调函数决定。
    fn emit_args(&mut self, args: &[Expr], callee: &Expr) {
        let mask = match &callee.kind {
            ExprKind::Var(n) => self.lazy_sigs.get(n.as_ref()).cloned(),
            _ => None,
        };
        for (i, a) in args.iter().enumerate() {
            if mask.as_ref().map(|m| m.get(i).copied().unwrap_or(false)).unwrap_or(false) {
                let thunk_name: Rc<str> = Rc::from(format!("(thunk_{}_{})", a.span.line, i).as_str());
                let ret = Stmt::new(StmtKind::Return(Some(a.clone())), a.span.clone());
                let (_f, cidx, info) = self.emit_fn(&thunk_name, &[], &[ret], &a.span);
                self.emit_fn_value(cidx, &info, &a.span);
            } else {
                self.expr(a);
            }
        }
    }

    fn func_def(&mut self, name: &Rc<str>, params: &[Param], body: &[Stmt], span: &Span) {
        // 先在本层声明函数名：内层递归要能解析到它（分析器同序）
        let slot = self.declare_var(name);
        let (_f, cidx, info) = self.emit_fn(name, params, body, span);
        self.emit_fn_value(cidx, &info, span);
        self.store(slot);
        // 顶层函数记入提升表：VM 启动时先放进全局槽，「先调用后定义」才不报错
        // （注意存的是常量下标，不是指令下标）
        if let Slot::Global(gs) = slot {
            self.hoists.push((gs, cidx));
        }
    }

    /* ───────────────────────── 变量读写 ───────────────────────── */

    fn store(&mut self, slot: Slot) {
        if let Slot::Lazy(_) = slot {
            // 赋值上来就报错；发一条 Pop 保持栈平衡
            let msg = "lazy 参数不能赋值（它是按名求值的表达式，每次读取重新计算）";
            self.err(msg, &Span::point(&self.file, self.line, 1));
            self.emit(Op::Pop);
            return;
        }
        self.emit(match slot {
            Slot::Local(i) => Op::SetLocal(i),
            Slot::Cell(i) => Op::SetBox(i),
            Slot::Global(i) => Op::SetGlobal(i),
            Slot::Upval(i) => Op::SetUpval(i),
            Slot::Lazy(_) => unreachable!(),
        });
    }

    fn load(&mut self, slot: Slot) {
        if let Slot::Lazy(i) = slot {
            // by-name：槽里是零参 thunk，读一次调一次
            self.emit(Op::GetLocal(i));
            self.emit(Op::Call(0));
            return;
        }
        self.emit(match slot {
            Slot::Local(i) => Op::GetLocal(i),
            Slot::Cell(i) => Op::GetBox(i),
            Slot::Global(i) => Op::GetGlobal(i),
            Slot::Upval(i) => Op::GetUpval(i),
            Slot::Lazy(_) => unreachable!(),
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
            // *p = 值 读旧值：先压指针再解引用
            Target::Deref(ptr) => {
                self.expr(ptr);
                self.emit(Op::Deref);
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
            // 栈上先放指针，值在它上面
            Target::Deref(ptr) => self.expr(ptr),
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
            Target::Deref(_) => {
                self.emit(Op::StoreDeref);
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
        let (ag, ai) = match slot_parts(slot) {
            Some(x) => x,
            None => return false,
        };

        // 变量 op 常量
        if let Some(kv) = literal_of(b) {
            let k = self.konst(kv);
            self.emit(Op::BinLocalK { op: code, slot: ai, flags: ag as u8, k });
            return true;
        }
        // 变量 op 变量
        if let ExprKind::Var(bn) = &b.kind {
            if let Some(bs) = self.lookup(bn) {
                let (bg, bi) = match slot_parts(bs) {
                    Some(x) => x,
                    None => return false,
                };
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
                // 类的降级方案里 self 就是方法的第 0 个参数，直接当普通变量取
                match self.lookup("self") {
                    Some(slot) => self.load(slot),
                    None => {
                        self.err("self 只能用在类的方法里", &e.span);
                        self.emit(Op::Nil);
                    }
                }
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
                match op {
                    // &x：直接按槽位取地址（不先把值压栈）
                    AOp::Addr => match &a.kind {
                        ExprKind::Var(name) => match self.resolve(name) {
                            Slot::Local(i) => {
                                self.emit(Op::AddrLocal(i));
                            }
                            Slot::Global(i) => {
                                self.emit(Op::AddrGlobal(i));
                            }
                            Slot::Cell(_) | Slot::Upval(_) | Slot::Lazy(_) => {
                                self.err(
                                    format!("变量「{}」被闭包捕获或是 lazy 参数，不能对它取地址", name),
                                    &e.span,
                                );
                                self.emit(Op::Nil);
                            }
                        },
                        ExprKind::Idx { obj, index } => {
                            self.expr(obj);
                            self.expr(index);
                            self.emit(Op::AddrIdx);
                        }
                        _ => {
                            self.emit(Op::Nil);
                        }
                    },
                    AOp::Deref => {
                        self.expr(a);
                        self.emit(Op::Deref);
                    }
                    _ => {
                        self.expr(a);
                        self.emit(match op {
                            AOp::Neg => Op::Neg,
                            AOp::Not => Op::Not,
                            AOp::BitNot => Op::BitNot,
                            _ => Op::Not,
                        });
                    }
                }
            }
            // 条件 ? 这个 : 那个 —— 只算被选中的那一支
            ExprKind::Ternary { cond, a, b } => {
                self.expr(cond);
                let jf = self.emit(Op::JumpIfFalse(0));
                self.expr(a);
                let jend = self.emit(Op::Jump(0));
                let else_at = self.here();
                self.patch_to(jf, else_at);
                self.expr(b);
                let end = self.here();
                self.patch_to(jend, end);
            }
            // 内联函数 fn(x) return 表达式：编成一个匿名函数 + 闭包值
            ExprKind::Lambda { params, body } => {
                let lname: Rc<str> = Rc::from("(lambda)");
                let ret = Stmt::new(StmtKind::Return(Some((**body).clone())), e.span.clone());
                let (_f, cidx, info) = self.emit_fn(&lname, params, &[ret], &e.span);
                self.emit_fn_value(cidx, &info, &e.span);
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
                        /* 长链（a + b + c + …）不能靠递归：几千项就会把栈打穿。
                         * 编译大文件时这是真实场景（地图表、拼接），所以这里把左脊摊平，
                         * 按原顺序逐段发射——左结合语义完全不变。 */
                        let mut spine: Vec<(crate::ast::Op, &Expr)> = Vec::new();
                        let mut cur: &Expr = e;
                        while let ExprKind::Bin { op: o, a, b } = &cur.kind {
                            if matches!(o, AOp::And | AOp::Or) {
                                break;
                            }
                            spine.push((*o, b.as_ref()));
                            cur = a.as_ref();
                        }
                        if spine.len() > 16 {
                            self.expr(cur);
                            for (o, rhs) in spine {
                                self.expr(rhs);
                                self.emit(binop_to_op(o));
                            }
                            return;
                        }
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
            ExprKind::Slice { obj, from, to } => {
                self.expr(obj);
                match from {
                    Some(f) => self.expr(f),
                    None => { self.emit(Op::Nil); }
                }
                match to {
                    Some(t) => self.expr(t),
                    None => { self.emit(Op::Nil); }
                }
                self.emit(Op::Slice);
            }
            ExprKind::Member { obj, name } => {
                self.expr(obj);
                let idx = self.intern(name);
                self.emit(Op::Member(idx));
            }
            ExprKind::Call { callee, args } => {
                // 带回调的 data.* 高阶函数：编成内联循环字节码，
                // 回调就是普通函数值（lambda / fn 别名 / function 名），走现成的 Call 指令。
                if let ExprKind::Var(name) = &callee.kind {
                    if is_hof(name) && self.emit_hof(name, args) {
                        return;
                    }
                }
                // obj.method(args) 走 CallMethod，其余走 Call
                if let ExprKind::Member { obj, name } = &callee.kind {
                    self.expr(obj);
                    self.emit_args(args, callee);
                    let idx = self.intern(name);
                    self.emit(Op::CallMethod(idx, args.len() as u8));
                } else {
                    /* C(...) 省略 new：TS 引擎直接报「要写 new C(…)」，这里对齐——
                     * 类名调用和函数调用长得一模一样，读代码的人分不清，强制写 new。 */
                    if let ExprKind::Var(name) = &callee.kind {
                        if self.lookup(name).is_none() {
                            let ctor: Rc<str> = Rc::from(format!("__new_{}_", name).as_str());
                            if let Some(slot) = self.lookup(&ctor) {
                                let _ = slot;
                                self.err(
                                    format!("「{}」是类名，要创建实例请写 new {}(…)", name, name),
                                    &callee.span,
                                );
                                return;
                            }
                        }
                    }
                    // f(args)：f 是全局变量时走 CallGlobal（省一条 GetGlobal，参数也不用挪位）
                    if let ExprKind::Var(name) = &callee.kind {
                        if let Some(Slot::Global(idx)) = self.lookup(name) {
                            self.emit_args(args, callee);
                            if args.len() <= 255 {
                                self.emit(Op::CallGlobal { slot: idx, nargs: args.len() as u8 });
                                return;
                            }
                        }
                    }
                    self.expr(callee);
                    self.emit_args(args, callee);
                    self.emit(Op::Call(args.len() as u8));
                }
            }
            ExprKind::New { class, args } => {
                // 类已经在解析阶段降级成 `__new_类(…)`，这里直接当普通函数调
                let ctor: Rc<str> = Rc::from(format!("__new_{}_", class).as_str());
                match self.lookup(&ctor) {
                    Some(slot) => {
                        self.load(slot);
                        for a in args {
                            self.expr(a);
                        }
                        self.emit(Op::Call(args.len() as u8));
                    }
                    None => {
                        self.err(format!("找不到类「{}」（是不是没定义？）", class), &e.span);
                        self.emit(Op::Nil);
                    }
                }
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

/// 槽位 → (是不是全局, 槽号)，给融合指令编码用。
/// Cell / Upval 没有融合指令，返回 None 让调用方回落通用路径。
fn slot_parts(s: Slot) -> Option<(bool, u16)> {
    match s {
        Slot::Local(i) => Some((false, i)),
        Slot::Global(i) => Some((true, i)),
        _ => None,
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

/// 带回调、需要展开成内联循环的 data.* 高阶函数
fn is_hof(n: &str) -> bool {
    matches!(
        n,
        "data.map" | "data.filter" | "data.reduce" | "data.groupby" | "data.index_by"
            | "data.any" | "data.all" | "data.find" | "data.find_index"
    )
}

fn binop_to_op(op: AOp) -> Op {
    match op {
        AOp::Add => Op::Add,
        AOp::Sub => Op::Sub,
        AOp::Mul => Op::Mul,
        AOp::Div => Op::Div,
        AOp::IDiv => Op::IDiv,
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
        // 这两个不参与二元运算，占位（上面专门处理）
        AOp::Addr | AOp::Deref => Op::Add,
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

