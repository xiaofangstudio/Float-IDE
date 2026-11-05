//! 内建函数与内建方法。
//!
//! 名字与 TS 版 `builtins.ts` / `methods.ts` 保持一致，保证老脚本能直接跑。
//! 这一层只做纯值变换，不碰 IO / 时间 / 随机数以外的世界状态；
//! 输出走 VM 的 Print 指令，所以这里没有 print。

use crate::value::{PtrTarget, Dict, Value};
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
    canonical(name).is_some() || crate::stdlib::is_fn(name)
}

/// 拿到内建名字的 'static 版本，供字节码里直接引用（不复制字符串）。
/// 兼容别名会被归一成正式名字。
pub fn builtin_static(name: &str) -> Option<&'static str> {
    // 标准库的 `库.函数`：直接给它一个 'static 字符串（首次用到时泄漏一份，数量有限）
    if let Some(x) = crate::stdlib::static_name(name) {
        return Some(x);
    }
    let n = canonical(name)?;
    BUILTIN_NAMES.iter().find(|x| **x == n).copied()
}

/// 正式名字表。**全英文**——语言是给程序员用的，不搞易语言那套中文关键字。
pub const BUILTIN_NAMES: [&str; 101] = [
    "dictallkey", "dictallval", "sorted",
    "alloc", "free",
    "len", "str", "int", "float", "bool", "type", "abs", "floor", "ceil", "round", "sqrt", "pow",
    "mod", "min", "max", "idiv", "random", "rand", "range", "sin", "cos", "tan", "sqrt2", "log", "exp", "join",
    "upper", "lower", "trim", "split", "substr", "strlen", "contain", "indexof", "char", "listin",
    "dictin", "not", "and", "or", "eq", "lt", "gt",
    "add", "sub", "mul", "div",
    // 数学（对标 Python math 的能力，名字沿用大家熟的那批）
    "asin", "acos", "atan", "atan2",
    "log2", "log10", "log1p", "expm1",
    "hypot", "trunc", "sign",
    "gcd", "lcm", "factorial", "comb", "perm", "isqrt",
    "isfinite", "isinf", "isnan", "near", "degrees", "radians",
    "pi", "e", "tau", "inf", "nan",
    // 文件 / 时间 / 网络
    "readfile", "writefile", "appendfile", "hasfile",
    "now",
    "fetch", "post",
    // 操作系统
    "cwd", "chdir", "listdir", "makedir", "removedir", "removefile", "renamefile",
    "exists", "isfile", "isdir", "filesize", "getenv", "pathjoin", "runcmd",
];

/// 兼容别名（只保证老代码还能跑；不进补全、不进文档）。
/// 中文名字是早期实现留下的，正式写法一律英文。
const ALIASES: [(&str, &str); 7] = [
    ("读文件", "readfile"),
    ("写文件", "writefile"),
    ("加文件", "appendfile"),
    ("有文件", "hasfile"),
    ("时间戳", "now"),
    ("取网页", "fetch"),
    ("发网页", "post"),
];

fn canonical(name: &str) -> Option<&'static str> {
    if let Some(a) = ALIASES.iter().find(|(c, _)| *c == name) {
        return Some(a.1);
    }
    BUILTIN_NAMES.iter().find(|n| **n == name).copied()
}

pub fn call_builtin(name: &str, args: &[Value]) -> R<Value> {
    // 标准库函数是 `库.函数` 的形式，先交给 stdlib 分发（不认识再往下走）
    if name.contains('.') {
        if let Some(r) = crate::stdlib::call(name, args) {
            return r;
        }
    }
    // 兼容别名先归一：内核内部只认英文正式名
    let name: &str = match canonical(name) {
        Some(n) => n,
        None => name,
    };
    match name {
        "len" => {
            need(args, 1, "len")?;
            Ok(Value::Int(match &args[0] {
                Value::Str(s) => s.chars().count() as i64,
                Value::List(l) => l.borrow().len() as i64,
                Value::Dict(d) => d.borrow().len() as i64,
                Value::Ptr(p) => {
                    let m = p.borrow();
                    match &m.target {
                        PtrTarget::Cells(c) => (c.borrow().len().saturating_sub(m.idx)) as i64,
                        _ => 1,
                    }
                }
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
                    if !m.owned {
                        return Err("这不是 alloc 出来的内存（多半是 &列表[i] 或 &变量 拿到的地址），不能 free".into());
                    }
                    if let PtrTarget::Cells(c) = &m.target {
                        c.borrow_mut().clear();
                        c.borrow_mut().shrink_to_fit();
                    }
                    m.freed = true;
                    Ok(Value::Null)
                }
                other => Err(format!("free 要一个指针，实际是 {}", other.type_name())),
            }
        }
        "dictallkey" => {
            need(args, 1, "dictallkey")?;
            match &args[0] {
                Value::Dict(d) => {
                    let keys: Vec<Value> = d.borrow().keys().iter().map(|k| Value::string(k.to_string())).collect();
                    Ok(Value::list(keys))
                }
                other => Err(format!("dictallkey 要一个字典，实际是 {}", other.type_name())),
            }
        }
        "dictallval" => {
            need(args, 1, "dictallval")?;
            match &args[0] {
                Value::Dict(d) => {
                    let vals: Vec<Value> = d.borrow().entries().into_iter().map(|(_, v)| v.clone()).collect();
                    Ok(Value::list(vals))
                }
                other => Err(format!("dictallval 要一个字典，实际是 {}", other.type_name())),
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
        // 加减乘除：和运算符 + - * / 完全等价，方便写 `c = add a b` 这种老式写法
        "add" | "sub" | "mul" | "div" => {
            need(args, 2, name)?;
            let op = match name {
                "add" => crate::bytecode::Op::Add,
                "sub" => crate::bytecode::Op::Sub,
                "mul" => crate::bytecode::Op::Mul,
                _ => crate::bytecode::Op::Div,
            };
            let mut acc = args[0].clone();
            for v in &args[1..] {
                acc = crate::vm::arith(op, &acc, v)?;
            }
            Ok(acc)
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
            // 两种用法：min(3, 1, 2) 多参数，或 min([3, 1, 2]) 传一个列表
            let flat: Vec<Value> = if args.len() == 1 {
                match &args[0] {
                    Value::List(l) => l.borrow().clone(),
                    v => vec![v.clone()],
                }
            } else {
                args.to_vec()
            };
            let vals: Vec<f64> = flat.iter().map(|v| num(v, name)).collect::<R<_>>()?;
            let pick = if name == "min" {
                vals.iter().cloned().fold(f64::INFINITY, f64::min)
            } else {
                vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
            };
            Ok(as_number(pick, args))
        }
        "random" => Ok(Value::Float(pseudo_random())),
        // range(n) / range(a, b) / range(a, b, step)：生成数列，for 里最常用
        "range" => {
            need(args, 1, "range")?;
            if args.len() > 3 {
                return Err("range 最多三个参数：range(末尾) / range(起点, 末尾) / range(起点, 末尾, 步长)".into());
            }
            let (from, to, step) = match args.len() {
                1 => (0.0, num(&args[0], "range")?, 1.0),
                2 => (num(&args[0], "range")?, num(&args[1], "range")?, 1.0),
                _ => (num(&args[0], "range")?, num(&args[1], "range")?, num(&args[2], "range")?),
            };
            if step == 0.0 {
                return Err("range 的步长不能是 0".into());
            }
            // 全是整数就产出整数，不然 1..10 会变成一堆 1.0 2.0
            let all_int = matches!(&args[0], Value::Int(_))
                && (args.len() < 2 || matches!(&args[1], Value::Int(_)))
                && (args.len() < 3 || matches!(&args[2], Value::Int(_)));
            // 先估算个数：等生成完再拒绝，内存已经吃下去了
            let est = ((to - from) / step).max(0.0);
            if est > crate::limits::MAX_LIST_ITEMS as f64 {
                return Err(crate::limits::grow_error(
                    "range 生成的列表", est as usize, crate::limits::MAX_LIST_ITEMS));
            }
            let mut out: Vec<Value> = Vec::new();
            let mut v = from;
            while (step > 0.0 && v < to) || (step < 0.0 && v > to) {
                crate::limits::guard_list(out.len(), 1)?;
                out.push(if all_int { Value::Int(v as i64) } else { Value::Float(v) });
                v += step;
            }
            Ok(Value::list(out))
        }        "rand" => {
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
        "sorted" => {
            // 全局排序：返回排好序的**新**列表，原列表不动（和 xs.sort() 的就地排序互补）
            need(args, 1, "sorted")?;
            let l = match &args[0] {
                Value::List(l) => Rc::clone(l),
                other => return Err(format!("sorted() 要的是列表，收到 {}", other.type_name())),
            };
            let mut items: Vec<Value> = l.borrow().clone();
            sort_values(&mut items);
            Ok(Value::List(Rc::new(std::cell::RefCell::new(items))))
        }
        "join" => {
            let mut s = String::new();
            for a in args {
                let t = a.to_display();
                crate::limits::guard_string(s.len(), t.len())?;
                s.push_str(&t);
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
            crate::limits::guard_list(0, s.len().saturating_add(1))?;
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

        /* ── 数学：三角函数 / 对数 / 整数组合 / 判断 / 常量 ── */
        // idiv：整除（向下取整，和 Python 的 // 一个意思）。
        // 注意语言里的 // 是注释，所以这儿用函数名，不用运算符。
        "idiv" => {
            need(args, 2, "idiv")?;
            let a = num(&args[0], "idiv")?;
            let b = num(&args[1], "idiv")?;
            if b == 0.0 {
                return Err("idiv 的除数不能是 0".into());
            }
            let q = (a / b).floor();
            // 两个都是整数就给整数，沾了小数就给小数（跟 Python 一致）
            if matches!(args[0], Value::Int(_)) && matches!(args[1], Value::Int(_)) {
                Ok(Value::Int(q as i64))
            } else {
                Ok(Value::Float(q))
            }
        }
        "asin" => Ok(Value::Float(num(&arg0(args, "asin")?, "asin")?.asin())),
        "acos" => Ok(Value::Float(num(&arg0(args, "acos")?, "acos")?.acos())),
        "atan" => Ok(Value::Float(num(&arg0(args, "atan")?, "atan")?.atan())),
        "atan2" => {
            need(args, 2, "atan2")?;
            Ok(Value::Float(num(&args[0], "atan2")?.atan2(num(&args[1], "atan2")?)))
        }
        "log2" => Ok(Value::Float(num(&arg0(args, "log2")?, "log2")?.log2())),
        "log10" => Ok(Value::Float(num(&arg0(args, "log10")?, "log10")?.log10())),
        "log1p" => Ok(Value::Float(num(&arg0(args, "log1p")?, "log1p")?.ln_1p())),
        "expm1" => Ok(Value::Float(num(&arg0(args, "expm1")?, "expm1")?.exp_m1())),
        // hypot：任意多个数的平方和开方（直角三角形斜边，多维也能用）
        "hypot" => {
            need(args, 1, "hypot")?;
            let mut acc = 0.0f64;
            for a in args {
                let x = num(a, "hypot")?;
                acc += x * x;
            }
            Ok(Value::Float(acc.sqrt()))
        }
        "trunc" => Ok(Value::Int(num(&arg0(args, "trunc")?, "trunc")?.trunc() as i64)),
        "sign" => {
            need(args, 1, "sign")?;
            let x = num(&args[0], "sign")?;
            Ok(Value::Int(if x > 0.0 { 1 } else if x < 0.0 { -1 } else { 0 }))
        }
        "gcd" => {
            need(args, 2, "gcd")?;
            let (a, b) = (int_of(&args[0], "gcd")?.abs(), int_of(&args[1], "gcd")?.abs());
            Ok(Value::Int(gcd_i64(a, b)))
        }
        "lcm" => {
            need(args, 2, "lcm")?;
            let (a, b) = (int_of(&args[0], "lcm")?.abs(), int_of(&args[1], "lcm")?.abs());
            if a == 0 || b == 0 {
                return Ok(Value::Int(0));
            }
            Ok(Value::Int(a / gcd_i64(a, b) * b))
        }
        "factorial" => {
            need(args, 1, "factorial")?;
            let n = int_of(&args[0], "factorial")?;
            if n < 0 {
                return Err("factorial 要一个不小于 0 的整数".into());
            }
            if n > 20 {
                return Err(format!("factorial({}) 太大了（超过 20 会超出整数范围）", n));
            }
            let mut acc: i64 = 1;
            for i in 2..=n {
                acc *= i;
            }
            Ok(Value::Int(acc))
        }
        "comb" => {
            need(args, 2, "comb")?;
            let n = int_of(&args[0], "comb")?;
            let k = int_of(&args[1], "comb")?;
            if n < 0 || k < 0 {
                return Err("comb(n, k) 的两个参数都不能是负数".into());
            }
            if k > n {
                return Ok(Value::Int(0));
            }
            let k = k.min(n - k);
            let mut acc: i64 = 1;
            for i in 0..k {
                acc = acc * (n - i) / (i + 1);
            }
            Ok(Value::Int(acc))
        }
        "perm" => {
            need(args, 2, "perm")?;
            let n = int_of(&args[0], "perm")?;
            let k = int_of(&args[1], "perm")?;
            if n < 0 || k < 0 {
                return Err("perm(n, k) 的两个参数都不能是负数".into());
            }
            if k > n {
                return Ok(Value::Int(0));
            }
            let mut acc: i64 = 1;
            for i in 0..k {
                acc *= n - i;
            }
            Ok(Value::Int(acc))
        }
        "isqrt" => {
            need(args, 1, "isqrt")?;
            let n = int_of(&args[0], "isqrt")?;
            if n < 0 {
                return Err("isqrt 要一个不小于 0 的整数".into());
            }
            // 整数平方根：先浮点开方，再校正到精确值（大数时浮点会差一格）
            let mut r = (n as f64).sqrt() as i64;
            while r > 0 && r * r > n {
                r -= 1;
            }
            while (r + 1) * (r + 1) <= n {
                r += 1;
            }
            Ok(Value::Int(r))
        }
        "isfinite" => {
            need(args, 1, "isfinite")?;
            Ok(Value::Bool(num(&args[0], "isfinite")?.is_finite()))
        }
        "isinf" => {
            need(args, 1, "isinf")?;
            Ok(Value::Bool(num(&args[0], "isinf")?.is_infinite()))
        }
        "isnan" => {
            need(args, 1, "isnan")?;
            Ok(Value::Bool(num(&args[0], "isnan")?.is_nan()))
        }
        // near(a, b)：两个数是不是足够接近（比起 math.isclose 这串更好记）
        // 第三个参数可以不写，默认相对误差 1e-9
        "near" => {
            need(args, 2, "near")?;
            let a = num(&args[0], "near")?;
            let b = num(&args[1], "near")?;
            let tol = if args.len() >= 3 { num(&args[2], "near")?.abs() } else { 1e-9 };
            let diff = (a - b).abs();
            let scale = a.abs().max(b.abs()).max(1.0);
            Ok(Value::Bool(diff <= tol * scale))
        }
        "degrees" => Ok(Value::Float(num(&arg0(args, "degrees")?, "degrees")?.to_degrees())),
        "radians" => Ok(Value::Float(num(&arg0(args, "radians")?, "radians")?.to_radians())),
        "pi" => Ok(Value::Float(std::f64::consts::PI)),
        "e" => Ok(Value::Float(std::f64::consts::E)),
        "tau" => Ok(Value::Float(std::f64::consts::TAU)),
        "inf" => Ok(Value::Float(f64::INFINITY)),
        "nan" => Ok(Value::Float(f64::NAN)),

        /* ── 文件 ── 相对路径锚在主脚本所在目录 */
        "readfile" => {
            need(args, 1, "readfile")?;
            let p = crate::host::resolve_path(&s_of(&args[0]));
            std::fs::read_to_string(&p)
                .map(|s| Value::string(s))
                .map_err(|e| format!("readfile 读不到「{}」：{}", p.display(), io_msg(&e)))
        }
        "writefile" => {
            need(args, 2, "writefile")?;
            let p = crate::host::resolve_path(&s_of(&args[0]));
            std::fs::write(&p, s_of(&args[1]))
                .map(|_| Value::Bool(true))
                .map_err(|e| format!("writefile 写不进「{}」：{}", p.display(), io_msg(&e)))
        }
        "appendfile" => {
            need(args, 2, "appendfile")?;
            let p = crate::host::resolve_path(&s_of(&args[0]));
            use std::io::Write as _;
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&p)
                .and_then(|mut f| f.write_all(s_of(&args[1]).as_bytes()))
                .map(|_| Value::Bool(true))
                .map_err(|e| format!("appendfile 追加不进「{}」：{}", p.display(), io_msg(&e)))
        }
        "hasfile" => {
            need(args, 1, "hasfile")?;
            Ok(Value::Bool(crate::host::resolve_path(&s_of(&args[0])).is_file()))
        }

        /* ── 时间 ── */
        "now" => Ok(Value::Int(now_ms())),

        /* ── 网络 ── 实现由宿主注入（桌面版走 ureq，浏览器 demo 明确报错） */
        "fetch" => {
            need(args, 1, "fetch")?;
            let url = s_of(&args[0]);
            crate::host::do_fetch("GET", &url, None)
                .map(|s| Value::string(s))
                .map_err(|e| format!("fetch 失败（{}）：{}", url, e))
        }
        "post" => {
            need(args, 2, "post")?;
            let url = s_of(&args[0]);
            let body = s_of(&args[1]);
            crate::host::do_fetch("POST", &url, Some(&body))
                .map(|s| Value::string(s))
                .map_err(|e| format!("post 失败（{}）：{}", url, e))
        }

        /* ── 操作系统 ── 相对路径同样锚在主脚本目录 */
        "cwd" => Ok(Value::string(crate::host::base_dir_string())),
        // chdir 只改脚本自己的"当前目录"，不动整个进程的工作目录
        // （不然会把 IDE 自己的相对路径一起改掉）
        "chdir" => {
            need(args, 1, "chdir")?;
            let p = crate::host::resolve_path(&s_of(&args[0]));
            if !p.is_dir() {
                return Err(format!("chdir 找不到目录「{}」", p.display()));
            }
            crate::host::set_base(Some(&p));
            Ok(Value::Bool(true))
        }
        "listdir" => {
            let dir = if args.is_empty() {
                crate::host::resolve_path(".")
            } else {
                crate::host::resolve_path(&s_of(&args[0]))
            };
            let rd = std::fs::read_dir(&dir)
                .map_err(|e| format!("listdir 打不开「{}」：{}", dir.display(), io_msg(&e)))?;
            let mut names: Vec<Value> = Vec::new();
            for ent in rd.flatten() {
                names.push(Value::string(ent.file_name().to_string_lossy().to_string()));
            }
            names.sort_by(|a, b| a.to_display().cmp(&b.to_display()));
            Ok(Value::list(names))
        }
        "makedir" => {
            need(args, 1, "makedir")?;
            let p = crate::host::resolve_path(&s_of(&args[0]));
            std::fs::create_dir_all(&p)
                .map(|_| Value::Bool(true))
                .map_err(|e| format!("makedir 建不了「{}」：{}", p.display(), io_msg(&e)))
        }
        "removedir" => {
            need(args, 1, "removedir")?;
            let p = crate::host::resolve_path(&s_of(&args[0]));
            // 只删空目录：不提供递归删除，误删代价太大
            std::fs::remove_dir(&p)
                .map(|_| Value::Bool(true))
                .map_err(|e| format!("removedir 删不掉「{}」（只删空目录）：{}", p.display(), io_msg(&e)))
        }
        "removefile" => {
            need(args, 1, "removefile")?;
            let p = crate::host::resolve_path(&s_of(&args[0]));
            std::fs::remove_file(&p)
                .map(|_| Value::Bool(true))
                .map_err(|e| format!("removefile 删不掉「{}」：{}", p.display(), io_msg(&e)))
        }
        "renamefile" => {
            need(args, 2, "renamefile")?;
            let from = crate::host::resolve_path(&s_of(&args[0]));
            let to = crate::host::resolve_path(&s_of(&args[1]));
            std::fs::rename(&from, &to)
                .map(|_| Value::Bool(true))
                .map_err(|e| format!("renamefile 改名失败（{} → {}）：{}", from.display(), to.display(), io_msg(&e)))
        }
        "exists" => {
            need(args, 1, "exists")?;
            Ok(Value::Bool(crate::host::resolve_path(&s_of(&args[0])).exists()))
        }
        "isfile" => {
            need(args, 1, "isfile")?;
            Ok(Value::Bool(crate::host::resolve_path(&s_of(&args[0])).is_file()))
        }
        "isdir" => {
            need(args, 1, "isdir")?;
            Ok(Value::Bool(crate::host::resolve_path(&s_of(&args[0])).is_dir()))
        }
        "filesize" => {
            need(args, 1, "filesize")?;
            let p = crate::host::resolve_path(&s_of(&args[0]));
            let md = std::fs::metadata(&p)
                .map_err(|e| format!("filesize 读不到「{}」：{}", p.display(), io_msg(&e)))?;
            Ok(Value::Int(md.len() as i64))
        }
        "getenv" => {
            need(args, 1, "getenv")?;
            let k = s_of(&args[0]);
            match std::env::var(&k) {
                Ok(v) => Ok(Value::string(v)),
                Err(_) => Ok(Value::string(String::new())),
            }
        }
        "pathjoin" => {
            need(args, 1, "pathjoin")?;
            let mut p = std::path::PathBuf::new();
            for a in args {
                p.push(s_of(a));
            }
            Ok(Value::string(p.to_string_lossy().replace('\\', "/")))
        }
        // runcmd("命令")：跑一条系统命令，返回 {code, out}（out 是标准输出+错误输出）
        "runcmd" => {
            need(args, 1, "runcmd")?;
            let cmd = s_of(&args[0]);
            let dir = crate::host::base_dir_string();
            let mut c = if cfg!(windows) {
                let mut c = std::process::Command::new("cmd");
                c.arg("/C").arg(&cmd);
                c
            } else {
                let mut c = std::process::Command::new("sh");
                c.arg("-c").arg(&cmd);
                c
            };
            if !dir.is_empty() {
                c.current_dir(dir);
            }
            let out = c.output().map_err(|e| format!("runcmd 起不来：{}", e))?;
            let mut text = String::from_utf8_lossy(&out.stdout).to_string();
            text.push_str(&String::from_utf8_lossy(&out.stderr));
            let mut d = Dict::new();
            d.set(Rc::from("code"), Value::Int(out.status.code().unwrap_or(-1) as i64));
            d.set(Rc::from("out"), Value::string(text));
            Ok(Value::Dict(Rc::new(std::cell::RefCell::new(d))))
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

/// 最大公约数（欧几里得算法）
fn gcd_i64(mut a: i64, mut b: i64) -> i64 {
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a.abs()
}

/// 文件 IO 的中文错误提示（把常见的 io 错误翻译成人话）
fn io_msg(e: &std::io::Error) -> String {    match e.kind() {
        std::io::ErrorKind::NotFound => "文件不存在".into(),
        std::io::ErrorKind::PermissionDenied => "没有权限".into(),
        _ => e.to_string(),
    }
}

/// 当前时间的毫秒级 Unix 时间戳
#[cfg(not(target_arch = "wasm32"))]
fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 浏览器（wasm）里没有系统时钟：给 0，别崩。
/// 浏览器 demo 走 TS 引擎，那边用 Date.now() 是真值。
#[cfg(target_arch = "wasm32")]
fn now_ms() -> i64 {
    0
}

/// 伪随机数。用系统时间做种子，够脚本玩；不追求密码学质量。
/// 浏览器（wasm）里没有系统时钟，退化成固定种子——总不能为了 rand 崩掉。
fn pseudo_random() -> f64 {    use std::cell::Cell;
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
            "len" => {
                let m = p.borrow();
                Ok(Value::Int(match &m.target {
                    PtrTarget::Cells(c) => (c.borrow().len().saturating_sub(m.idx)) as i64,
                    _ => 1,
                }))
            }
            "free" => {
                let mut m = p.borrow_mut();
                if m.freed {
                    return Err("这块内存已经 free 过了，不用再 free 一次".into());
                }
                if !m.owned {
                    return Err("这不是 alloc 出来的内存（多半是 &列表[i] 或 &变量 拿到的地址），不能 free".into());
                }
                if let PtrTarget::Cells(c) = &m.target {
                    c.borrow_mut().clear();
                }
                m.freed = true;
                Ok(Value::Null)
            }
            other => Err(format!("指针没有方法「{}」（能用的是 len / free）", other)),
        },
        other => Err(format!("{} 没有方法「{}」", other.to_display(), name)),
    }
}

pub fn str_method(s: &Rc<String>, name: &str, args: &[Value]) -> R<Value> {
    match name {
        "upper" => Ok(Value::string(s.to_uppercase())),
        "lower" => Ok(Value::string(s.to_lowercase())),
        "trim" => Ok(Value::string(s.trim().to_string())),
        "len" => Ok(Value::Int(s.chars().count() as i64)),
        "contains" | "includes" => {
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
            crate::limits::guard_list(0, s.len().saturating_add(1))?;
            let parts: Vec<Value> = if sep.is_empty() {
                s.chars().map(|c| Value::string(c.to_string())).collect()
            } else {
                s.split(sep.as_str()).map(|p| Value::string(p.to_string())).collect()
            };
            Ok(Value::list(parts))
        }
        "substr" | "substring" => {
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
            crate::limits::guard_string(0, s.len().saturating_mul(n))?;
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

pub fn list_method(l: &crate::value::ListRef, name: &str, args: &[Value]) -> R<Value> {
    match name {
        // add 是 push 的别名（和 TS 兼容引擎一致）；返回新长度
        "push" | "add" => {
            need(args, 1, "push")?;
            let mut items = l.borrow_mut();
            crate::limits::guard_list(items.len(), 1)?;
            items.push(args[0].clone_deep());
            Ok(Value::Int(items.len() as i64))
        }
        // clear：清空，返回 0（和 TS 一致）
        "clear" => {
            let mut items = l.borrow_mut();
            items.clear();
            Ok(Value::Int(0))
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
            crate::limits::guard_list(items.len(), 1)?;
            items.insert(idx, args[1].clone_deep());
            Ok(Value::Int(items.len() as i64))
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
            // 默认分隔符是空串（和 TS 兼容引擎一致：xs.join() 就是直接拼起来）
            let sep = if args.is_empty() { String::new() } else { s_of(&args[0]) };
            let items = l.borrow();
            let parts: Vec<String> = items.iter().map(|x| x.to_display()).collect();
            let total: usize = parts.iter().map(|p| p.len()).sum::<usize>()
                .saturating_add(sep.len().saturating_mul(parts.len().saturating_sub(1)));
            crate::limits::guard_string(0, total)?;
            Ok(Value::string(parts.join(&sep)))
        }
        "reverse" => {
            l.borrow_mut().reverse();
            // 返回列表自己（和 TS 兼容引擎一致，方便链式写法）
            Ok(Value::List(Rc::clone(l)))
        }
        "sort" => {
            let mut items = l.borrow_mut();
            sort_values(&mut items);
            drop(items);
            // 返回列表自己（和 TS 兼容引擎一致）
            Ok(Value::List(Rc::clone(l)))
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

pub fn dict_method(d: &crate::value::DictRef, name: &str, args: &[Value]) -> R<Value> {
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
            let old = d.borrow().get(&k).unwrap_or(Value::Null);
            d.borrow_mut().remove(&k);
            // 返回被删掉的那个值（没有就是 null），和 TS 兼容引擎一致
            Ok(old)
        }
        // set(键, 值)：写一个键值对，返回字典自己（方便链式写法）
        "set" => {
            need(args, 2, "set")?;
            let k: Rc<str> = Rc::from(args[0].to_display().as_str());
            crate::limits::guard_dict(d.borrow().len(), 1)?;
            d.borrow_mut().set(k, args[1].clone_deep());
            Ok(Value::Dict(Rc::clone(d)))
        }
        // items()：[[键, 值], …]，遍历字典时最顺手的形状
        "items" => {
            let rows: Vec<Value> = d
                .borrow()
                .entries()
                .into_iter()
                .map(|(k, v)| {
                    let pair = vec![Value::string(k.to_string()), v.clone()];
                    Value::list(pair)
                })
                .collect();
            Ok(Value::list(rows))
        }
        "len" => Ok(Value::Int(d.borrow().len() as i64)),
        other => Err(format!("字典没有方法「{}」", other)),
    }
}

/// 让空字典也能被构造（供 VM 的 BuildDict 之外的地方用）
pub fn empty_dict() -> Value {
    Value::Dict(Rc::new(std::cell::RefCell::new(Dict::new())))
}

/// Copper 的统一排序规则：数字比数字（int/float 混排都行），
/// 其余一律按字符串字典序。列表方法 xs.sort() 和全局 sorted() 都走这里，
/// 两边永远不会跑偏。
pub fn sort_values(items: &mut [Value]) {
    items.sort_by(|a, b| {
        match (a, b) {
            (Value::Int(_) | Value::Float(_), Value::Int(_) | Value::Float(_)) => {
                a.as_num().partial_cmp(&b.as_num()).unwrap_or(std::cmp::Ordering::Equal)
            }
            _ => {
                let (x, y) = (a.to_display(), b.to_display());
                x.cmp(&y)
            }
        }
    });
}
