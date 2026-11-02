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
use crate::value::{Dict, PtrTarget, Value};
use std::cell::RefCell;
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

/// try 的处理器：出错时从这里"跳回来"继续跑。
/// 存的是出错瞬间要恢复的三样东西——去哪、栈多高、帧多深。
struct Handler {
    ip: u32,
    sp: usize,
    frames_len: usize,
    iters_len: usize,
    ranges_len: usize,
}

/// 调用帧。字段全是小整数：fib 这种一秒几百万次调用的场景，
/// 帧多大就是成倍的差别（塞个 Rc<str> 名字进来，帧从 28 字涨到 72 字节，
/// 每帧还要多一次引用计数）。
struct Frame {
    /// 帧编号：&局部变量 的指针记下它，用来判断变量是否还活着
    id: u64,
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
    /// 进函数时 try 处理器的深度：return 出门时要把函数里没散掉的 try 一起收掉，
    /// 不然外层一旦报错会跳进一个已经废弃的 catch 里
    hdl_base: u32,
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
                if m.freed { return None; }
                match &m.target {
                    PtrTarget::Cells(c) => c.borrow().get(m.idx + i).cloned(),
                    _ => None,
                }
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

/// 操作数栈：容量固定，用裸指针访问。
///
/// **关键设计**：底层是 `Vec<MaybeUninit<Value>>`，只有 `[0, sp)` 这一段是"活的值"，
/// 析构时也只 drop 这一段。这样"从槽里把值搬走"（ptr::read）之后，
/// 被搬走的格子天然不在存活区间内，不会被第二次 drop —— 不需要每次出栈都写一遍 null。
/// （以前用 `Vec<Value>` + 收尾抹 null，既慢又容易漏，漏一次就是 Rc 计数下溢、
///  堆被写坏、程序退出时崩。）
struct OpStack {
    buf: Vec<std::mem::MaybeUninit<Value>>,
    sp: usize,
}

impl OpStack {
    fn new() -> OpStack {
        let n = STACK_PAD + STACK_CAP;
        let mut buf: Vec<std::mem::MaybeUninit<Value>> = Vec::with_capacity(n);
        // 只有栈底那一段（永远不参与运算的垫子）需要先初始化成 null，
        // 其余格子保持"未初始化"，用多少写多少。
        unsafe {
            buf.set_len(n);
            let p = buf.as_mut_ptr() as *mut Value;
            for i in 0..STACK_PAD {
                std::ptr::write(p.add(i), Value::Null);
            }
        }
        OpStack { buf, sp: STACK_PAD }
    }
}

impl Drop for OpStack {
    fn drop(&mut self) {
        // 只析构真正活着的值（栈底垫子 + 栈上内容）
        let p = self.buf.as_mut_ptr() as *mut Value;
        for i in 0..self.sp.min(self.buf.len()) {
            unsafe { std::ptr::drop_in_place(p.add(i)) };
        }
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
    /// try 的处理器栈（出错就弹回最近的一层）
    handlers: Vec<Handler>,
    /// `throw` 抛出来的原值：catch 拿到的就是它（没 throw 过就是运行时错误）
    thrown: Option<Value>,
    /// 每个指令位置一格调用缓存
    call_ic: Vec<CallIc>,
    /// 手动内存对账表
    /// 下一个调用帧的编号（&局部变量 的指针用它判断变量是否还活着）
    next_frame_id: u64,
    allocs: Vec<AllocRec>,
    /// 报告通道：`报告 xxx` 写这里，和 print 分开走
    reports: Vec<String>,
    out: String,
    steps: u64,
    max_steps: u64,
    /// 预设输入（IDE 里接输入框，测试里喂脚本化输入）
    inputs: Vec<String>,
    input_pos: usize,
    /// 交互模式：ask 没有预设输入时"挂起等人回答"，而不是直接给空串
    interactive: bool,
    need_input: bool,
    /// 计时器基准（毫秒）：time read 报的是距这里的秒数
    time_base: f64,
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
            handlers: Vec::new(),
            thrown: None,
            call_ic: vec![CallIc::default(); chunk_len],
            next_frame_id: 2,
            allocs: Vec::new(),
            reports: Vec::new(),
            out: String::new(),
            steps: 0,
            max_steps: DEFAULT_MAX_STEPS,
            inputs: Vec::new(),
            input_pos: 0,
            interactive: false,
            need_input: false,
            time_base: now_millis(),
        }
    }

    pub fn set_max_steps(&mut self, n: u64) {
        self.max_steps = n;
    }

    /// alloc 之后登记一笔（记下是哪一行要的、要了几格）
    fn note_alloc(&mut self, v: &Value, ip: usize) {
        if let Value::Ptr(p) = v {
            let id = Rc::as_ptr(p) as usize;
            let cells = match &p.borrow().target {
                PtrTarget::Cells(c) => c.borrow().len(),
                _ => 1,
            };
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

    /// 交互模式：ask 没有预设输入时先挂起，等 IDE 把回答送进来再重跑。
    pub fn set_interactive(&mut self, on: bool) {
        self.interactive = on;
    }

    /// 这次停下是不是因为"等人输入"
    pub fn needs_input(&self) -> bool {
        self.need_input
    }

    /// 取走 ask 的提问文本（就是挂起前最后打印的那一行；调试行不算提问）
    pub fn take_prompt(&mut self) -> Option<String> {
        let last = self
            .out
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty() && !l.starts_with('\u{1}'))
            .map(|l| l.trim().to_string());
        last
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

    /// 合并源码里的行号 → (哪个文件, 那个文件里的第几行)。
    /// 没走 import 时两者一样，直接返回主文件。
    fn origin(&self, line: u32) -> (Rc<str>, u32) {
        let i = line as usize - 1;
        match (self.chunk.line_file.get(i), self.chunk.line_local.get(i)) {
            (Some(f), Some(local)) => (
                self.chunk
                    .files
                    .get(*f as usize)
                    .cloned()
                    .unwrap_or_else(|| Rc::clone(&self.chunk.file)),
                *local,
            ),
            _ => (Rc::clone(&self.chunk.file), line),
        }
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
            let (fname, local) = self.origin(line);
            if fname == self.chunk.file {
                msg = format!("第 {} 行：{}", local, msg);
            } else {
                // 错误出在 import 进来的库里：得说清是哪个文件，不然行号对不上
                msg = format!("{} 第 {} 行：{}", fname, local, msg);
            }
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
        // 收尾不需要再扫栈抹 null：OpStack 的 Drop 只析构 [0, sp) 这段活值，
        // 被 ptr::read 搬走的格子天然不在里面。
        self.run_inner()
    }

    fn run_inner(&mut self) -> Result<RunOutcome, FloatError> {
        self.globals = vec![Value::Null; self.chunk.n_globals as usize];
        self.stack.sp = STACK_PAD;
        self.frames.clear();
        self.iters.clear();
        self.ranges.clear();
        self.handlers.clear();
        self.thrown = None;
        self.allocs.clear();
        self.reports.clear();
        self.out.clear();
        self.steps = 0;
        self.input_pos = 0;
        self.frames.push(Frame {
            id: 1,
            base: 0,
            locals_base: 0,
            ret_ip: 0,
            call_ip: 0,
            iter_base: 0,
            range_base: 0,
            hdl_base: 0,
            fn_id: u16::MAX,
            argc: 0,
        });
        // try/catch 的落地方式：exec 里所有报错照旧 `return Err(…)`，
        // 由这里统一接住——有 catch 就把现场倒回去、把错误值压栈、接着跑。
        // 好处是 40 多个报错点一个都不用改，热路径也没有任何额外判断。
        let mut start_ip = 0usize;
        let mut start_sp = STACK_PAD;
        loop {
            match unsafe { self.exec(start_ip, start_sp) } {
                Ok(o) => return Ok(o),
                Err(e) => {
                    // 「等人输入」不是真错误：不能交给 catch 处理，直接冒到上层
                    if self.need_input {
                        return Err(e);
                    }
                    match self.handlers.pop() {
                    None => return Err(e),
                    Some(h) => {
                        // 回到 try 那一刻的现场
                        self.frames.truncate(h.frames_len);
                        self.iters.truncate(h.iters_len);
                        self.ranges.truncate(h.ranges_len);
                        self.stack.sp = h.sp;
                        // 错误值占住栈顶，catch 那边用 SetLocal 取走
                        let v = match self.thrown.take() {
                            Some(v) => v,
                            None => self.error_value(&e),
                        };
                        if h.sp < self.stack.buf.len() {
                            // 用裸写：这一格虽然不在存活区间里，但里面可能是已搬走的残值
                            unsafe {
                                let p = self.stack.buf.as_mut_ptr() as *mut Value;
                                std::ptr::write(p.add(h.sp), v);
                            }
                            start_sp = h.sp + 1;
                        } else {
                            return Err(e);
                        }
                        // 报错点可能在函数里，回来时 base 要跟着 frame 重算
                        start_ip = h.ip as usize;
                    }
                    }
                }
            }
        }
    }

    /// catch 接到的"运行时错误"长什么样：一个小字典，键是中文的。
    /// （`throw` 抛出来的东西则原样传过去，不做包装。）
    fn error_value(&self, e: &FloatError) -> Value {
        let mut d = crate::value::Dict::new();
        d.set(Rc::from("消息"), Value::string(e.message.clone()));
        d.set(Rc::from("message"), Value::string(e.message.clone()));
        d.set(Rc::from("行"), Value::Int(e.span.line as i64));
        d.set(Rc::from("类型"), Value::string("运行时错误".to_string()));
        Value::Dict(Rc::new(std::cell::RefCell::new(d)))
    }

    /// 执行主体。全程用裸指针读写操作数栈和全局槽。
    ///
    /// 安全性：sp 只在 [0, STACK_CAP) 内移动（每轮开头统一检查余量 256 格），
    /// 槽号来自编译期，全局槽长度在 reset 后不变。
    unsafe fn exec(&mut self, start_ip: usize, start_sp: usize) -> Result<RunOutcome, FloatError> {
        let sptr = self.stack.buf.as_mut_ptr() as *mut Value;
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

        let mut ip = start_ip;
        let mut sp = start_sp;
        // 局部变量基址缓存：GetLocal/SetLocal 是最高频指令，不能每次都查 frames
        let mut base = self.frames.last().map(|f| f.locals_base as usize).unwrap_or(0);
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
                    let v = take(sptr, sp);
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
                    let v = take(sptr, sp);
                    if (i as usize) < glen {
                        *gptr.add(i as usize) = v;
                    }
                }
                Op::Add | Op::Sub | Op::Mul | Op::Div | Op::IDiv | Op::Mod | Op::Pow => {
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
                            // 整除向下取整：-7 // 2 = -4（Rust 的 / 是向零截断，得自己算）
                            Op::IDiv if y != 0 => Some(Value::Int(floor_div_i64(x, y))),
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
                    let b = take(sptr, sp);
                    sp -= 1;
                    let a = take(sptr, sp);
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
                    let b = take(sptr, sp);
                    sp -= 1;
                    let a = take(sptr, sp);
                    let v = match compare(op, &a, &b) {
                        Ok(v) => v,
                        Err(m) => return Err(self.err_at(ip, m)),
                    };
                    std::ptr::write(sptr.add(sp), Value::Bool(v));
                    sp += 1;
                }
                Op::BitAnd | Op::BitOr | Op::BitXor | Op::Shl | Op::Shr => {
                    sp -= 1;
                    let b = take(sptr, sp);
                    sp -= 1;
                    let a = take(sptr, sp);
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
                    let a = take(sptr, sp);
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
                    let a = take(sptr, sp);
                    std::ptr::write(sptr.add(sp), Value::Bool(!a.truthy()));
                    sp += 1;
                }
                Op::BitNot => {
                    sp -= 1;
                    let a = take(sptr, sp);
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
                    let v = take(sptr, sp);
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
                        self.next_frame_id += 1;
                        self.frames.push(Frame {
                            id: self.next_frame_id - 1,
                            base: callee_idx as u32,
                            locals_base: newbase as u32,
                            ret_ip: ip as u32,
                            call_ip: ip as u32,
                            iter_base: self.iters.len() as u32,
                            range_base: self.ranges.len() as u32,
                            hdl_base: self.handlers.len() as u32,
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
                    // ── 类实例：查 __m_类_方法，receiver 直接当 self（第 0 个参数）──
                    let recv_idx = sp.saturating_sub(n + 1);
                    let cls = match &*sptr.add(recv_idx) {
                        Value::Dict(d) => d.borrow().get("__cls").map(|v| v.to_display()),
                        _ => None,
                    };
                    if let Some(mut cls) = cls {
                        // 沿着 __base 往上找：子类没写的方法用父类的
                        let mut found = None;
                        for _ in 0..16 {
                            let want_name = format!("__m_{}_{}", cls, name);
                            if let Some(slot) = self.chunk.global_names.iter().position(|g| &**g == want_name.as_str()) {
                                found = Some(slot);
                                break;
                            }
                            let parent = match &*sptr.add(recv_idx) {
                                Value::Dict(d) => d.borrow().get("__base").map(|v| v.to_display()),
                                _ => None,
                            };
                            match parent {
                                Some(p) => cls = p,
                                None => break,
                            }
                        }
                        if let Some(slot) = found {
                            let fv = (*gptr.add(slot)).clone();
                            if let Value::Func(f) = fv {
                                let (entry, nlocals, nparams, fid) = (f.entry, f.nlocals, f.params.len(), f.id);
                                if self.frames.len() >= MAX_DEPTH {
                                    return Err(self.err_at(ip, format!("递归太深了（超过 {} 层）", MAX_DEPTH)));
                                }
                                // 实参不够就用默认值补
                                if n + 1 < nparams {
                                    for k in (n + 1)..nparams {
                                        std::ptr::write(
                                            sptr.add(sp),
                                            f.defaults.get(k).cloned().unwrap_or(Value::Null),
                                        );
                                        sp += 1;
                                    }
                                }
                                let want = nlocals.max(nparams);
                                while sp < recv_idx + want {
                                    std::ptr::write(sptr.add(sp), Value::Null);
                                    sp += 1;
                                }
                                self.next_frame_id += 1;
                                self.frames.push(Frame {
                                    id: self.next_frame_id - 1,
                                    base: recv_idx as u32,
                                    locals_base: recv_idx as u32,
                                    ret_ip: ip as u32,
                                    call_ip: ip as u32,
                                    iter_base: self.iters.len() as u32,
                                    range_base: self.ranges.len() as u32,
                                    hdl_base: self.handlers.len() as u32,
                                    fn_id: fid,
                                    argc: (n + 1).min(nparams) as u8,
                                });
                                base = recv_idx;
                                ip = entry;
                                continue;
                            }
                        }
                    }
                    let mut args = Vec::with_capacity(n);
                    for _ in 0..n {
                        sp -= 1;
                        args.push(take(sptr, sp));
                    }
                    args.reverse();
                    sp -= 1;
                    let recv = take(sptr, sp);
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
                    let v = take(sptr, sp);
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
                        self.next_frame_id += 1;
                        self.frames.push(Frame {
                            id: self.next_frame_id - 1,
                            base: callee_idx as u32,
                            locals_base: callee_idx as u32,
                            ret_ip: ip as u32,
                            call_ip: ip as u32,
                            iter_base: self.iters.len() as u32,
                            range_base: self.ranges.len() as u32,
                            hdl_base: self.handlers.len() as u32,
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
                    let idx = take(sptr, sp);
                    sp -= 1;
                    let obj = take(sptr, sp);
                    let v = match index_get(&obj, &idx) {
                        Ok(v) => v,
                        Err(m) => return Err(self.err_at(ip, m)),
                    };
                    std::ptr::write(sptr.add(sp), v);
                    sp += 1;
                }
                Op::IndexSet => {
                    sp -= 1;
                    let val = take(sptr, sp);
                    sp -= 1;
                    let idx = take(sptr, sp);
                    sp -= 1;
                    let obj = take(sptr, sp);
                    if let Err(m) = index_set(&obj, &idx, val) {
                        return Err(self.err_at(ip, m));
                    }
                }
                Op::AddrLocal(i) => {
                    // &局部变量：记下 (帧号, 绝对槽位)，解引用时再回来找
                    let slot = base + i as usize;
                    let frame = self.frames.last().map(|f| f.id).unwrap_or(1);
                    let p = Rc::new(RefCell::new(crate::value::MemBlock {
                        target: PtrTarget::Local { frame, slot },
                        idx: 0,
                        owned: false,
                        freed: false,
                    }));
                    std::ptr::write(sptr.add(sp), Value::Ptr(p));
                    sp += 1;
                }
                Op::AddrGlobal(i) => {
                    let p = Rc::new(RefCell::new(crate::value::MemBlock {
                        target: PtrTarget::Global(i as usize),
                        idx: 0,
                        owned: false,
                        freed: false,
                    }));
                    std::ptr::write(sptr.add(sp), Value::Ptr(p));
                    sp += 1;
                }
                Op::AddrIdx => {
                    sp -= 1;
                    let idx = take(sptr, sp);
                    sp -= 1;
                    let obj = take(sptr, sp);
                    let n = match idx {
                        Value::Int(k) => k,
                        Value::Float(f) => f as i64,
                        ref other => return Err(self.err_at(ip, format!("& 取地址的下标必须是数字，得到 {}", other.to_display()))),
                    };
                    let v = match &obj {
                        Value::List(l) => {
                            let len = l.borrow().len() as i64;
                            if n < 0 || n >= len {
                                return Err(self.err_at(ip, format!("& 取地址：下标 {} 越界了（列表长度 {}）", n, len)));
                            }
                            Value::alias_list(Rc::clone(l), n as usize)
                        }
                        Value::Ptr(p) => {
                            let b = p.borrow();
                            if b.freed {
                                return Err(self.err_at(ip, "这块内存已经 free 了，不能再取地址".to_string()));
                            }
                            match &b.target {
                                PtrTarget::Cells(c) => {
                                    let len = c.borrow().len() as i64;
                                    let real = b.idx as i64 + n;
                                    if real < 0 || real >= len {
                                        return Err(self.err_at(ip, format!("& 取地址：第 {} 格越界了（这块一共 {} 格）", real, len)));
                                    }
                                    Value::shift_ptr(p, n)
                                }
                                _ => {
                                    if n != 0 {
                                        return Err(self.err_at(ip, "&变量 的指针只有一格，不能偏移".to_string()));
                                    }
                                    obj.clone()
                                }
                            }
                        }
                        other => return Err(self.err_at(ip, format!("& 只能对列表元素或指针格子取地址，得到 {}", other.to_display()))),
                    };
                    std::ptr::write(sptr.add(sp), v);
                    sp += 1;
                }
                Op::Deref => {
                    sp -= 1;
                    let p = take(sptr, sp);
                    let v = match &p {
                        Value::Ptr(pr) => {
                            let b = pr.borrow();
                            if b.freed {
                                return Err(self.err_at(ip, "这块内存已经 free 了，不能再读".to_string()));
                            }
                            match &b.target {
                                PtrTarget::Cells(c) => {
                                    let cells = c.borrow();
                                    if b.idx >= cells.len() {
                                        return Err(self.err_at(ip, format!(
                                            "指针指到第 {} 格，越界了（这块一共 {} 格）", b.idx, cells.len())));
                                    }
                                    cells[b.idx].clone()
                                }
                                PtrTarget::Global(i) => self.globals.get(*i).cloned().unwrap_or(Value::Null),
                                PtrTarget::Local { frame, slot } => {
                                    if !self.frames.iter().any(|f| f.id == *frame) || *slot >= sp {
                                        return Err(self.err_at(ip, "这个指针指向的变量已经离开作用域了（悬空指针）".to_string()));
                                    }
                                    (*sptr.add(*slot)).clone()
                                }
                            }
                        }
                        other => return Err(self.err_at(ip, format!("* 只能解引用指针，得到 {}（取地址用 &）", other.to_display()))),
                    };
                    std::ptr::write(sptr.add(sp), v);
                    sp += 1;
                }
                Op::StoreDeref => {
                    sp -= 1;
                    let val = take(sptr, sp);
                    sp -= 1;
                    let p = take(sptr, sp);
                    match &p {
                        Value::Ptr(pr) => {
                            let b = pr.borrow();
                            if b.freed {
                                return Err(self.err_at(ip, "这块内存已经 free 了，不能再写".to_string()));
                            }
                            match &b.target {
                                PtrTarget::Cells(c) => {
                                    let len = c.borrow().len();
                                    if b.idx >= len {
                                        return Err(self.err_at(ip, format!(
                                            "指针指到第 {} 格，越界了（这块一共 {} 格）", b.idx, len)));
                                    }
                                    c.borrow_mut()[b.idx] = val.clone_deep();
                                }
                                PtrTarget::Global(i) => {
                                    if *i < glen {
                                        *gptr.add(*i) = val.clone_deep();
                                    }
                                }
                                PtrTarget::Local { frame, slot } => {
                                    if !self.frames.iter().any(|f| f.id == *frame) || *slot >= sp {
                                        return Err(self.err_at(ip, "这个指针指向的变量已经离开作用域了（悬空指针）".to_string()));
                                    }
                                    std::ptr::write(sptr.add(*slot), val.clone_deep());
                                }
                            }
                        }
                        other => return Err(self.err_at(ip, format!("* 只能解引用指针，得到 {}", other.to_display()))),
                    }
                }
                Op::Member(name_idx) => {
                    let name = self.chunk.name_at(name_idx);
                    sp -= 1;
                    let obj = take(sptr, sp);
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
                    let val = take(sptr, sp);
                    sp -= 1;
                    let obj = take(sptr, sp);
                    if let Err(m) = member_set(&obj, &name, val) {
                        return Err(self.err_at(ip, m));
                    }
                }
                Op::Ask => {
                    let line = if self.input_pos < self.inputs.len() {
                        let v = self.inputs[self.input_pos].clone();
                        self.input_pos += 1;
                        v
                    } else if self.interactive {
                        // 没有现成答案：标记"等人回答"，让上层把提问抛给界面
                        self.need_input = true;
                        return Err(self.err_at(ip, "__need_input__"));
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
                Op::Debug { n, level } => {
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
                    // 级别标记（\u{1} + 级别数字 + 空格）：界面据此上色，
                    // 1 = 报告（蓝），2 = 警告（黄），3 = 报错（红）。纯展示，不影响程序。
                    let lv = match level {
                        2 => '2',
                        3 => '3',
                        _ => '1',
                    };
                    self.out.push('\u{1}');
                    self.out.push(lv);
                    self.out.push(' ');
                    self.out.push_str(&line);
                    self.out.push('\n');
                }
                Op::TimeRead => {
                    // 距上次 time reset 过了多少秒（毫秒精度，按 float 给）
                    let now = now_millis();
                    std::ptr::write(sptr.add(sp), Value::Float((now - self.time_base) / 1000.0));
                    sp += 1;
                }
                Op::TimeReset => {
                    self.time_base = now_millis();
                }
                /* ── 计数循环 ── */
                Op::RangeInit { var, is_global, inclusive } => {
                    if sp < 3 {
                        return Err(self.err_at(ip, "for 循环的起止值不完整"));
                    }
                    sp -= 1;
                    let step = take(sptr, sp);
                    sp -= 1;
                    let to = take(sptr, sp);
                    sp -= 1;
                    let from = take(sptr, sp);
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
                    let it = take(sptr, sp);
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
                Op::TryBegin(h) => {
                    // 记下现场：出错时 run() 会把这三样倒回来再跳到 h
                    self.handlers.push(Handler {
                        ip: h,
                        sp,
                        frames_len: self.frames.len(),
                        iters_len: self.iters.len(),
                        ranges_len: self.ranges.len(),
                    });
                }
                Op::TryEnd => {
                    self.handlers.pop();
                }
                Op::Throw => {
                    sp -= 1;
                    let v = take(sptr, sp);
                    // read 之后这一格就是"已移走"的残值，必须立刻置 null，
                    // 否则析构时会被 drop 第二次
                    std::ptr::write(sptr.add(sp), Value::Null);
                    let shown = v.to_display();
                    self.thrown = Some(v);
                    return Err(self.err_at(ip, shown));
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
    // 函数里没跑完的 try 一并收掉，否则它会一直躺在处理器栈上
    if vm.handlers.len() != frame.hdl_base as usize {
        vm.handlers.truncate(frame.hdl_base as usize);
    }
    drop_from(sptr, sp, frame.base as usize);
    *base = vm.frames.last().map(|f| f.locals_base as usize).unwrap_or(0);
    if vm.frames.is_empty() {
        return FrameExit::Top;
    }
    FrameExit::Cont(frame.ret_ip as usize)
}

/// 整数整除（向下取整）。Rust 的 `/` 是向零截断，做不到 `-7 // 2 = -4`，
/// 所以按"商的符号和除数不一致且有余数就再减 1"补正。
#[inline]
fn floor_div_i64(x: i64, y: i64) -> i64 {
    let q = x / y;
    if x % y != 0 && ((x < 0) != (y < 0)) {
        q - 1
    } else {
        q
    }
}

/// 当前时间的毫秒数（给 time read / time reset 用）。
/// 浏览器（wasm）里没有系统时钟，给 0：time read 会一直读到 0，不会崩。
#[cfg(not(target_arch = "wasm32"))]
fn now_millis() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as f64)
        .unwrap_or(0.0)
}

#[cfg(target_arch = "wasm32")]
fn now_millis() -> f64 {
    0.0
}

/// 把 [from, sp) 这段值取成一个 Vec，栈顶退到 from
#[inline]
unsafe fn take_tail(ptr: *mut Value, sp: &mut usize, from: usize) -> Vec<Value> {
    let mut out = Vec::with_capacity(sp.saturating_sub(from));
    let mut i = from;
    while i < *sp {
        out.push(take(ptr, i));
        i += 1;
    }
    *sp = from;
    out
}

/// 从栈槽里"拿走"一个值。
/// 槽里剩下的旧字节不用管：OpStack 只析构 [0, sp) 这段活值，
/// 搬走之后这一格就不再被 drop（这也是它能做到零开销的原因）。
#[inline]
unsafe fn take(ptr: *mut Value, i: usize) -> Value {
    std::ptr::read(ptr.add(i))
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

pub(crate) fn arith(op: Op, a: &Value, b: &Value) -> Result<Value, String> {
    // 指针运算：p + n / p - n / p - q（两个指针相减 = 隔了几格）
    if op == Op::Add || op == Op::Sub {
        if let Value::Ptr(pa) = a {
            if let Value::Ptr(pb) = b {
                if op == Op::Sub {
                    let (ia, ib) = (pa.borrow().idx as i64, pb.borrow().idx as i64);
                    return Ok(Value::Int(ia - ib));
                }
                return Err("两个指针不能相加".into());
            }
            let n = b.as_num().ok_or_else(|| format!("指针只能加减整数格数，得到 {}", b.to_display()))?;
            return Ok(Value::shift_ptr(pa, if op == Op::Sub { -(n as i64) } else { n as i64 }));
        }
        if op == Op::Add {
            if let Value::Ptr(pb) = b {
                let n = a.as_num().ok_or_else(|| format!("指针只能加减整数格数，得到 {}", a.to_display()))?;
                return Ok(Value::shift_ptr(pb, n as i64));
            }
        }
    }
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
            Op::IDiv => {
                if y == 0 {
                    return Err("整除的除数为 0".into());
                }
                Value::Int(floor_div_i64(x, y))
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
        // 整除：浮点也向下取整（7.5 // 2 = 3.0，和 Python 一致）
        Op::IDiv => {
            if y == 0.0 {
                return Err("整除的除数为 0".into());
            }
            Value::Float((x / y).floor())
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
            match &m.target {
                PtrTarget::Cells(c) => {
                    let cells = c.borrow();
                    // 和 C 一致：p[k] 是从当前位置挪 k 格，负数是往回
                    let real = m.idx as i64 + match idx {
                        Value::Int(n) => *n,
                        Value::Float(f) => *f as i64,
                        other => return Err(format!("指针下标必须是数字，得到 {}", other.to_display())),
                    };
                    if real < 0 || real as usize >= cells.len() {
                        return Err(format!(
                            "下标 {}（第 {} 格）越界了（这块一共 {} 格）", idx.to_display(), real, cells.len()));
                    }
                    Ok(cells[real as usize].clone())
                }
                _ => {
                    if let Value::Int(0) = idx { /* &变量 只有一格 */ } else {
                        return Err("&变量 的指针只有一格，p[0] 就是变量本身".into());
                    }
                    Err("&变量 的指针要在 VM 里直接读（不走这里）".into())
                }
            }
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
            let m = p.borrow();
            if m.freed {
                return Err("这块内存已经 free 了，不能再写".into());
            }
            match &m.target {
                PtrTarget::Cells(c) => {
                    let mut cells = c.borrow_mut();
                    let real = m.idx as i64 + match idx {
                        Value::Int(n) => *n,
                        Value::Float(f) => *f as i64,
                        other => return Err(format!("指针下标必须是数字，得到 {}", other.to_display())),
                    };
                    if real < 0 || real as usize >= cells.len() {
                        return Err(format!(
                            "下标 {}（第 {} 格）越界了（这块一共 {} 格）", idx.to_display(), real, cells.len()));
                    }
                    cells[real as usize] = val.clone_deep();
                    Ok(())
                }
                _ => Err("&变量 的指针只有一格（*p 直接读写变量本身）".into()),
            }
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
            let n = match &m.target {
                PtrTarget::Cells(c) => c.borrow().len().saturating_sub(m.idx),
                _ => 0,
            };
            drop(m);
            Ok((IterSrc::Ptr(Rc::clone(p)), n))
        }
        other => Err(format!("{} 不能用于 for 遍历", other.to_display())),
    }
}
