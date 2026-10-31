//! 内建函数与内建方法。
//!
//! 名字与 TS 版 `builtins.ts` / `methods.ts` 保持一致，保证老脚本能直接跑。
//! 这一层只做纯值变换，不碰 IO / 时间 / 随机数以外的世界状态；
//! 输出走 VM 的 Print 指令，所以这里没有 print。

use crate::value::{Dict, Value};
use std::rc::Rc;

type R<T> = Result<T, String>;

fn need(args: &[Value], n: usize, name: &str) -> R<()> {
    if args.len() < n {
        return Err(format!("{} 需要 {} 个参数，实际给了 {}", name, n, args.len()));
    }
    Ok(())
}

fn num(v: &Value, name: &str) -> R<f64> {
    v.as_num().ok_or_else(|| format!("{} 需要数字，实际是 {}", name, v.to_display()))
}

fn int_of(v: &Value, name: &str) -> R<i64> {
    v.as_int().ok_or_else(|| format!("{} 需要整数，实际是 {}", name, v.to_display()))
}

fn s_of(v: &Value) -> String {
    match v {
        Value::Str(s) => s.as_str().to_string(),
        other => other.to_display(),
    }
}

/// 名字是不是内建函数（给 IDE 补全和运行时校验共用）
#[allow(dead_code)]
pub fn is_builtin(name: &str) -> bool {
    BUILTIN_NAMES.contains(&name)
}

/// 拿到内建名字的 'static 版本，供字节码里直接引用（不复制字符串）
pub fn builtin_static(name: &str) -> Option<&'static str> {
    BUILTIN_NAMES.iter().find(|n| **n == name).copied()
}

pub const BUILTIN_NAMES: [&str; 43] = [
    "alloc", "free",
    "len", "str", "int", "float", "bool", "type", "abs", "floor", "ceil", "round", "sqrt", "pow",
    "mod", "min", "max", "random", "rand", "sin", "cos", "tan", "sqrt2", "log", "exp", "join",
    "upper", "lower", "trim", "split", "substr", "strlen", "contain", "indexof", "char", "listin",
    "dictin", "not", "and", "or", "eq", "lt", "gt",
];

pub fn call_builtin(name: &str, args: &[Value]) -> R<Value> {
    match name {
        "len" => {
            need(args, 1, "len")?;
            Ok(Value::Int(match &args[0] {
                Value::Str(s) => s.chars().count() as i64,
                Value::List(l) => l.borrow().len() as i64,
                Value::Dict(d) => d.borrow().len() as i64,
                Value::Ptr(p) => p.borrow().buf.len() as i64,
                other => return Err(format!("len 不接受 {}", other.to_display())),
            }))
        }
        "alloc" => {
            need(args, 1, "alloc")?;
            let n = int_of(&args[0], "alloc")?;
            if n < 0 {
                return Err(format!("alloc 要一个不小于 0 的格子数，实际是 {}", n));
            }
            // 一亿个格子已经远超脚本能用的范围，直接拦住，免得把进程吃光
            if n > 10_000_000 {
                return Err(format!("alloc 一次要了 {} 格，太多了（上限 1000 万格）", n));
            }
            Ok(Value::alloc(n as usize))
        }
        "free" => {
            need(args, 1, "free")?;
            match &args[0] {
                Value::Ptr(p) => {
                    let mut m = p.borrow_mut();
                    if m.freed {
                        return Err("这块内存已经 free 过了，不用再 free 一次".into());
                    }
                    m.buf.clear();
                    m.buf.shrink_to_fit();
                    m.freed = true;
                    Ok(Value::Null)
                }
                other => Err(format!("free 要一个指针，实际是 {}", other.type_name())),
            }
        }
        "str" => {
            need(args, 1, "str")?;
            Ok(Value::string(s_of(&args[0])))
        }
        "int" => {
            need(args, 1, "int")?;
            Ok(Value::Int(int_of(&args[0], "int")?))
        }
        "float" => {
            need(args, 1, "float")?;
            Ok(Value::Float(num(&args[0], "float")?))
        }
        "bool" => {
            need(args, 1, "bool")?;
            Ok(Value::Bool(args[0].truthy()))
        }
        "type" => {
            need(args, 1, "type")?;
            Ok(Value::str(args[0].type_name()))
        }
        "abs" => {
            need(args, 1, "abs")?;
            Ok(match &args[0] {
                Value::Int(i) => Value::Int(i.abs()),
                other => Value::Float(num(other, "abs")?.abs()),
            })
        }
        "floor" => {
            need(args, 1, "floor")?;
            Ok(Value::Int(num(&args[0], "floor")?.floor() as i64))
        }
        "ceil" => {
            need(args, 1, "ceil")?;
            Ok(Value::Int(num(&args[0], "ceil")?.ceil() as i64))
        }
        "round" => {
            need(args, 1, "round")?;
            Ok(Value::Int(num(&args[0], "round")?.round() as i64))
        }
        "sqrt" | "sqrt2" => {
            need(args, 1, "sqrt")?;
            Ok(Value::Float(num(&args[0], "sqrt")?.sqrt()))
        }
        "pow" => {
            need(args, 2, "pow")?;
            let r = num(&args[0], "pow")?.powf(num(&args[1], "pow")?);
            Ok(if r.fract() == 0.0 && r.abs() < 9.007_199_254_740_992e15 {
                Value::Int(r as i64)
            } else {
                Value::Float(r)
            })
        }
        "mod" => {
            need(args, 2, "mod")?;
            let y = num(&args[1], "mod")?;
            if y == 0.0 {
                return Err("取余的除数为 0".into());
            }
            Ok(Value::Float(num(&args[0], "mod")? % y))
        }
        "min" | "max" => {
            need(args, 1, name)?;
            let vals: Vec<f64> = args.iter().map(|v| num(v, name)).collect::<R<_>>()?;
            let pick = if name == "min" {
                vals.iter().cloned().fold(f64::INFINITY, f64::min)
            } else {
                vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
            };
            Ok(as_number(pick, args))
        }
        "random" => Ok(Value::Float(pseudo_random())),
        "rand" => {
            need(args, 2, "rand")?;
            let a = int_of(&args[0], "rand")?;
            let b = int_of(&args[1], "rand")?;
            let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
            let span = (hi - lo + 1).max(1) as f64;
            Ok(Value::Int(lo + (pseudo_random() * span) as i64))
        }
        "sin" => Ok(Value::Float(num(&arg0(args, "sin")?, "sin")?.sin())),
        "cos" => Ok(Value::Float(num(&arg0(args, "cos")?, "cos")?.cos())),
        "tan" => Ok(Value::Float(num(&arg0(args, "tan")?, "tan")?.tan())),
        "log" => Ok(Value::Float(num(&arg0(args, "log")?, "log")?.ln())),
        "exp" => Ok(Value::Float(num(&arg0(args, "exp")?, "exp")?.exp())),
        "join" => {
            let mut s = String::new();
            for a in args {
                s.push_str(&a.to_display());
            }
            Ok(Value::string(s))
        }
        "upper" => Ok(Value::string(s_of(&arg0(args, "upper")?).to_uppercase())),
        "lower" => Ok(Value::string(s_of(&arg0(args, "lower")?).to_lowercase())),
        "trim" => Ok(Value::string(s_of(&arg0(args, "trim")?).trim().to_string())),
        "split" => {
            need(args, 2, "split")?;
            let s = s_of(&args[0]);
            let sep = s_of(&args[1]);
            let parts: Vec<Value> = if sep.is_empty() {
                s.chars().map(|c| Value::string(c.to_string())).collect()
            } else {
                s.split(sep.as_str()).map(|p| Value::string(p.to_string())).collect()
            };
            Ok(Value::list(parts))
        }
        "substr" => {
            need(args, 2, "substr")?;
            let s = s_of(&args[0]);
            let chars: Vec<char> = s.chars().collect();
            let start = int_of(&args[1], "substr")?.max(0) as usize;
            let len = if args.len() >= 3 {
                int_of(&args[2], "substr")?.max(0) as usize
            } else {
                chars.len().saturating_sub(start)
            };
            let end = (start + len).min(chars.len());
            if start >= chars.len() {
                return Ok(Value::str(""));
            }
            let out: String = chars[start..end].iter().collect();
            Ok(Value::string(out))
        }
        "strlen" => {
            need(args, 1, "strlen")?;
            Ok(Value::Int(s_of(&args[0]).chars().count() as i64))
        }
        "contain" => {
            need(args, 2, "contain")?;
            Ok(Value::Bool(s_of(&args[0]).contains(&s_of(&args[1]))))
        }
        "indexof" => {
            need(args, 2, "indexof")?;
            let s = s_of(&args[0]);
            let sub = s_of(&args[1]);
            Ok(Value::Int(match s.find(sub.as_str()) {
                Some(b) => s[..b].chars().count() as i64,
                None => -1,
            }))
        }
        "char" => {
            need(args, 2, "char")?;
            let s = s_of(&args[0]);
            let i = int_of(&args[1], "char")?;
            let chars: Vec<char> = s.chars().collect();
            let idx = if i < 0 { i + chars.len() as i64 } else { i };
            if idx < 0 || idx as usize >= chars.len() {
                return Err(format!("char 下标 {} 越界", i));
            }
            Ok(Value::string(chars[idx as usize].to_string()))
        }
        "listin" => {
            need(args, 2, "listin")?;
            match &args[1] {
                Value::List(l) => Ok(Value::Bool(l.borrow().iter().any(|x| x.deep_eq(&args[0])))),
                other => Err(format!("listin 的第二个参数要是列表，实际是 {}", other.to_display())),
            }
        }
        "dictin" => {
            need(args, 2, "dictin")?;
            match &args[0] {
                Value::Dict(d) => Ok(Value::Bool(d.borrow().has(&args[1].to_display()))),
                other => Err(format!("dictin 的第一个参数要是字典，实际是 {}", other.to_display())),
            }
        }
        "not" => {
            need(args, 1, "not")?;
            Ok(Value::Bool(!args[0].truthy()))
        }
        "and" => {
            need(args, 2, "and")?;
            Ok(Value::Bool(args[0].truthy() && args[1].truthy()))
        }
        "or" => {
            need(args, 2, "or")?;
            Ok(Value::Bool(args[0].truthy() || args[1].truthy()))
        }
        "eq" => {
            need(args, 2, "eq")?;
            Ok(Value::Bool(args[0].deep_eq(&args[1])))
        }
        "lt" | "gt" => {
            need(args, 2, name)?;
            if let (Value::Str(a), Value::Str(b)) = (&args[0], &args[1]) {
                return Ok(Value::Bool(if name == "lt" { a < b } else { a > b }));
            }
            let a = num(&args[0], name)?;
            let b = num(&args[1], name)?;
            Ok(Value::Bool(if name == "lt" { a < b } else { a > b }))
        }
        other => Err(format!("没有叫「{}」的内建函数", other)),
    }
}

fn arg0(args: &[Value], name: &str) -> R<Value> {
    need(args, 1, name)?;
    Ok(args[0].clone())
}

fn as_number(v: f64, args: &[Value]) -> Value {
    if args.iter().all(|x| matches!(x, Value::Int(_))) && v.fract() == 0.0 {
        Value::Int(v as i64)
    } else {
        Value::Float(v)
    }
}

/// 伪随机数。用系统时间做种子，够脚本玩；不追求密码学质量。
/// 浏览器（wasm）里没有系统时钟，退化成固定种子——总不能为了 rand 崩掉。
fn pseudo_random() -> f64 {
    use std::cell::Cell;
    thread_local! {
        static SEED: Cell<u64> = Cell::new(0);
    }
    SEED.with(|s| {
        let mut x = s.get();
        if x == 0 {
            x = seed_value() | 1;
        }
        // xorshift64*
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        s.set(x);
        let r = x.wrapping_mul(0x2545F4914F6CDD1D);
        (r >> 11) as f64 / (1u64 << 53) as f64
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn seed_value() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9e3779b97f4a7c15)
}

#[cfg(target_arch = "wasm32")]
fn seed_value() -> u64 {
    // 浏览器里 std::time 不可用，用个固定种子；同一个页面里序列照样不重复
    thread_local! {
        static N: std::cell::Cell<u64> = std::cell::Cell::new(0x9e3779b97f4a7c15);
    }
    N.with(|n| {
        let v = n.get();
        n.set(v.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407));
        v
    })
}

/* ───────────────────────── 内建方法 ───────────────────────── */

pub fn call_method(recv: &Value, name: &str, args: &[Value]) -> R<Value> {
    match recv {
        Value::Str(s) => str_method(s, name, args),
        Value::List(l) => list_method(l, name, args),
        Value::Dict(d) => dict_method(d, name, args),
        Value::Ptr(p) => match name {
            "len" => Ok(Value::Int(p.borrow().buf.len() as i64)),
            "free" => {
                let mut m = p.borrow_mut();
                if m.freed {
                    return Err("这块内存已经 free 过了，不用再 free 一次".into());
                }
                m.buf.clear();
                m.freed = true;
                Ok(Value::Null)
            }
            other => Err(format!("指针没有方法「{}」（能用的是 len / free）", other)),
        },
        other => Err(format!("{} 没有方法「{}」", other.to_display(), name)),
    }
}

fn str_method(s: &Rc<String>, name: &str, args: &[Value]) -> R<Value> {
    match name {
        "upper" => Ok(Value::string(s.to_uppercase())),
        "lower" => Ok(Value::string(s.to_lowercase())),
        "trim" => Ok(Value::string(s.trim().to_string())),
        "len" => Ok(Value::Int(s.chars().count() as i64)),
        "contains" => {
            need(args, 1, "contains")?;
            Ok(Value::Bool(s.contains(&s_of(&args[0]))))
        }
        "starts_with" => {
            need(args, 1, "starts_with")?;
            Ok(Value::Bool(s.starts_with(&s_of(&args[0]))))
        }
        "ends_with" => {
            need(args, 1, "ends_with")?;
            Ok(Value::Bool(s.ends_with(&s_of(&args[0]))))
        }
        "find" => {
            need(args, 1, "find")?;
            let sub = s_of(&args[0]);
            Ok(Value::Int(match s.find(sub.as_str()) {
                Some(b) => s[..b].chars().count() as i64,
                None => -1,
            }))
        }
        "replace" => {
            need(args, 2, "replace")?;
            let from = s_of(&args[0]);
            let to = s_of(&args[1]);
            Ok(Value::string(s.replace(from.as_str(), to.as_str())))
        }
        "split" => {
            need(args, 1, "split")?;
            let sep = s_of(&args[0]);
            let parts: Vec<Value> = if sep.is_empty() {
                s.chars().map(|c| Value::string(c.to_string())).collect()
            } else {
                s.split(sep.as_str()).map(|p| Value::string(p.to_string())).collect()
            };
            Ok(Value::list(parts))
        }
        "substr" => {
            need(args, 1, "substr")?;
            let chars: Vec<char> = s.chars().collect();
            let start = int_of(&args[0], "substr")?.max(0) as usize;
            let len = if args.len() >= 2 {
                int_of(&args[1], "substr")?.max(0) as usize
            } else {
                chars.len().saturating_sub(start)
            };
            if start >= chars.len() {
                return Ok(Value::str(""));
            }
            let end = (start + len).min(chars.len());
            let out: String = chars[start..end].iter().collect();
            Ok(Value::string(out))
        }
        "repeat" => {
            need(args, 1, "repeat")?;
            let n = int_of(&args[0], "repeat")?.max(0) as usize;
            Ok(Value::string(s.repeat(n)))
        }
        "chars" => Ok(Value::list(
            s.chars().map(|c| Value::string(c.to_string())).collect(),
        )),
        "to_int" => Ok(Value::Int(
            s.trim().parse::<f64>().map(|x| x as i64).map_err(|_| format!("「{}」转不成整数", s))?,
        )),
        "to_float" => Ok(Value::Float(
            s.trim().parse::<f64>().map_err(|_| format!("「{}」转不成小数", s))?,
        )),
        other => Err(format!("字符串没有方法「{}」", other)),
    }
}

fn list_method(l: &crate::value::ListRef, name: &str, args: &[Value]) -> R<Value> {
    match name {
        "push" => {
            need(args, 1, "push")?;
            l.borrow_mut().push(args[0].clone_deep());
            Ok(Value::Null)
        }
        "pop" => {
            let v = l.borrow_mut().pop();
            v.ok_or_else(|| "空列表不能 pop".to_string())
        }
        "insert" => {
            need(args, 2, "insert")?;
            let mut items = l.borrow_mut();
            let i = int_of(&args[0], "insert")?;
            let idx = if i < 0 { (items.len() as i64 + i + 1).max(0) } else { i } as usize;
            let idx = idx.min(items.len());
            items.insert(idx, args[1].clone_deep());
            Ok(Value::Null)
        }
        "remove" => {
            need(args, 1, "remove")?;
            let mut items = l.borrow_mut();
            let i = int_of(&args[0], "remove")?;
            let idx = if i < 0 { i + items.len() as i64 } else { i };
            if idx < 0 || idx as usize >= items.len() {
                return Err(format!("remove 下标 {} 越界", i));
            }
            Ok(items.remove(idx as usize))
        }
        "index_of" => {
            need(args, 1, "index_of")?;
            let items = l.borrow();
            Ok(Value::Int(
                items.iter().position(|x| x.deep_eq(&args[0])).map(|i| i as i64).unwrap_or(-1),
            ))
        }
        "contains" => {
            need(args, 1, "contains")?;
            let items = l.borrow();
            Ok(Value::Bool(items.iter().any(|x| x.deep_eq(&args[0]))))
        }
        "len" => Ok(Value::Int(l.borrow().len() as i64)),
        "join" => {
            let sep = if args.is_empty() { ",".to_string() } else { s_of(&args[0]) };
            let items = l.borrow();
            let parts: Vec<String> = items.iter().map(|x| x.to_display()).collect();
            Ok(Value::string(parts.join(&sep)))
        }
        "reverse" => {
            l.borrow_mut().reverse();
            Ok(Value::Null)
        }
        "sort" => {
            let mut items = l.borrow_mut();
            items.sort_by(|a, b| {
                let (x, y) = (a.to_display(), b.to_display());
                match (a, b) {
                    (Value::Int(_) | Value::Float(_), Value::Int(_) | Value::Float(_)) => a
                        .as_num()
                        .partial_cmp(&b.as_num())
                        .unwrap_or(std::cmp::Ordering::Equal),
                    _ => x.cmp(&y),
                }
            });
            Ok(Value::Null)
        }
        "sum" => {
            let items = l.borrow();
            let mut acc_i: i64 = 0;
            let mut acc_f: f64 = 0.0;
            let mut all_int = true;
            for x in items.iter() {
                match x {
                    Value::Int(i) => {
                        acc_i += *i;
                        acc_f += *i as f64;
                    }
                    other => {
                        all_int = false;
                        acc_f += num(other, "sum")?;
                    }
                }
            }
            Ok(if all_int { Value::Int(acc_i) } else { Value::Float(acc_f) })
        }
        "slice" => {
            need(args, 1, "slice")?;
            let items = l.borrow();
            let len = items.len() as i64;
            let mut start = int_of(&args[0], "slice")?;
            let mut end = if args.len() >= 2 { int_of(&args[1], "slice")? } else { len };
            if start < 0 {
                start += len;
            }
            if end < 0 {
                end += len;
            }
            let s = start.clamp(0, len) as usize;
            let e = end.clamp(0, len) as usize;
            let out: Vec<Value> = if s < e { items[s..e].to_vec() } else { Vec::new() };
            Ok(Value::list(out))
        }
        other => Err(format!("列表没有方法「{}」", other)),
    }
}

fn dict_method(d: &crate::value::DictRef, name: &str, args: &[Value]) -> R<Value> {
    match name {
        "keys" => Ok(Value::list(
            d.borrow().keys().iter().map(|k| Value::string(k.to_string())).collect(),
        )),
        "values" => Ok(Value::list(d.borrow().entries().map(|(_, v)| v.clone()).collect())),
        "has" => {
            need(args, 1, "has")?;
            Ok(Value::Bool(d.borrow().has(&args[0].to_display())))
        }
        "get" => {
            need(args, 1, "get")?;
            let k = args[0].to_display();
            match d.borrow().get(&k) {
                Some(v) => Ok(v),
                None => Ok(args.get(1).cloned().unwrap_or(Value::Null)),
            }
        }
        "remove" => {
            need(args, 1, "remove")?;
            let k = args[0].to_display();
            d.borrow_mut().remove(&k);
            Ok(Value::Null)
        }
        "len" => Ok(Value::Int(d.borrow().len() as i64)),
        other => Err(format!("字典没有方法「{}」", other)),
    }
}

/// 让空字典也能被构造（供 VM 的 BuildDict 之外的地方用）
pub fn empty_dict() -> Value {
    Value::Dict(Rc::new(std::cell::RefCell::new(Dict::new())))
}
