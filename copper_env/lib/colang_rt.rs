// ── 运行时前奏：全部复用 float-lang 的实现，语义和 VM 完全一致 ──
pub use std::rc::Rc;
pub use float_lang::builtins::{call_builtin, str_method, list_method, dict_method};
pub use float_lang::value::Dict;
pub use float_lang::bytecode::Op;
pub use float_lang::vm::{arith, compare, compound_value, index_get, index_set, member_get, member_set};

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

/* ── 捕获单元 + 指针（和 Rust AOT 的 aot.rs 同一套，出厂编译器也必须有） ──
 * 嵌套函数要按引用共享外层变量（改了外层也要看得见），所以被捕获的变量、
 * 以及被 & 取过地址的变量，都存成"一格的 cell"，函数之间传的是同一个 Rc。 */
pub use float_lang::value::{MemBlock, PtrTarget};
pub type Cell = Rc<std::cell::RefCell<Vec<V>>>;

#[inline]
pub fn cell_new(v: V) -> Cell {
    Rc::new(std::cell::RefCell::new(vec![v]))
}
#[inline]
pub fn cell_get(c: &Cell) -> V {
    c.borrow()[0].clone()
}
#[inline]
pub fn cell_set(c: &Cell, v: V) {
    c.borrow_mut()[0] = v;
}

/// &变量：直接绑到那个变量所在的 cell 上（读写都回到变量本身）
pub fn ptr_of_cell(c: Cell) -> V {
    V::Ptr(Rc::new(std::cell::RefCell::new(MemBlock {
        target: PtrTarget::Cells(c),
        idx: 0,
        owned: false,
        freed: false,
        base: 0,
    })))
}
/// &全局变量：绑到全局槽
pub fn ptr_of_global(i: usize) -> V {
    V::Ptr(Rc::new(std::cell::RefCell::new(MemBlock {
        target: PtrTarget::Global(i),
        idx: 0,
        owned: false,
        freed: false,
        base: 0,
    })))
}
/// &列表[i]：和 VM 的 AddrIdx 一样共享同一个 Vec
pub fn ptr_of_index(base: &V, i: i64, ln: u32, cl: u32) -> R<V> {
    match base {
        V::List(l) => {
            let len = l.borrow().len() as i64;
            if i < 0 || i >= len {
                return Err(fe(ln, cl, format!("& 取地址：第 {} 格越界了（这块一共 {} 格）", i, len)));
            }
            Ok(V::Ptr(Rc::new(std::cell::RefCell::new(MemBlock {
                target: PtrTarget::Cells(l.clone()),
                idx: i as usize,
                owned: false,
                freed: false,
                base: 0,
            }))))
        }
        other => Err(fe(ln, cl, format!("& 只能对列表元素或指针格子取地址，得到 {}", other.to_display()))),
    }
}

pub fn ptr_get(g: &Globals, p: &V, ln: u32, cl: u32) -> R<V> {
    match p {
        V::Ptr(pr) => {
            let b = pr.borrow();
            if b.freed {
                return Err(fe(ln, cl, String::from("这块内存已经 free 了，不能再读")));
            }
            match &b.target {
                PtrTarget::Cells(c) => {
                    let cells = c.borrow();
                    if b.idx >= cells.len() {
                        return Err(fe(
                            ln,
                            cl,
                            format!("指针指到第 {} 格，越界了（这块一共 {} 格）", b.idx, cells.len()),
                        ));
                    }
                    Ok(cells[b.idx].clone())
                }
                PtrTarget::Global(i) => Ok(g.v.get(*i).cloned().unwrap_or(V::Null)),
                PtrTarget::Local { .. } => Err(fe(ln, cl, String::from("这个指针指向的变量已经离开作用域了（悬空指针）"))),
            }
        }
        other => Err(fe(ln, cl, format!("* 只能解引用指针，得到 {}（取地址用 &）", other.to_display()))),
    }
}

pub fn ptr_set(g: &mut Globals, p: &V, v: V, ln: u32, cl: u32) -> R<V> {
    match p {
        V::Ptr(pr) => {
            let b = pr.borrow();
            if b.freed {
                return Err(fe(ln, cl, String::from("这块内存已经 free 了，不能再写")));
            }
            match &b.target {
                PtrTarget::Cells(c) => {
                    let len = c.borrow().len();
                    if b.idx >= len {
                        return Err(fe(
                            ln,
                            cl,
                            format!("指针指到第 {} 格，越界了（这块一共 {} 格）", b.idx, len),
                        ));
                    }
                    c.borrow_mut()[b.idx] = v.clone_deep();
                }
                PtrTarget::Global(i) => {
                    if *i < g.v.len() {
                        g.v[*i] = v.clone_deep();
                    }
                }
                PtrTarget::Local { .. } => {
                    return Err(fe(ln, cl, String::from("这个指针指向的变量已经离开作用域了（悬空指针）")))
                }
            }
            Ok(V::Null)
        }
        other => Err(fe(ln, cl, format!("* 只能对指针赋值，得到 {}", other.to_display()))),
    }
}


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

/// 读一行输入（`ask`）：和 VM 的控制台风格一致——回声非空输入；
/// 读到 EOF 就报错退出（和 VM 同一句话、同一退出码），不再拿空串让循环空转。
/// 只有在终端里才每行 flush（管道/重定向时 flush 是纯 syscall 开销）
pub fn ask_line() -> R<V> {
    use std::io::{BufRead, IsTerminal, Write};
    if std::io::stdin().is_terminal() {
        out_flush();
        let _ = std::io::stdout().flush();
    }
    let mut s = String::new();
    match std::io::stdin().lock().read_line(&mut s) {
        Ok(0) | Err(_) => {
            out_flush();
            eprintln!("\n运行出错：输入已经结束（EOF）：ask 没有拿到答案");
            std::process::exit(1);
        }
        Ok(_) => {}
    }
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
