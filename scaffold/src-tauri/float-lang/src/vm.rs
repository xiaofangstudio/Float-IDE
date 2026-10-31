//! 虚拟机：一个大的 `match` 循环执行字节码。
//!
//! 关键设计：
//! * **操作数栈和全局槽都是裸指针**，栈顶下标 `sp` 常驻寄存器。
//!   这是解释器最要紧的一步：如果走 `Vec::push`，编译器分不清哪次调用会改到
//!   `self`，每取一个字段都要重新从内存加载，单条指令能贵一倍。
//! * **局部变量直接放在值栈上**，帧只记 base；`GetLocal(i)` 就是 `stack[base+i]`
//! * 整数走 `Int` 分支，不碰浮点单元；只有混算才提升成 `Float`
//! * `for` 计数循环有专用指令（`RangeInit` / `RangeNext`），每圈只剩一条循环指令
//! * `s += i`、`n - 1`、`x > 0` 这类高频写法在编译期融成单条指令
//! * 字符串 `+=` 在引用独占时就地追加，5 万次拼接从 O(n²) 降到 O(n)
//! * 递归/死循环有硬上限，脚本写错不会把 IDE 拖死
//!
//! 这一层刻意不做 JIT。目标是"比 CPython 快一截"，
//! 靠专用指令 + 少调度 + 吃透常见模式，而不是运行时编译。

use crate::builtins::{call_builtin, call_method};
use crate::bytecode::{Chunk, Op};
use crate::error::{FloatError, Span};
use crate::value::{Dict, Value};
use std::rc::Rc;

/// 调用栈深度上限。超过就报「递归太深」，而不是让进程爆栈。
const MAX_DEPTH: usize = 400;
/// 单次运行最多执行多少条指令，防死循环
const DEFAULT_MAX_STEPS: u64 = 200_000_000;
/// 每执行这么多条指令才查一次步数上限（每条都查太浪费）
const FUEL: u64 = 1 << 20;
/// 操作数栈容量（值个数）。400 层递归 × 每层几十个局部变量，远用不满。
/// 一条指令最多压 1 个值，`Call` 补默认参数最多 255 个，所以留 256 格余量就够。
const STACK_CAP: usize = 1 << 16;
const STACK_MARGIN: usize = 256;
/// 栈底留的缓冲区。槽位全部预置成 null，栈顶从 PAD 起步——
/// 万一字节码算错了栈高度，最坏是读到 null，而不是越界访存。
const STACK_PAD: usize = 64;

/// 调用帧。字段全是小整数：fib 这种一秒几百万次调用的场景，
/// 帧多大就是成倍的差别（塞个 Rc<str> 名字进来，帧从 28 字涨到 72 字节，
/// 每帧还要多一次引用计数）。
struct Frame {
    /// 这一帧结束后栈要截断到哪（就是 callee 那一格的地址）
    base: u32,
    /// 局部变量起点（callee 后一格）
    locals_base: u32,
    ret_ip: u32,
    /// 调用发生在哪条指令之后（报错取行号用）
    call_ip: u32,
    /// 进函数时迭代器栈 / 计数循环栈的深度，返回时截断回去
    iter_base: u32,
    range_base: u32,
    /// 函数在 chunk.fn_names 里的编号（报错的调用链要用）
    fn_id: u16,
    /// 这次调用实际传了几个实参（默认值打桩要用）
    argc: u8,
}

/// 手动内存的登记：alloc 记一笔，free 划掉，程序结束时对账。
/// 用户要求"跟 Rust 一样严格"：申请了没还，就不让程序正常结束。
struct AllocRec {
    id: usize,
    line: u32,
    cells: usize,
    freed: bool,
}

/// 调用点的内联缓存：记住上次在这里调到的是哪个函数对象，
/// 命中就不必再去读 UserFn 里的 entry/nlocals/nparams（省一次解引用）。
/// 函数对象换了（重新赋值）时指针对不上，自动回落并刷新。
#[derive(Clone, Copy, Default)]
struct CallIc {
    fp: usize,
    entry: u32,
    nlocals: u32,
    nparams: u32,
    fn_id: u16,
}

/// 迭代源。以前是直接把整个列表 clone 一份再遍历——百万元素就是百万次复制。
/// 现在只留个引用，按需取第 i 个。
enum IterSrc {
    List(crate::value::ListRef),
    Dict(crate::value::DictRef),
    Ptr(crate::value::PtrRef),
    Owned(Vec<Value>),
}

impl IterSrc {
    fn get(&self, i: usize) -> Option<Value> {
        match self {
            IterSrc::List(l) => l.borrow().get(i).cloned(),
            IterSrc::Owned(v) => v.get(i).cloned(),
            IterSrc::Dict(d) => {
                let d = d.borrow();
                d.keys().get(i).map(|k| Value::string(k.to_string()))
            }
            IterSrc::Ptr(p) => {
                let m = p.borrow();
                if m.freed { None } else { m.buf.get(i).cloned() }
            }
        }
    }
}

struct IterState {
    src: IterSrc,
    len: usize,
    pos: usize,
}

/// 计数循环的运行时状态。整数走整数，混了浮点才退化成浮点比较。
/// `inclusive` 是 `for i = 1 to 10` 这种含端点的写法；必须逐圈判断，
/// 不能提前把终点加减一个步长（步长不是 1 时会多跑一圈）。
enum RangeState {
    Int { i: i64, end: i64, step: i64, inclusive: bool },
    Float { i: f64, end: f64, step: f64, inclusive: bool },
}

struct RangeLoop {
    state: RangeState,
    var: u16,
    is_global: bool,
}

/// 操作数栈：容量固定，用裸指针访问，Drop 时把真实用量同步回 Vec。
struct OpStack {
    buf: Vec<Value>,
    sp: usize,
}

impl OpStack {
    fn new() -> OpStack {
        // 整块初始化成 null：len 固定，栈顶靠 sp 走，
        // 好处是不用 set_len 收尾，也不会碰到未初始化内存。
        OpStack { buf: vec![Value::Null; STACK_PAD + STACK_CAP], sp: STACK_PAD }
    }
}

/// 保证任何返回路径都把 sp 同步回 OpStack（包括 `?` 提前返回）
struct SpSync {
    stack: *mut OpStack,
    sp: *mut usize,
}

impl Drop for SpSync {
    fn drop(&mut self) {
        unsafe { (*self.stack).sp = *self.sp };
    }
}

#[derive(Debug)]
pub struct RunOutcome {
    pub output: String,
    /// `报告 xxx` 的内容（IDE 右侧面板用）
    pub reports: Vec<String>,
    pub value: Value,
    pub steps: u64,
}

pub struct Vm {
    chunk: Chunk,
    globals: Vec<Value>,
    stack: OpStack,
    frames: Vec<Frame>,
    iters: Vec<IterState>,
    ranges: Vec<RangeLoop>,
    /// 每个指令位置一格调用缓存
    call_ic: Vec<CallIc>,
    /// 手动内存对账表
    allocs: Vec<AllocRec>,
    /// 报告通道：`报告 xxx` 写这里，和 print 分开走
    reports: Vec<String>,
    out: String,
    steps: u64,
    max_steps: u64,
    /// 预设输入（IDE 里接输入框，测试里喂脚本化输入）
    inputs: Vec<String>,
    input_pos: usize,
}

impl Vm {
    pub fn new(chunk: Chunk) -> Vm {
        let n = chunk.n_globals as usize;
        let chunk_len = chunk.code.len();
        Vm {
            chunk,
            globals: vec![Value::Null; n],
            stack: OpStack::new(),
            frames: Vec::new(),
            iters: Vec::new(),
            ranges: Vec::new(),
            call_ic: vec![CallIc::default(); chunk_len],
            allocs: Vec::new(),
            reports: Vec::new(),
            out: String::new(),
            steps: 0,
            max_steps: DEFAULT_MAX_STEPS,
            inputs: Vec::new(),
            input_pos: 0,
        }
    }

    pub fn set_max_steps(&mut self, n: u64) {
        self.max_steps = n;
    }

    /// alloc 之后登记一笔（记下是哪一行要的、要了几格）
    fn note_alloc(&mut self, v: &Value, ip: usize) {
        if let Value::Ptr(p) = v {
            let id = Rc::as_ptr(p) as usize;
            let cells = p.borrow().buf.len();
            let line = self.line_of(ip);
            self.allocs.push(AllocRec { id, line, cells, freed: false });
        }
    }

    /// free 之后划掉
    fn note_free(&mut self, v: &Value) {
        if let Value::Ptr(p) = v {
            let id = Rc::as_ptr(p) as usize;
            if let Some(r) = self.allocs.iter_mut().find(|r| r.id == id && !r.freed) {
                r.freed = true;
            }
        }
    }

    /// 程序结束时对账：还有没 free 的就报错，不让它"正常跑完"
    fn check_leaks(&self) -> Result<(), FloatError> {
        let mut leaks: Vec<&AllocRec> = self.allocs.iter().filter(|r| !r.freed).collect();
        if leaks.is_empty() {
            return Ok(());
        }
        let n = leaks.len();
        leaks.sort_by_key(|r| r.line);
        let mut msg = String::new();
        if n == 1 {
            let r = leaks[0];
            msg.push_str(&format!(
                "第 {} 行申请的 {} 格内存还没有 free",
                r.line, r.cells
            ));
        } else {
            msg.push_str(&format!("有 {} 块手动申请的内存没有 free：", n));
            for r in leaks.iter().take(4) {
                msg.push_str(&format!("\n    第 {} 行申请了 {} 格", r.line, r.cells));
            }
            if n > 4 {
                msg.push_str(&format!("\n    ……还有 {} 块", n - 4));
            }
        }
        msg.push_str("\n  手动申请的内存必须自己还回去；不想管内存就别用 alloc，用列表就行。");
        Ok(()).and(Err(FloatError::new(msg, Span::new(&self.chunk.file, 1, 1, 1))))
    }

    /// 取走目前累积的输出（出错时也想知道跑到哪了）
    pub fn take_output(&mut self) -> String {
        std::mem::take(&mut self.out)
    }

    /// 预置输入队列：脚本里的 ask 依次取用，取完给空串。
    pub fn set_inputs(&mut self, inputs: Vec<String>) {
        self.inputs = inputs;
        self.input_pos = 0;
    }

    /// 出错时取某条指令对应的源码行
    /// 帧上的函数名（查函数名表；主程序没有编号）
    fn frame_name(&self, f: &Frame) -> String {
        if f.fn_id == u16::MAX {
            return "主程序".to_string();
        }
        self.chunk
            .fn_names
            .get(f.fn_id as usize)
            .map(|n| n.to_string())
            .unwrap_or_else(|| "?".to_string())
    }

    fn line_of(&self, ip: usize) -> u32 {
        if ip == 0 {
            return 0;
        }
        self.chunk.lines.get(ip - 1).copied().unwrap_or(0)
    }

    /// 拼一条中文报错：**第几行** + 原因 + 调用链。
    ///
    /// 用户说过"报错也要是中文"，所以这里不出现英文单词，调用链也按
    /// 「谁在第几行调了谁」的中文顺序排。
    ///
    /// `ip` 就是出错那条指令的位置：只在出错路径上取，主循环里不用每轮记一次，
    /// 不然光维护这个字段就能把 fib 拖慢三成。
    fn err_at(&self, ip: usize, msg: impl Into<String>) -> FloatError {
        let mut msg = msg.into();
        let line = self.line_of(ip);
        if line > 0 {
            msg = format!("第 {} 行：{}", line, msg);
        }
        // 调用链：由外到内，"谁在第几行调了下一个"
        let mut trace = Vec::new();
        let mut chain = Vec::new();
        for i in 1..self.frames.len() {
            let callee = &self.frames[i];
            let caller = &self.frames[i - 1];
            let at = self.line_of(callee.call_ip as usize);
            trace.push((Rc::clone(&self.chunk.file), at));
            if at > 0 {
                chain.push(format!("{} 第 {} 行", self.frame_name(caller), at));
            } else {
                chain.push(self.frame_name(caller));
            }
        }
        if !chain.is_empty() {
            let last = self.frames.last().map(|f| self.frame_name(f)).unwrap_or_default();
            msg.push_str(&format!("\n  调用链：{} → {}", chain.join(" → "), last));
        }
        let mut e = FloatError::new(msg, Span::new(&self.chunk.file, line.max(1), 1, 1));
        e.trace = trace;
        e
    }

    pub fn run(&mut self) -> Result<RunOutcome, FloatError> {
        self.globals = vec![Value::Null; self.chunk.n_globals as usize];
        self.stack.sp = STACK_PAD;
        self.frames.clear();
        self.iters.clear();
        self.ranges.clear();
        self.allocs.clear();
        self.reports.clear();
        self.out.clear();
        self.steps = 0;
        self.input_pos = 0;
        self.frames.push(Frame {
            base: 0,
            locals_base: 0,
            ret_ip: 0,
            call_ip: 0,
            iter_base: 0,
            range_base: 0,
            fn_id: u16::MAX,
            argc: 0,
        });
        unsafe { self.exec() }
    }

    /// 执行主体。全程用裸指针读写操作数栈和全局槽。
    ///
    /// 安全性：sp 只在 [0, STACK_CAP) 内移动（每轮开头统一检查余量 256 格），
    /// 槽号来自编译期，全局槽长度在 reset 后不变。
    unsafe fn exec(&mut self) -> Result<RunOutcome, FloatError> {
        let sptr = self.stack.buf.as_mut_ptr();
        let gptr = self.globals.as_mut_ptr();
        let glen = self.globals.len();
        // 指令区在整个运行期不变，直接拿裸指针取指，省掉每条指令的边界检查。
        // 安全性：ip 只会来自编译期生成的跳转目标 / 返回地址，全部落在 code 内。
        let code = self.chunk.code.as_ptr();
        let ncode = self.chunk.code.len();
        // 调用缓存也用裸指针：热路径上不能有 Vec 的边界检查
        let icp = self.call_ic.as_mut_ptr();
        let kptr = self.chunk.consts.as_ptr();
        let klen = self.chunk.consts.len();

        let mut ip = 0usize;
        let mut sp = STACK_PAD;
        // 局部变量基址缓存：GetLocal/SetLocal 是最高频指令，不能每次都查 frames
        let mut base = 0usize;
        let mut fuel = FUEL;
        let _sync = SpSync { stack: &mut self.stack as *mut OpStack, sp: &mut sp as *mut usize };

        loop {
            debug_assert!(ip < ncode);
            // 一条指令最多压 1 个值（Call 补默认参数最多 255 个），
            // 所以每轮开头统一检查余量，后面所有 push 都不用再判越界
            if sp > STACK_PAD + STACK_CAP - STACK_MARGIN {
                return Err(self.err_at(ip, "操作数栈满了：表达式嵌套或参数太多"));
            }
            let op = *code.add(ip);
            ip += 1;

            match op {
                Op::Const(i) => {
                    let cv = if (i as usize) < klen { (*kptr.add(i as usize)).clone() } else { Value::Null };
                    std::ptr::write(sptr.add(sp), cv);
                    sp += 1;
                }
                Op::Nil => {
                    std::ptr::write(sptr.add(sp), Value::Null);
                    sp += 1;
                }
                Op::True => {
                    std::ptr::write(sptr.add(sp), Value::Bool(true));
                    sp += 1;
                }
                Op::False => {
                    std::ptr::write(sptr.add(sp), Value::Bool(false));
                    sp += 1;
                }
                Op::Pop => {
                    sp -= 1;
                    std::ptr::drop_in_place(sptr.add(sp));
                }
                Op::Dup => {
                    let v = if sp > 0 { (*sptr.add(sp - 1)).clone() } else { Value::Null };
                    std::ptr::write(sptr.add(sp), v);
                    sp += 1;
                }
                Op::GetLocal(i) => {
                    let idx = base + i as usize;
                    let v = if idx < sp { (*sptr.add(idx)).clone() } else { Value::Null };
                    std::ptr::write(sptr.add(sp), v);
                    sp += 1;
                }
                Op::SetLocal(i) => {
                    sp -= 1;
                    let v = std::ptr::read(sptr.add(sp));
                    let idx = base + i as usize;
                    if idx < sp {
                        *sptr.add(idx) = v;
                    }
                }
                Op::GetGlobal(i) => {
                    let idx = i as usize;
                    let v = if idx < glen { (*gptr.add(idx)).clone() } else { Value::Null };
                    std::ptr::write(sptr.add(sp), v);
                    sp += 1;
                }
                Op::SetGlobal(i) => {
                    sp -= 1;
                    let v = std::ptr::read(sptr.add(sp));
                    if (i as usize) < glen {
                        *gptr.add(i as usize) = v;
                    }
                }
                Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Mod | Op::Pow => {
                    if sp < 2 {
                        return Err(self.err_at(ip, "运算缺少操作数"));
                    }
                    // 整数直通：这是脚本里 90% 的运算，原地替换栈顶两格
                    if let (Value::Int(x), Value::Int(y)) = (&*sptr.add(sp - 2), &*sptr.add(sp - 1)) {
                        let (x, y) = (*x, *y);
                        let fast = match op {
                            Op::Add => x.checked_add(y).map(Value::Int),
                            Op::Sub => x.checked_sub(y).map(Value::Int),
                            Op::Mul => x.checked_mul(y).map(Value::Int),
                            Op::Div if y != 0 && x % y == 0 => Some(Value::Int(x / y)),
                            Op::Mod if y != 0 => Some(Value::Int(x % y)),
                            _ => None,
                        };
                        if let Some(v) = fast {
                            sp -= 1;
                            *sptr.add(sp - 1) = v;
                            continue;
                        }
                    }
                    sp -= 1;
                    let b = std::ptr::read(sptr.add(sp));
                    sp -= 1;
                    let a = std::ptr::read(sptr.add(sp));
                    let v = match arith(op, &a, &b) {
                        Ok(v) => v,
                        Err(m) => return Err(self.err_at(ip, m)),
                    };
                    std::ptr::write(sptr.add(sp), v);
                    sp += 1;
                }
                Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge => {
                    if sp < 2 {
                        return Err(self.err_at(ip, "比较缺少操作数"));
                    }
                    if let (Value::Int(x), Value::Int(y)) = (&*sptr.add(sp - 2), &*sptr.add(sp - 1)) {
                        let (x, y) = (*x, *y);
                        let v = match op {
                            Op::Eq => x == y,
                            Op::Ne => x != y,
                            Op::Lt => x < y,
                            Op::Le => x <= y,
                            Op::Gt => x > y,
                            Op::Ge => x >= y,
                            _ => false,
                        };
                        sp -= 1;
                        *sptr.add(sp - 1) = Value::Bool(v);
                        continue;
                    }
                    sp -= 1;
                    let b = std::ptr::read(sptr.add(sp));
                    sp -= 1;
                    let a = std::ptr::read(sptr.add(sp));
                    let v = match compare(op, &a, &b) {
                        Ok(v) => v,
                        Err(m) => return Err(self.err_at(ip, m)),
                    };
                    std::ptr::write(sptr.add(sp), Value::Bool(v));
                    sp += 1;
                }
                Op::BitAnd | Op::BitOr | Op::BitXor | Op::Shl | Op::Shr => {
                    sp -= 1;
                    let b = std::ptr::read(sptr.add(sp));
                    sp -= 1;
                    let a = std::ptr::read(sptr.add(sp));
                    let x = match a.as_int() {
                        Some(v) => v,
                        None => return Err(self.err_at(ip, "位运算需要整数")),
                    };
                    let y = match b.as_int() {
                        Some(v) => v,
                        None => return Err(self.err_at(ip, "位运算需要整数")),
                    };
                    let r = match op {
                        Op::BitAnd => x & y,
                        Op::BitOr => x | y,
                        Op::BitXor => x ^ y,
                        Op::Shl => x << (y & 63),
                        Op::Shr => x >> (y & 63),
                        _ => unreachable!(),
                    };
                    std::ptr::write(sptr.add(sp), Value::Int(r));
                    sp += 1;
                }
                Op::Neg => {
                    sp -= 1;
                    let a = std::ptr::read(sptr.add(sp));
                    let v = match a {
                        Value::Int(i) => Value::Int(-i),
                        Value::Float(f) => Value::Float(-f),
                        ref other => match other.as_num() {
                            Some(n) => Value::Float(-n),
                            None => return Err(self.err_at(ip, "取负需要数字")),
                        },
                    };
                    std::ptr::write(sptr.add(sp), v);
                    sp += 1;
                }
                Op::Not => {
                    sp -= 1;
                    let a = std::ptr::read(sptr.add(sp));
                    std::ptr::write(sptr.add(sp), Value::Bool(!a.truthy()));
                    sp += 1;
                }
                Op::BitNot => {
                    sp -= 1;
                    let a = std::ptr::read(sptr.add(sp));
                    let x = match a.as_int() {
                        Some(v) => v,
                        None => return Err(self.err_at(ip, "按位取反需要整数")),
                    };
                    std::ptr::write(sptr.add(sp), Value::Int(!x));
                    sp += 1;
                }
                Op::JumpIfArgc { n, target } => {
                    let argc = self.frames.last().map(|f| f.argc as usize).unwrap_or(0);
                    if argc >= n as usize {
                        ip = target as usize;
                    }
                }
                Op::Jump(t) => {
                    // 往回跳才是循环；死循环保护只在这里查，省掉每条指令的开销
                    if (t as usize) <= ip {
                        fuel -= 1;
                        if fuel == 0 {
                            self.steps += FUEL;
                            if self.steps > self.max_steps {
                                return Err(self.err_at(ip, "执行步数超限，可能是死循环"));
                            }
                            fuel = FUEL;
                        }
                    }
                    ip = t as usize;
                }
                Op::JumpIfFalse(t) => {
                    sp -= 1;
                    let v = std::ptr::read(sptr.add(sp));
                    if !v.truthy() {
                        ip = t as usize;
                    }
                }
                Op::JumpIfFalseKeep(t) => {
                    if !(sp > 0 && (*sptr.add(sp - 1)).truthy()) {
                        ip = t as usize;
                    }
                }
                Op::JumpIfTrueKeep(t) => {
                    if sp > 0 && (*sptr.add(sp - 1)).truthy() {
                        ip = t as usize;
                    }
                }
                /* ── 融合的变量复合赋值 ── */
                Op::CompoundVar { bop, dst, src, flags } => {
                    let dg = flags & 1 != 0;
                    let sg = flags & 2 != 0;
                    let d = dst as usize;
                    let b = if sg {
                        if (src as usize) < glen { (*gptr.add(src as usize)).clone() } else { Value::Null }
                    } else {
                        let idx = base + src as usize;
                        if idx < sp { (*sptr.add(idx)).clone() } else { Value::Null }
                    };
                    // 先把目标值从槽位取出来（槽位暂时置空），
                    // 这样字符串引用计数为 1 时就能就地追加。
                    let a = if dg {
                        if d < glen { std::mem::replace(&mut *gptr.add(d), Value::Null) } else { Value::Null }
                    } else {
                        let idx = base + d;
                        if idx < sp {
                            std::mem::replace(&mut *sptr.add(idx), Value::Null)
                        } else {
                            Value::Null
                        }
                    };
                    // 整数快路径写在最前面，不进函数调用
                    let fast = if let (Value::Int(x), Value::Int(y)) = (&a, &b) {
                        let (x, y) = (*x, *y);
                        match bop {
                            0 => x.checked_add(y).map(Value::Int),
                            1 => x.checked_sub(y).map(Value::Int),
                            2 => x.checked_mul(y).map(Value::Int),
                            3 if y != 0 && x % y == 0 => Some(Value::Int(x / y)),
                            4 if y != 0 => Some(Value::Int(x % y)),
                            _ => None,
                        }
                    } else {
                        None
                    };
                    let v = match fast {
                        Some(v) => v,
                        None => match compound_value(bop, a, &b) {
                            Ok(v) => v,
                            Err(m) => return Err(self.err_at(ip, m)),
                        },
                    };
                    if dg {
                        if d < glen {
                            *gptr.add(d) = v;
                        }
                    } else {
                        let idx = base + d;
                        if idx < sp {
                            *sptr.add(idx) = v;
                        }
                    }
                }
                Op::CompoundVarConst { bop, dst, flags, k } => {
                    let dg = flags & 1 != 0;
                    let d = dst as usize;
                    let b = if (k as usize) < klen { (*kptr.add(k as usize)).clone() } else { Value::Null };
                    let a = if dg {
                        if d < glen { std::mem::replace(&mut *gptr.add(d), Value::Null) } else { Value::Null }
                    } else {
                        let idx = base + d;
                        if idx < sp {
                            std::mem::replace(&mut *sptr.add(idx), Value::Null)
                        } else {
                            Value::Null
                        }
                    };
                    let fast = if let (Value::Int(x), Value::Int(y)) = (&a, &b) {
                        let (x, y) = (*x, *y);
                        match bop {
                            0 => x.checked_add(y).map(Value::Int),
                            1 => x.checked_sub(y).map(Value::Int),
                            2 => x.checked_mul(y).map(Value::Int),
                            3 if y != 0 && x % y == 0 => Some(Value::Int(x / y)),
                            4 if y != 0 => Some(Value::Int(x % y)),
                            _ => None,
                        }
                    } else {
                        None
                    };
                    let v = match fast {
                        Some(v) => v,
                        None => match compound_value(bop, a, &b) {
                            Ok(v) => v,
                            Err(m) => return Err(self.err_at(ip, m)),
                        },
                    };
                    if dg {
                        if d < glen {
                            *gptr.add(d) = v;
                        }
                    } else {
                        let idx = base + d;
                        if idx < sp {
                            *sptr.add(idx) = v;
                        }
                    }
                }
                Op::IncVar { slot, flags, delta } => {
                    let g = flags & 1 != 0;
                    let bad = if g {
                        let idx = slot as usize;
                        if idx < glen {
                            !inc_value(&mut *gptr.add(idx), delta)
                        } else {
                            true
                        }
                    } else {
                        let idx = base + slot as usize;
                        if idx < sp {
                            !inc_value(&mut *sptr.add(idx), delta)
                        } else {
                            true
                        }
                    };
                    if bad {
                        return Err(self.err_at(ip, "自增自减需要数字"));
                    }
                }
                Op::BinLocalLocal { op, a, aflags, b, bflags } => {
                    let av = if aflags & 1 != 0 {
                        let i = a as usize;
                        if i < glen { (*gptr.add(i)).clone() } else { Value::Null }
                    } else {
                        let i = base + a as usize;
                        if i < sp { (*sptr.add(i)).clone() } else { Value::Null }
                    };
                    let bv = if bflags & 1 != 0 {
                        let i = b as usize;
                        if i < glen { (*gptr.add(i)).clone() } else { Value::Null }
                    } else {
                        let i = base + b as usize;
                        if i < sp { (*sptr.add(i)).clone() } else { Value::Null }
                    };
                    if let (Value::Int(x), Value::Int(y)) = (&av, &bv) {
                        let (x, y) = (*x, *y);
                        let fast = match op {
                            0 => x.checked_add(y).map(Value::Int),
                            1 => x.checked_sub(y).map(Value::Int),
                            2 => x.checked_mul(y).map(Value::Int),
                            3 if y != 0 && x % y == 0 => Some(Value::Int(x / y)),
                            4 if y != 0 => Some(Value::Int(x % y)),
                            6 => Some(Value::Bool(x == y)),
                            7 => Some(Value::Bool(x != y)),
                            8 => Some(Value::Bool(x < y)),
                            9 => Some(Value::Bool(x <= y)),
                            10 => Some(Value::Bool(x > y)),
                            11 => Some(Value::Bool(x >= y)),
                            _ => None,
                        };
                        if let Some(v) = fast {
                            std::ptr::write(sptr.add(sp), v);
                            sp += 1;
                            continue;
                        }
                    }
                    let v = if op <= 5 {
                        match arith(bop_to_op(op).unwrap_or(Op::Add), &av, &bv) {
                            Ok(v) => v,
                            Err(m) => return Err(self.err_at(ip, m)),
                        }
                    } else {
                        match compare(cmp_of(op), &av, &bv) {
                            Ok(v) => Value::Bool(v),
                            Err(m) => return Err(self.err_at(ip, m)),
                        }
                    };
                    std::ptr::write(sptr.add(sp), v);
                    sp += 1;
                }
                Op::BinLocalK { op, slot, flags, k } => {
                    let a = if flags & 1 != 0 {
                        let idx = slot as usize;
                        if idx < glen { (*gptr.add(idx)).clone() } else { Value::Null }
                    } else {
                        let idx = base + slot as usize;
                        if idx < sp { (*sptr.add(idx)).clone() } else { Value::Null }
                    };
                    let b = if (k as usize) < klen { (*kptr.add(k as usize)).clone() } else { Value::Null };
                    if let (Value::Int(x), Value::Int(y)) = (&a, &b) {
                        let (x, y) = (*x, *y);
                        let fast = match op {
                            0 => x.checked_add(y).map(Value::Int),
                            1 => x.checked_sub(y).map(Value::Int),
                            2 => x.checked_mul(y).map(Value::Int),
                            3 if y != 0 && x % y == 0 => Some(Value::Int(x / y)),
                            4 if y != 0 => Some(Value::Int(x % y)),
                            6 => Some(Value::Bool(x == y)),
                            7 => Some(Value::Bool(x != y)),
                            8 => Some(Value::Bool(x < y)),
                            9 => Some(Value::Bool(x <= y)),
                            10 => Some(Value::Bool(x > y)),
                            11 => Some(Value::Bool(x >= y)),
                            _ => None,
                        };
                        if let Some(v) = fast {
                            std::ptr::write(sptr.add(sp), v);
                            sp += 1;
                            continue;
                        }
                    }
                    let v = if op <= 5 {
                        match arith(bop_to_op(op).unwrap_or(Op::Add), &a, &b) {
                            Ok(v) => v,
                            Err(m) => return Err(self.err_at(ip, m)),
                        }
                    } else {
                        match compare(cmp_of(op), &a, &b) {
                            Ok(v) => Value::Bool(v),
                            Err(m) => return Err(self.err_at(ip, m)),
                        }
                    };
                    std::ptr::write(sptr.add(sp), v);
                    sp += 1;
                }
                Op::Call(nargs) => {
                    let n = nargs as usize;
                    if sp < n + 1 {
                        return Err(self.err_at(ip, "调用缺少函数或参数"));
                    }
                    let callee_idx = sp - n - 1;
                    // 先只看类型再决定怎么调，避免为了判断类型就克隆一次 Rc
                    let native_name = match &*sptr.add(callee_idx) {
                        Value::Native(nm) => Some(*nm),
                        _ => None,
                    };
                    if let Some(name) = native_name {
                        let args = take_tail(sptr, &mut sp, callee_idx + 1);
                        sp -= 1; // 摘掉 callee
                        std::ptr::drop_in_place(sptr.add(sp));
                        let v = match call_builtin(name, &args) {
                            Ok(v) => v,
                            Err(m) => return Err(self.err_at(ip, m)),
                        };
                        // 手动内存要记账：alloc 记一笔，free 划掉
                        if name == "alloc" {
                            self.note_alloc(&v, ip);
                        } else if name == "free" {
                            if let Some(a) = args.first() {
                                self.note_free(a);
                            }
                        }
                        std::ptr::write(sptr.add(sp), v);
                        sp += 1;
                    } else if matches!(&*sptr.add(callee_idx), Value::Func(_)) {
                        let site = ip - 1;
                        let fp = match &*sptr.add(callee_idx) {
                            Value::Func(f) => Rc::as_ptr(f) as usize,
                            _ => 0,
                        };
                        let ic = *icp.add(site);
                        let (fid, entry, nlocals, nparams) = if fp != 0 && ic.fp == fp {
                            (ic.fn_id, ic.entry as usize, ic.nlocals as usize, ic.nparams as usize)
                        } else {
                            let (fid, entry, nlocals, nparams) = match &*sptr.add(callee_idx) {
                                Value::Func(f) => (f.id, f.entry, f.nlocals, f.params.len()),
                                _ => unreachable!(),
                            };
                            *icp.add(site) = CallIc {
                                fp,
                                entry: entry as u32,
                                nlocals: nlocals as u32,
                                nparams: nparams as u32,
                                fn_id: fid,
                            };
                            (fid, entry, nlocals, nparams)
                        };
                        if self.frames.len() >= MAX_DEPTH {
                            let name = match &*sptr.add(callee_idx) {
                                Value::Func(f) => f.name.to_string(),
                                _ => "?".to_string(),
                            };
                            return Err(self.err_at(ip, format!(
                                "递归太深了（超过 {} 层）停了下来。检查一下 {} 的结束条件是不是写错了。",
                                MAX_DEPTH, name
                            )));
                        }
                        if n > nparams {
                            drop_from(sptr, &mut sp, callee_idx + 1 + nparams);
                        }
                        // 参数不足补默认值
                        if n < nparams {
                            if let Value::Func(f) = &*sptr.add(callee_idx) {
                                for k in n..nparams {
                                    std::ptr::write(sptr.add(sp), f.defaults.get(k).cloned().unwrap_or(Value::Null));
                                    sp += 1;
                                }
                            }
                        }
                        // callee 那一格留着不挪：返回时 drop_from(base) 会连它一起收掉，
                        // 省掉一次参数整体搬移
                        let newbase = callee_idx + 1;
                        let want = nlocals.max(nparams);
                        while sp < newbase + want {
                            std::ptr::write(sptr.add(sp), Value::Null);
                            sp += 1;
                        }
                        self.frames.push(Frame {
                            base: callee_idx as u32,
                            locals_base: newbase as u32,
                            ret_ip: ip as u32,
                            call_ip: ip as u32,
                            iter_base: self.iters.len() as u32,
                            range_base: self.ranges.len() as u32,
                            fn_id: fid,
                            argc: n.min(nparams) as u8,
                        });
                        base = newbase;
                        ip = entry;
                    } else {
                        let shown = (*sptr.add(callee_idx)).to_display();
                        return Err(self.err_at(ip, format!("{} 不是一个函数", shown)));
                    }
                }
                Op::CallMethod(name_idx, nargs) => {
                    let name = self.chunk.name_at(name_idx);
                    let n = nargs as usize;
                    let mut args = Vec::with_capacity(n);
                    for _ in 0..n {
                        sp -= 1;
                        args.push(std::ptr::read(sptr.add(sp)));
                    }
                    args.reverse();
                    sp -= 1;
                    let recv = std::ptr::read(sptr.add(sp));
                    let v = match call_method(&recv, &name, &args) {
                        Ok(v) => v,
                        Err(m) => return Err(self.err_at(ip, m)),
                    };
                    if &*name == "free" {
                        self.note_free(&recv);
                    }
                    std::ptr::write(sptr.add(sp), v);
                    sp += 1;
                }
                Op::Return => {
                    sp -= 1;
                    let v = std::ptr::read(sptr.add(sp));
                    match leave_frame(self, sptr, &mut sp, &mut base) {
                        FrameExit::Cont(rip) => {
                            std::ptr::write(sptr.add(sp), v);
                            sp += 1;
                            ip = rip;
                        }
                        FrameExit::Top => {
                            // 顶层结束 = 程序跑完。手动内存没还对上账就不算跑完。
                            self.check_leaks()?;
                            return Ok(RunOutcome {
                                output: std::mem::take(&mut self.out),
                                reports: std::mem::take(&mut self.reports),
                                value: v,
                                steps: self.steps,
                            })
                        }
                    }
                }
                Op::ReturnLocal { slot, flags } => {
                    let idx = slot as usize;
                    let v = if flags & 1 != 0 {
                        if idx < glen { (*gptr.add(idx)).clone() } else { Value::Null }
                    } else {
                        let at = base + idx;
                        if at < sp { (*sptr.add(at)).clone() } else { Value::Null }
                    };
                    match leave_frame(self, sptr, &mut sp, &mut base) {
                        FrameExit::Cont(rip) => {
                            std::ptr::write(sptr.add(sp), v);
                            sp += 1;
                            ip = rip;
                        }
                        FrameExit::Top => {
                            // 顶层结束 = 程序跑完。手动内存没还对上账就不算跑完。
                            self.check_leaks()?;
                            return Ok(RunOutcome {
                                output: std::mem::take(&mut self.out),
                                reports: std::mem::take(&mut self.reports),
                                value: v,
                                steps: self.steps,
                            })
                        }
                    }
                }
                Op::CallGlobal { slot, nargs } => {
                    let n = nargs as usize;
                    if sp < n {
                        return Err(self.err_at(ip, "调用缺少参数"));
                    }
                    // 参数已经就位在 [sp-n, sp)，callee 从全局槽读，
                    // 所以整段调用一次搬移都不用做
                    let callee_idx = sp - n;
                    let kind = if (slot as usize) < glen {
                        match &*gptr.add(slot as usize) {
                            Value::Func(_) => 1u8,
                            Value::Native(_) => 0u8,
                            _ => 2u8,
                        }
                    } else {
                        2u8
                    };
                    if kind == 0 {
                        let name = match &*gptr.add(slot as usize) {
                            Value::Native(nm) => *nm,
                            _ => unreachable!(),
                        };
                        let args = take_tail(sptr, &mut sp, callee_idx);
                        let v = match call_builtin(name, &args) {
                            Ok(v) => v,
                            Err(m) => return Err(self.err_at(ip, m)),
                        };
                        // 手动内存要记账：alloc 记一笔，free 划掉
                        if name == "alloc" {
                            self.note_alloc(&v, ip);
                        } else if name == "free" {
                            if let Some(a) = args.first() {
                                self.note_free(a);
                            }
                        }
                        std::ptr::write(sptr.add(sp), v);
                        sp += 1;
                    } else if kind == 1 {
                        let site = ip - 1;
                        let fp = match &*gptr.add(slot as usize) {
                            Value::Func(f) => Rc::as_ptr(f) as usize,
                            _ => 0,
                        };
                        let ic = *icp.add(site);
                        let (fid, entry, nlocals, nparams) = if fp != 0 && ic.fp == fp {
                            (ic.fn_id, ic.entry as usize, ic.nlocals as usize, ic.nparams as usize)
                        } else {
                            let (fid, entry, nlocals, nparams) = match &*gptr.add(slot as usize) {
                                Value::Func(f) => (f.id, f.entry, f.nlocals, f.params.len()),
                                _ => unreachable!(),
                            };
                            *icp.add(site) = CallIc {
                                fp,
                                entry: entry as u32,
                                nlocals: nlocals as u32,
                                nparams: nparams as u32,
                                fn_id: fid,
                            };
                            (fid, entry, nlocals, nparams)
                        };
                        if self.frames.len() >= MAX_DEPTH {
                            let name = match &*gptr.add(slot as usize) {
                                Value::Func(f) => f.name.to_string(),
                                _ => "?".to_string(),
                            };
                            return Err(self.err_at(ip, format!(
                                "递归太深了（超过 {} 层）停了下来。检查一下 {} 的结束条件是不是写错了。",
                                MAX_DEPTH, name
                            )));
                        }
                        if n > nparams {
                            drop_from(sptr, &mut sp, callee_idx + nparams);
                        }
                        if n < nparams {
                            if let Value::Func(f) = &*gptr.add(slot as usize) {
                                for k in n..nparams {
                                    std::ptr::write(
                                        sptr.add(sp),
                                        f.defaults.get(k).cloned().unwrap_or(Value::Null),
                                    );
                                    sp += 1;
                                }
                            }
                        }
                        let want = nlocals.max(nparams);
                        while sp < callee_idx + want {
                            std::ptr::write(sptr.add(sp), Value::Null);
                            sp += 1;
                        }
                        self.frames.push(Frame {
                            base: callee_idx as u32,
                            locals_base: callee_idx as u32,
                            ret_ip: ip as u32,
                            call_ip: ip as u32,
                            iter_base: self.iters.len() as u32,
                            range_base: self.ranges.len() as u32,
                            fn_id: fid,
                            argc: n.min(nparams) as u8,
                        });
                        base = callee_idx;
                        ip = entry;
                    } else {
                        let shown = (*gptr.add(slot as usize)).to_display();
                        return Err(self.err_at(ip, format!("{} 不是一个函数", shown)));
                    }
                }
                Op::BuildList(n) => {
                    let n = n as usize;
                    let at = sp.saturating_sub(n);
                    let items = take_tail(sptr, &mut sp, at);
                    std::ptr::write(sptr.add(sp), Value::list(items));
                    sp += 1;
                }
                Op::BuildDict(n) => {
                    let n = n as usize;
                    let at = sp.saturating_sub(n * 2);
                    let raw = take_tail(sptr, &mut sp, at);
                    let mut d = Dict::new();
                    let mut i = 0;
                    while i + 1 < raw.len() {
                        d.set(Rc::from(raw[i].to_display().as_str()), raw[i + 1].clone());
                        i += 2;
                    }
                    std::ptr::write(sptr.add(sp), Value::Dict(Rc::new(std::cell::RefCell::new(d))));
                    sp += 1;
                }
                Op::Index => {
                    sp -= 1;
                    let idx = std::ptr::read(sptr.add(sp));
                    sp -= 1;
                    let obj = std::ptr::read(sptr.add(sp));
                    let v = match index_get(&obj, &idx) {
                        Ok(v) => v,
                        Err(m) => return Err(self.err_at(ip, m)),
                    };
                    std::ptr::write(sptr.add(sp), v);
                    sp += 1;
                }
                Op::IndexSet => {
                    sp -= 1;
                    let val = std::ptr::read(sptr.add(sp));
                    sp -= 1;
                    let idx = std::ptr::read(sptr.add(sp));
                    sp -= 1;
                    let obj = std::ptr::read(sptr.add(sp));
                    if let Err(m) = index_set(&obj, &idx, val) {
                        return Err(self.err_at(ip, m));
                    }
                }
                Op::Member(name_idx) => {
                    let name = self.chunk.name_at(name_idx);
                    sp -= 1;
                    let obj = std::ptr::read(sptr.add(sp));
                    let v = match member_get(&obj, &name) {
                        Ok(v) => v,
                        Err(m) => return Err(self.err_at(ip, m)),
                    };
                    std::ptr::write(sptr.add(sp), v);
                    sp += 1;
                }
                Op::MemberSet(name_idx) => {
                    let name = self.chunk.name_at(name_idx);
                    sp -= 1;
                    let val = std::ptr::read(sptr.add(sp));
                    sp -= 1;
                    let obj = std::ptr::read(sptr.add(sp));
                    if let Err(m) = member_set(&obj, &name, val) {
                        return Err(self.err_at(ip, m));
                    }
                }
                Op::Ask => {
                    let line = if self.input_pos < self.inputs.len() {
                        let v = self.inputs[self.input_pos].clone();
                        self.input_pos += 1;
                        v
                    } else {
                        String::new()
                    };
                    // 控制台风格：把用户输入也回显到输出里，读起来像一段对话
                    if !line.is_empty() {
                        self.out.push_str(&line);
                        self.out.push('\n');
                    }
                    std::ptr::write(sptr.add(sp), Value::string(line));
                    sp += 1;
                }
                Op::Report(n) => {
                    let cnt = n as usize;
                    if cnt == 0 {
                        // 不带参数 = 报告"现在都有什么"
                        let mut lines = Vec::new();
                        for (i, g) in self.globals.iter().enumerate() {
                            let name = self.chunk.global_names.get(i).map(|x| x.to_string()).unwrap_or_default();
                            if name.is_empty() {
                                continue;
                            }
                            lines.push(format!("  {} = {}（{}）", name, g.to_display(), g.type_name()));
                        }
                        let body = if lines.is_empty() {
                            "当前变量：（还没有）".to_string()
                        } else {
                            format!("当前变量：\n{}", lines.join("\n"))
                        };
                        self.reports.push(body);
                    } else {
                        let at = sp.saturating_sub(cnt);
                        let items = take_tail(sptr, &mut sp, at);
                        let mut line = String::new();
                        for (i, v) in items.iter().enumerate() {
                            if i > 0 {
                                line.push(' ');
                            }
                            line.push_str(&v.to_display());
                        }
                        self.reports.push(line);
                    }
                }
                Op::Print(n) => {
                    let n = n as usize;
                    let at = sp.saturating_sub(n);
                    let items = take_tail(sptr, &mut sp, at);
                    let mut line = String::new();
                    for (i, v) in items.iter().enumerate() {
                        if i > 0 {
                            line.push(' ');
                        }
                        line.push_str(&v.to_display());
                    }
                    self.out.push_str(&line);
                    self.out.push('\n');
                }
                /* ── 计数循环 ── */
                Op::RangeInit { var, is_global, inclusive } => {
                    if sp < 3 {
                        return Err(self.err_at(ip, "for 循环的起止值不完整"));
                    }
                    sp -= 1;
                    let step = std::ptr::read(sptr.add(sp));
                    sp -= 1;
                    let to = std::ptr::read(sptr.add(sp));
                    sp -= 1;
                    let from = std::ptr::read(sptr.add(sp));
                    let state = match (&from, &to, &step) {
                        (Value::Int(f), Value::Int(t), Value::Int(s)) => {
                            if *s == 0 {
                                return Err(self.err_at(ip, "for 循环的步长不能是 0"));
                            }
                            RangeState::Int { i: *f, end: *t, step: *s, inclusive }
                        }
                        _ => {
                            let f = match from.as_num() {
                                Some(v) => v,
                                None => return Err(self.err_at(ip, "for 循环的起点不是数字")),
                            };
                            let t = match to.as_num() {
                                Some(v) => v,
                                None => return Err(self.err_at(ip, "for 循环的终点不是数字")),
                            };
                            let s = match step.as_num() {
                                Some(v) => v,
                                None => return Err(self.err_at(ip, "for 循环的步长不是数字")),
                            };
                            if s == 0.0 {
                                return Err(self.err_at(ip, "for 循环的步长不能是 0"));
                            }
                            RangeState::Float { i: f, end: t, step: s, inclusive }
                        }
                    };
                    self.ranges.push(RangeLoop { state, var, is_global });
                }
                Op::RangeNext { fail } => {
                    fuel -= 1;
                    if fuel == 0 {
                        self.steps += FUEL;
                        if self.steps > self.max_steps {
                            return Err(self.err_at(ip, "执行步数超限，可能是死循环"));
                        }
                        fuel = FUEL;
                    }
                    let mut land: Option<(usize, bool, Value)> = None;
                    if let Some(r) = self.ranges.last_mut() {
                        let (var, g) = (r.var as usize, r.is_global);
                        match &mut r.state {
                            RangeState::Int { i, end, step, inclusive } => {
                                let ok = if *step > 0 {
                                    if *inclusive { *i <= *end } else { *i < *end }
                                } else if *inclusive {
                                    *i >= *end
                                } else {
                                    *i > *end
                                };
                                if ok {
                                    land = Some((var, g, Value::Int(*i)));
                                    *i = i.wrapping_add(*step);
                                }
                            }
                            RangeState::Float { i, end, step, inclusive } => {
                                let ok = if *step > 0.0 {
                                    if *inclusive { *i <= *end } else { *i < *end }
                                } else if *inclusive {
                                    *i >= *end
                                } else {
                                    *i > *end
                                };
                                if ok {
                                    land = Some((var, g, Value::Float(*i)));
                                    *i += *step;
                                }
                            }
                        }
                    }
                    match land {
                        Some((var, g, v)) => {
                            if g {
                                if var < glen {
                                    *gptr.add(var) = v;
                                }
                            } else {
                                let idx = base + var;
                                if idx < sp {
                                    *sptr.add(idx) = v;
                                }
                            }
                        }
                        None => {
                            self.ranges.pop();
                            ip = fail as usize;
                        }
                    }
                }
                Op::RangeEnd => {
                    self.ranges.pop();
                }
                Op::IterInit => {
                    sp -= 1;
                    let it = std::ptr::read(sptr.add(sp));
                    let (src, len) = match iter_source(&it) {
                        Ok(v) => v,
                        Err(m) => return Err(self.err_at(ip, m)),
                    };
                    self.iters.push(IterState { src, len, pos: 0 });
                }
                Op::IterNext(target) => {
                    let mut got: Option<Value> = None;
                    if let Some(st) = self.iters.last_mut() {
                        if st.pos < st.len {
                            got = st.src.get(st.pos);
                            st.pos += 1;
                        }
                    }
                    match got {
                        Some(v) => {
                            std::ptr::write(sptr.add(sp), v);
                            sp += 1;
                        }
                        None => ip = target as usize,
                    }
                }
                Op::IterEnd => {
                    self.iters.pop();
                }
            }
        }
    }
}

/// 帧退出后的去向
enum FrameExit {
    /// 回到调用者，ret_ip 是下一条指令
    Cont(usize),
    /// 顶层返回，程序结束
    Top,
}

/// 退出当前帧：收迭代器 / 计数循环状态、截栈。返回值由调用方自己放。
#[inline]
unsafe fn leave_frame(
    vm: &mut Vm,
    sptr: *mut Value,
    sp: &mut usize,
    base: &mut usize,
) -> FrameExit {
    let frame = match vm.frames.pop() {
        Some(f) => f,
        None => return FrameExit::Top,
    };
    // 函数体里没走完的迭代器 / 计数循环状态要收干净，
    // 否则下一次递归会用到残留状态（for 循环取值错乱就是这么来的）
    if vm.iters.len() != frame.iter_base as usize {
        vm.iters.truncate(frame.iter_base as usize);
    }
    if vm.ranges.len() != frame.range_base as usize {
        vm.ranges.truncate(frame.range_base as usize);
    }
    drop_from(sptr, sp, frame.base as usize);
    *base = vm.frames.last().map(|f| f.locals_base as usize).unwrap_or(0);
    if vm.frames.is_empty() {
        return FrameExit::Top;
    }
    FrameExit::Cont(frame.ret_ip as usize)
}

/// 把 [from, sp) 这段值取成一个 Vec，栈顶退到 from
#[inline]
unsafe fn take_tail(ptr: *mut Value, sp: &mut usize, from: usize) -> Vec<Value> {
    let mut out = Vec::with_capacity(sp.saturating_sub(from));
    let mut i = from;
    while i < *sp {
        out.push(std::ptr::read(ptr.add(i)));
        i += 1;
    }
    *sp = from;
    out
}

/// 把栈从 sp 收回到 to，逐个析构（Rc 要正确减引用）
#[inline]
unsafe fn drop_from(ptr: *mut Value, sp: &mut usize, to: usize) {
    while *sp > to {
        *sp -= 1;
        std::ptr::drop_in_place(ptr.add(*sp));
    }
}

#[inline]
fn inc_value(v: &mut Value, delta: i8) -> bool {
    match v {
        Value::Int(i) => {
            *v = Value::Int(i.wrapping_add(delta as i64));
            true
        }
        Value::Float(f) => {
            *v = Value::Float(*f + delta as f64);
            true
        }
        ref other => {
            let n = match other.as_num() {
                Some(n) => n,
                None => return false,
            };
            *v = Value::Float(n + delta as f64);
            true
        }
    }
}

/* ───────────────────────── 运算语义 ───────────────────────── */

/// 复合赋值（`x += y`）的统一实现：先试就地字符串追加，再走通用算术。
fn compound_value(bop: u8, mut a: Value, b: &Value) -> Result<Value, String> {
    // 0 = 加法，唯一可能走"就地追加"的分支
    if bop == 0 {
        let appendable = match (&a, b) {
            (Value::Str(sa), Value::Str(_)) => Rc::strong_count(sa) == 1 && Rc::weak_count(sa) == 0,
            _ => false,
        };
        if appendable {
            if let (Value::Str(s), Value::Str(sb)) = (&mut a, b) {
                // 引用计数为 1，可以原地 push_str，摊还 O(1)
                if let Some(buf) = Rc::get_mut(s) {
                    buf.push_str(sb.as_str());
                }
            }
            return Ok(a);
        }
    }
    // 整数快路径
    if let (Value::Int(x), Value::Int(y)) = (&a, b) {
        let (x, y) = (*x, *y);
        let fast = match bop {
            0 => x.checked_add(y).map(Value::Int),
            1 => x.checked_sub(y).map(Value::Int),
            2 => x.checked_mul(y).map(Value::Int),
            3 if y != 0 && x % y == 0 => Some(Value::Int(x / y)),
            4 if y != 0 => Some(Value::Int(x % y)),
            _ => None,
        };
        if let Some(v) = fast {
            return Ok(v);
        }
    }
    let op = bop_to_op(bop).ok_or_else(|| "不支持的复合赋值".to_string())?;
    arith(op, &a, b)
}

/// `BinLocalK` 的比较操作码 → 通用比较指令
fn cmp_of(code: u8) -> Op {
    match code {
        6 => Op::Eq,
        7 => Op::Ne,
        8 => Op::Lt,
        9 => Op::Le,
        10 => Op::Gt,
        11 => Op::Ge,
        _ => Op::Eq,
    }
}

fn bop_to_op(bop: u8) -> Option<Op> {
    Some(match bop {
        0 => Op::Add,
        1 => Op::Sub,
        2 => Op::Mul,
        3 => Op::Div,
        4 => Op::Mod,
        5 => Op::Pow,
        _ => return None,
    })
}

fn arith(op: Op, a: &Value, b: &Value) -> Result<Value, String> {
    // 字符串：+ 就是拼接
    if op == Op::Add {
        if matches!(a, Value::Str(_)) || matches!(b, Value::Str(_)) {
            let mut s = match a {
                Value::Str(sa) => sa.as_str().to_string(),
                other => other.to_display(),
            };
            match b {
                Value::Str(sb) => s.push_str(sb.as_str()),
                other => s.push_str(&other.to_display()),
            }
            return Ok(Value::string(s));
        }
    }
    if let (Value::Int(x), Value::Int(y)) = (a, b) {
        let (x, y) = (*x, *y);
        return Ok(match op {
            Op::Add => match x.checked_add(y) {
                Some(v) => Value::Int(v),
                None => Value::Float(x as f64 + y as f64),
            },
            Op::Sub => match x.checked_sub(y) {
                Some(v) => Value::Int(v),
                None => Value::Float(x as f64 - y as f64),
            },
            Op::Mul => match x.checked_mul(y) {
                Some(v) => Value::Int(v),
                None => Value::Float(x as f64 * y as f64),
            },
            Op::Div => {
                if y == 0 {
                    return Err("除数为 0".into());
                }
                if x % y == 0 {
                    Value::Int(x / y)
                } else {
                    Value::Float(x as f64 / y as f64)
                }
            }
            Op::Mod => {
                if y == 0 {
                    return Err("取余的除数为 0".into());
                }
                Value::Int(x % y)
            }
            Op::Pow => {
                if y >= 0 && y < 63 {
                    match x.checked_pow(y as u32) {
                        Some(v) => Value::Int(v),
                        None => Value::Float((x as f64).powf(y as f64)),
                    }
                } else {
                    Value::Float((x as f64).powf(y as f64))
                }
            }
            _ => return Err("不支持的整数运算".into()),
        });
    }
    let x = a.as_num().ok_or_else(|| format!("{} 不是数字", a.to_display()))?;
    let y = b.as_num().ok_or_else(|| format!("{} 不是数字", b.to_display()))?;
    Ok(match op {
        Op::Add => Value::Float(x + y),
        Op::Sub => Value::Float(x - y),
        Op::Mul => Value::Float(x * y),
        Op::Div => {
            if y == 0.0 {
                return Err("除数为 0".into());
            }
            Value::Float(x / y)
        }
        Op::Mod => {
            if y == 0.0 {
                return Err("取余的除数为 0".into());
            }
            Value::Float(x % y)
        }
        Op::Pow => {
            let r = x.powf(y);
            if r.fract() == 0.0 && r.abs() < 9.007_199_254_740_992e15 {
                Value::Int(r as i64)
            } else {
                Value::Float(r)
            }
        }
        _ => return Err("不支持的运算".into()),
    })
}

fn compare(op: Op, a: &Value, b: &Value) -> Result<bool, String> {
    if op == Op::Eq {
        return Ok(a.deep_eq(b));
    }
    if op == Op::Ne {
        return Ok(!a.deep_eq(b));
    }
    // 两个字符串按字典序；其余按数字
    if let (Value::Str(x), Value::Str(y)) = (a, b) {
        let (x, y) = (x.as_str(), y.as_str());
        return Ok(match op {
            Op::Lt => x < y,
            Op::Le => x <= y,
            Op::Gt => x > y,
            Op::Ge => x >= y,
            _ => return Err("不支持的比较".into()),
        });
    }
    let x = a.as_num().ok_or_else(|| format!("{} 不能参与比较", a.to_display()))?;
    let y = b.as_num().ok_or_else(|| format!("{} 不能参与比较", b.to_display()))?;
    Ok(match op {
        Op::Lt => x < y,
        Op::Le => x <= y,
        Op::Gt => x > y,
        Op::Ge => x >= y,
        _ => return Err("不支持的比较".into()),
    })
}

/* ───────────────────────── 下标 / 成员 ───────────────────────── */

fn norm_index(i: &Value, len: usize) -> Result<usize, String> {
    let raw = i.as_int().ok_or_else(|| "下标要是整数".to_string())?;
    let idx = if raw < 0 { raw + len as i64 } else { raw };
    if idx < 0 || idx as usize >= len {
        return Err(format!("下标 {} 越界（长度 {}）", raw, len));
    }
    Ok(idx as usize)
}

fn index_get(obj: &Value, idx: &Value) -> Result<Value, String> {
    match obj {
        Value::List(l) => {
            let items = l.borrow();
            let i = norm_index(idx, items.len())?;
            Ok(items[i].clone())
        }
        Value::Str(s) => {
            let chars: Vec<char> = s.chars().collect();
            let i = norm_index(idx, chars.len())?;
            Ok(Value::string(chars[i].to_string()))
        }
        Value::Dict(d) => {
            let key = idx.to_display();
            d.borrow().get(&key).ok_or_else(|| format!("字典里没有键「{}」", key))
        }
        Value::Ptr(p) => {
            let m = p.borrow();
            if m.freed {
                return Err("这块内存已经 free 了，不能再读".into());
            }
            let i = norm_index(idx, m.buf.len())?;
            Ok(m.buf[i].clone())
        }
        other => Err(format!("{} 不能用下标取值", other.to_display())),
    }
}

fn index_set(obj: &Value, idx: &Value, val: Value) -> Result<(), String> {
    match obj {
        Value::List(l) => {
            let len = l.borrow().len();
            let i = norm_index(idx, len)?;
            l.borrow_mut()[i] = val.clone_deep();
            Ok(())
        }
        Value::Dict(d) => {
            let key = Rc::from(idx.to_display().as_str());
            d.borrow_mut().set(key, val.clone_deep());
            Ok(())
        }
        Value::Ptr(p) => {
            let mut m = p.borrow_mut();
            if m.freed {
                return Err("这块内存已经 free 了，不能再写".into());
            }
            let len = m.buf.len();
            let i = norm_index(idx, len)?;
            m.buf[i] = val.clone_deep();
            Ok(())
        }
        other => Err(format!("{} 不能用下标赋值", other.to_display())),
    }
}

fn member_get(obj: &Value, name: &str) -> Result<Value, String> {
    match obj {
        Value::Dict(d) => d.borrow().get(name).ok_or_else(|| format!("字典里没有键「{}」", name)),
        other => Err(format!(
            "{} 没有成员「{}」（类与实例这版 VM 还没实现）",
            other.to_display(),
            name
        )),
    }
}

fn member_set(obj: &Value, name: &str, val: Value) -> Result<(), String> {
    match obj {
        Value::Dict(d) => {
            d.borrow_mut().set(Rc::from(name), val.clone_deep());
            Ok(())
        }
        other => Err(format!(
            "{} 不能设置成员「{}」（类与实例这版 VM 还没实现）",
            other.to_display(),
            name
        )),
    }
}

/// 建迭代源：列表/字典/指针只留引用，字符串拆成字符（本来就小）
fn iter_source(v: &Value) -> Result<(IterSrc, usize), String> {
    match v {
        Value::List(l) => {
            let n = l.borrow().len();
            Ok((IterSrc::List(Rc::clone(l)), n))
        }
        Value::Str(s) => {
            let chars: Vec<Value> = s.chars().map(|c| Value::string(c.to_string())).collect();
            let n = chars.len();
            Ok((IterSrc::Owned(chars), n))
        }
        Value::Dict(d) => {
            let n = d.borrow().len();
            Ok((IterSrc::Dict(Rc::clone(d)), n))
        }
        Value::Ptr(p) => {
            let m = p.borrow();
            if m.freed {
                return Err("这块内存已经 free 了，不能再遍历".into());
            }
            let n = m.buf.len();
            drop(m);
            Ok((IterSrc::Ptr(Rc::clone(p)), n))
        }
        other => Err(format!("{} 不能用于 for 遍历", other.to_display())),
    }
}
