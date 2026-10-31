//! 值模型。
//!
//! 与 TS 版的语义对齐，但有两个刻意的改进：
//!
//! 1. **整数是整数**。TS 版所有数字都是 f64，`int`/`float` 只是标注；
//!    这里分成 `Int(i64)` 和 `Float(f64)`: 整型运算走整数指令，不碰浮点单元。
//!    这一步是 Rust 版能比 V8 快的第一个结构性原因。
//! 2. **字典是有序的**，键集合与插入顺序都保得住（TS 用 Map，本来就保序）。
//!
//! 保持不变的语义：
//! * 变量赋值/传参是引用语义（和 Python / TS 版一致）
//! * 只有"往容器里塞值"才深拷贝（`clone_deep`），这一点 TS 版用 cloneVal 做过

use crate::ast::{Param, Stmt};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::rc::Rc;

pub type ListRef = Rc<RefCell<Vec<Value>>>;

/// 手动分配的内存块。
///
/// 外面套 Rc 是故意的：忘了 `free` 也不会漏——没人引用时自动回收；
/// `free` 之后再访问会得到明确的中文报错，而不是野指针。
/// 也就是说同一份脚本里动态内存和手动内存可以混着用，不需要任何开关。
pub struct MemBlock {
    pub buf: Vec<Value>,
    pub freed: bool,
}

pub type PtrRef = Rc<RefCell<MemBlock>>;
pub type DictRef = Rc<RefCell<Dict>>;

/// 用户函数（脚本里 function 定义的）
#[derive(Debug)]
pub struct UserFn {
    pub name: Rc<str>,
    /// 函数在 Chunk.fn_names 里的编号（帧只存编号，省得每帧克隆一次名字）
    pub id: u16,
    pub params: Vec<Param>,
    pub body: Rc<Vec<Stmt>>,
    pub file: Rc<str>,
    /// 函数在字节码里的入口地址
    pub entry: usize,
    /// 需要多少个局部槽（参数 + 函数体内的局部变量）
    pub nlocals: usize,
    /// 参数的默认值（没有默认值就是 Null）
    pub defaults: Vec<Value>,
}

/// 有序字典：HashMap 负责查，Vec 负责保序。
#[derive(Default, Debug)]
pub struct Dict {
    keys: Vec<Rc<str>>,
    map: HashMap<Rc<str>, Value>,
}

impl Dict {
    pub fn new() -> Dict {
        Dict::default()
    }

    pub fn set(&mut self, k: Rc<str>, v: Value) {
        if !self.map.contains_key(&k) {
            self.keys.push(Rc::clone(&k));
        }
        self.map.insert(k, v);
    }

    pub fn get(&self, k: &str) -> Option<Value> {
        self.map.get(k).cloned()
    }

    pub fn has(&self, k: &str) -> bool {
        self.map.contains_key(k)
    }

    pub fn remove(&mut self, k: &str) -> bool {
        if self.map.remove(k).is_some() {
            self.keys.retain(|x| &**x != k);
            true
        } else {
            false
        }
    }

    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    pub fn keys(&self) -> &[Rc<str>] {
        &self.keys
    }

    pub fn entries(&self) -> impl Iterator<Item = (&Rc<str>, &Value)> {
        self.keys.iter().filter_map(move |k| self.map.get(k).map(|v| (k, v)))
    }
}

#[derive(Clone)]
pub enum Value {
    Null,
    Int(i64),
    Float(f64),
    Bool(bool),
    /// 字符串用 `Rc<String>` 而不是 `Rc<str>`，是为了让 `s += "x"` 能走
    /// "独占时就地追加"的快路径（引用计数为 1 时 `Rc::get_mut` 拿到 `&mut String`）。
    /// 这是 CPython 处理字符串拼接的同一招，5 万次拼接从 O(n²) 降到 O(n)。
    Str(Rc<String>),
    List(ListRef),
    Dict(DictRef),
    /// 指针（手动分配的格子）
    Ptr(PtrRef),
    Func(Rc<UserFn>),
    /// 内建函数，直接用名字索引
    Native(&'static str),
}

impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_display())
    }
}

impl Value {
    pub fn str(s: &str) -> Value {
        Value::Str(Rc::new(s.to_string()))
    }

    /// 把已经拥有的 String 直接包成值（少一次拷贝）
    pub fn string(s: String) -> Value {
        Value::Str(Rc::new(s))
    }

    pub fn list(items: Vec<Value>) -> Value {
        Value::List(Rc::new(RefCell::new(items)))
    }

    pub fn dict() -> Value {
        Value::Dict(Rc::new(RefCell::new(Dict::new())))
    }

    /// 申请 n 个格子，初值都是 null
    pub fn alloc(n: usize) -> Value {
        Value::Ptr(Rc::new(RefCell::new(MemBlock { buf: vec![Value::Null; n], freed: false })))
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::Bool(_) => "bool",
            Value::Str(_) => "str",
            Value::List(_) => "list",
            Value::Dict(_) => "dict",
            Value::Ptr(_) => "ptr",
            Value::Func(_) | Value::Native(_) => "function",
        }
    }

    pub fn truthy(&self) -> bool {
        match self {
            Value::Null => false,
            Value::Bool(b) => *b,
            Value::Int(i) => *i != 0,
            Value::Float(f) => *f != 0.0 && !f.is_nan(),
            Value::Str(s) => !s.is_empty(),
            Value::List(l) => !l.borrow().is_empty(),
            Value::Dict(d) => !d.borrow().is_empty(),
            Value::Ptr(p) => !p.borrow().freed,
            Value::Func(_) | Value::Native(_) => true,
        }
    }

    /// 参与算术时统一取数值。字符串里是数字也认（和 TS 版 toNum 一致）。
    pub fn as_num(&self) -> Option<f64> {
        match self {
            Value::Int(i) => Some(*i as f64),
            Value::Float(f) => Some(*f),
            Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            Value::Str(s) => {
                let t = s.trim();
                if t.is_empty() {
                    None
                } else {
                    t.parse::<f64>().ok()
                }
            }
            _ => None,
        }
    }

    /// 整数化：整数直接给，浮点/布尔/数字串尽量转
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            Value::Float(f) => Some(*f as i64),
            Value::Bool(b) => Some(if *b { 1 } else { 0 }),
            Value::Str(s) => s.trim().parse::<f64>().ok().map(|x| x as i64),
            _ => None,
        }
    }

    pub fn to_display(&self) -> String {
        let mut seen = Vec::new();
        self.fmt_rec(&mut seen, 0)
    }

    fn fmt_rec(&self, seen: &mut Vec<usize>, depth: usize) -> String {
        const MAX_DEPTH: usize = 16;
        match self {
            Value::Null => "null".to_string(),
            Value::Bool(b) => if *b { "true" } else { "false" }.to_string(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => fmt_float(*f),
            Value::Str(s) => s.to_string(),
            Value::Func(f) => format!("<function {}>", f.name),
            Value::Native(n) => format!("<native {}>", n),
            Value::List(l) => {
                let id = Rc::as_ptr(l) as *const u8 as usize;
                if seen.contains(&id) {
                    return "…(循环)".to_string();
                }
                if depth >= MAX_DEPTH {
                    return "…".to_string();
                }
                seen.push(id);
                let s = {
                    let v = l.borrow();
                    let mut out = String::from("[");
                    for (i, x) in v.iter().enumerate() {
                        if i > 0 {
                            out.push_str(", ");
                        }
                        // 容器内的字符串带引号（与 TS 版一致）
                        match x {
                            Value::Str(st) => {
                                out.push('"');
                                out.push_str(st);
                                out.push('"');
                            }
                            _ => out.push_str(&x.fmt_rec(seen, depth + 1)),
                        }
                    }
                    out.push(']');
                    out
                };
                seen.pop();
                s
            }
            Value::Ptr(p) => {
                let m = p.borrow();
                if m.freed {
                    format!("<ptr {} 已释放>", m.buf.len())
                } else {
                    format!("<ptr {}>", m.buf.len())
                }
            }
            Value::Dict(d) => {
                let id = Rc::as_ptr(d) as *const u8 as usize;
                if seen.contains(&id) {
                    return "…(循环)".to_string();
                }
                if depth >= MAX_DEPTH {
                    return "…".to_string();
                }
                seen.push(id);
                let s = {
                    let m = d.borrow();
                    let mut out = String::from("{");
                    let mut first = true;
                    for (k, v) in m.entries() {
                        if !first {
                            out.push_str(", ");
                        }
                        first = false;
                        let _ = write!(out, "{}: ", k);
                        match v {
                            Value::Str(st) => {
                                out.push('"');
                                out.push_str(st);
                                out.push('"');
                            }
                            _ => out.push_str(&v.fmt_rec(seen, depth + 1)),
                        }
                    }
                    out.push('}');
                    out
                };
                seen.pop();
                s
            }
        }
    }

    /// 内容相等（容器按内容比，带环保护）
    pub fn deep_eq(&self, other: &Value) -> bool {
        let mut pairs = Vec::new();
        self.eq_rec(other, &mut pairs)
    }

    fn eq_rec(&self, other: &Value, pairs: &mut Vec<(usize, usize)>) -> bool {
        match (self, other) {
            (Value::Null, Value::Null) => true,
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Func(a), Value::Func(b)) => Rc::ptr_eq(a, b),
            (Value::Native(a), Value::Native(b)) => a == b,
            // 指针按"是不是同一块"比，不看内容
            (Value::Ptr(a), Value::Ptr(b)) => Rc::ptr_eq(a, b),
            // 数值跨类型比较：1 == 1.0 成立
            (a, b) => {
                let an = matches!(a, Value::Int(_) | Value::Float(_));
                let bn = matches!(b, Value::Int(_) | Value::Float(_));
                if an && bn {
                    return a.as_num() == b.as_num();
                }
                match (a, b) {
                    (Value::List(x), Value::List(y)) => {
                        let xa = Rc::as_ptr(x) as *const u8 as usize;
                        let ya = Rc::as_ptr(y) as *const u8 as usize;
                        if let Some(&(_, prev)) = pairs.iter().find(|(p, _)| *p == xa) {
                            return prev == ya;
                        }
                        pairs.push((xa, ya));
                        let xb = x.borrow();
                        let yb = y.borrow();
                        let ok = xb.len() == yb.len()
                            && xb.iter().zip(yb.iter()).all(|(m, n)| m.eq_rec(n, pairs));
                        pairs.pop();
                        ok
                    }
                    (Value::Dict(x), Value::Dict(y)) => {
                        let xa = Rc::as_ptr(x) as *const u8 as usize;
                        let ya = Rc::as_ptr(y) as *const u8 as usize;
                        if let Some(&(_, prev)) = pairs.iter().find(|(p, _)| *p == xa) {
                            return prev == ya;
                        }
                        pairs.push((xa, ya));
                        let xb = x.borrow();
                        let yb = y.borrow();
                        let ok = xb.len() == yb.len()
                            && xb.entries().all(|(k, v)| match yb.get(k) {
                                Some(w) => v.eq_rec(&w, pairs),
                                None => false,
                            });
                        pairs.pop();
                        ok
                    }
                    _ => false,
                }
            }
        }
    }

    /// 往容器里塞值时的深拷贝（自引用保持同一份，不无限展开）
    pub fn clone_deep(&self) -> Value {
        let mut memo: Vec<(usize, Value)> = Vec::new();
        self.clone_rec(&mut memo)
    }

    fn clone_rec(&self, memo: &mut Vec<(usize, Value)>) -> Value {
        match self {
            Value::List(l) => {
                let id = Rc::as_ptr(l) as *const u8 as usize;
                if let Some((_, v)) = memo.iter().find(|(p, _)| *p == id) {
                    return v.clone();
                }
                let out = Rc::new(RefCell::new(Vec::new()));
                memo.push((id, Value::List(Rc::clone(&out))));
                let items: Vec<Value> = l.borrow().iter().map(|x| x.clone_rec(memo)).collect();
                *out.borrow_mut() = items;
                Value::List(out)
            }
            // 指针是"手柄"，深拷贝时仍然指向同一块内存（和列表/字典不同）
            Value::Ptr(p) => Value::Ptr(Rc::clone(p)),
            Value::Dict(d) => {
                let id = Rc::as_ptr(d) as *const u8 as usize;
                if let Some((_, v)) = memo.iter().find(|(p, _)| *p == id) {
                    return v.clone();
                }
                let out = Rc::new(RefCell::new(Dict::new()));
                memo.push((id, Value::Dict(Rc::clone(&out))));
                let entries: Vec<(Rc<str>, Value)> = d
                    .borrow()
                    .entries()
                    .map(|(k, v)| (Rc::clone(k), v.clone_rec(memo)))
                    .collect();
                {
                    let mut m = out.borrow_mut();
                    for (k, v) in entries {
                        m.set(k, v);
                    }
                }
                Value::Dict(out)
            }
            other => other.clone(),
        }
    }
}

/// 浮点显示：先按 12 位有效数字收敛，避免 0.1+0.2 打出 0.30000000000000004
pub fn fmt_float(v: f64) -> String {
    if v.is_nan() {
        return "NaN".to_string();
    }
    if v.is_infinite() {
        return if v > 0.0 { "Infinity" } else { "-Infinity" }.to_string();
    }
    if v == v.trunc() && v.abs() < 9.007_199_254_740_992e15 {
        let i = v as i64;
        return i.to_string();
    }
    let rounded = (v * 1e12).round() / 1e12;
    format!("{}", rounded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 浮点显示收敛长尾() {
        assert_eq!(fmt_float(0.1 + 0.2), "0.3");
        assert_eq!(fmt_float(2.0), "2");
        assert_eq!(fmt_float(1.5), "1.5");
        assert_eq!(fmt_float(-0.5), "-0.5");
    }

    #[test]
    fn 列表格式化带引号() {
        let v = Value::list(vec![Value::Int(1), Value::str("a"), Value::Bool(true)]);
        assert_eq!(v.to_display(), r#"[1, "a", true]"#);
    }

    #[test]
    fn 顶层字符串不带引号() {
        assert_eq!(Value::str("abc").to_display(), "abc");
    }

    #[test]
    fn 自引用列表不会爆栈() {
        let l = Rc::new(RefCell::new(Vec::new()));
        l.borrow_mut().push(Value::List(Rc::clone(&l)));
        let v = Value::List(l);
        assert_eq!(v.to_display(), "[…(循环)]");
    }

    #[test]
    fn 内容相等() {
        let a = Value::list(vec![Value::Int(1), Value::Int(2)]);
        let b = Value::list(vec![Value::Int(1), Value::Int(2)]);
        assert!(a.deep_eq(&b));
        assert!(!a.deep_eq(&Value::list(vec![Value::Int(1)])));
    }

    #[test]
    fn 整数与浮点相等() {
        assert!(Value::Int(1).deep_eq(&Value::Float(1.0)));
    }

    #[test]
    fn 深拷贝是真拷贝() {
        let a = Value::list(vec![Value::Int(1)]);
        let b = a.clone_deep();
        if let (Value::List(x), Value::List(y)) = (&a, &b) {
            x.borrow_mut().push(Value::Int(2));
            assert_eq!(y.borrow().len(), 1);
        }
    }

    #[test]
    fn 字典保序() {
        let mut d = Dict::new();
        d.set(Rc::from("b"), Value::Int(1));
        d.set(Rc::from("a"), Value::Int(2));
        d.set(Rc::from("c"), Value::Int(3));
        let ks: Vec<String> = d.keys().iter().map(|k| k.to_string()).collect();
        assert_eq!(ks, vec!["b", "a", "c"]);
    }

    #[test]
    fn truthy_规则() {
        assert!(!Value::Null.truthy());
        assert!(!Value::Int(0).truthy());
        assert!(!Value::str("").truthy());
        assert!(Value::list(vec![]).truthy() == false);
        assert!(Value::Int(1).truthy());
    }
}
