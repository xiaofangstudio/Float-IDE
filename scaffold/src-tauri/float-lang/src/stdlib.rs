//! 标准库（v1）：`import os` / `data` / `json` / `time` / `random` / `ui` / `re` / `net`
//!
//! 设计：
//! * 对外名字是 `os.readfile(...)` 这种带点的形式；解析器把它收敛成一次内建调用
//!   `call_builtin("os.readfile", …)`，所以 VM / AOT / TS 三处走的是同一条路。
//! * 迁移类函数（readfile / listdir / now …）**直接转发给老内建**，
//!   保证新旧两条名字行为逐字节一致，不用维护两份逻辑。
//! * 错误文案统一中文，格式：`库.函数 需要…/不能…`，和现有内建一致。

use crate::value::{Dict, Value};
use std::cell::RefCell;
use std::rc::Rc;

type R<T> = Result<T, String>;

/* ───────────────────────── 通用小工具 ───────────────────────── */

fn need(args: &[Value], n: usize, name: &str) -> R<()> {
    if args.len() < n {
        Err(format!("{} 需要 {} 个参数，实际给了 {}", name, n, args.len()))
    } else {
        Ok(())
    }
}

fn num(v: &Value, name: &str) -> R<f64> {
    v.as_num().ok_or_else(|| format!("{} 需要数字，得到 {}", name, v.to_display()))
}

fn int_of(v: &Value, name: &str) -> R<i64> {
    v.as_int().ok_or_else(|| format!("{} 需要整数，得到 {}", name, v.to_display()))
}

fn s_of(v: &Value) -> String {
    v.to_display()
}

fn list_of(v: &Value, name: &str) -> R<Vec<Value>> {
    match v {
        Value::List(l) => Ok(l.borrow().clone()),
        other => Err(format!("{} 需要列表，得到 {}", name, other.to_display())),
    }
}

fn dict_of<'a>(v: &'a Value, name: &str) -> R<std::cell::Ref<'a, Dict>> {
    match v {
        Value::Dict(d) => Ok(d.borrow()),
        other => Err(format!("{} 需要字典，得到 {}", name, other.to_display())),
    }
}

fn mk_list(items: Vec<Value>) -> Value {
    Value::List(Rc::new(RefCell::new(items)))
}

fn mk_dict(d: Dict) -> Value {
    Value::Dict(Rc::new(RefCell::new(d)))
}

/* ───────────────────────── 命名空间表 ───────────────────────── */

/// 已知的标准库命名空间（解析器用它判断 `os.readfile` 是库调用还是成员访问）
pub const NAMESPACES: &[(&str, &[&str])] = &[
    (
        "os",
        &[
            "readfile", "writefile", "appendfile", "hasfile", "exists", "isfile", "isdir",
            "filesize", "listdir", "makedir", "removedir", "removefile", "renamefile", "cwd",
            "chdir", "getenv", "pathjoin", "runcmd", "basename", "dirname", "extname", "abspath",
            "tempdir", "homedir", "copyfile", "mtime", "setenv", "args", "pid", "exit", "platform",
            "sleep",
        ],
    ),
    (
        "data",
        &[
            "sort", "sort_by", "max", "min", "sum", "avg", "median", "stdev", "count", "freq",
            "unique", "flatten", "chunk", "zip", "enumerate", "reverse", "slice", "is_empty",
            "keys", "values", "entries", "merge", "pick", "omit", "get", "tally",
        ],
    ),
    ("json", &["parse", "stringify", "pretty", "valid", "try_parse"]),
    (
        "time",
        &[
            "now", "timestamp", "millis", "sleep", "mark", "since", "format", "parse", "parts",
            "add", "diff", "iso",
        ],
    ),
    (
        "random",
        &["rand", "float", "choice", "shuffle", "sample", "seed", "gauss", "weighted"],
    ),
    (
        "ui",
        &["clear", "clear_line", "color", "progress", "menu", "input", "table", "bar", "title", "spinner"],
    ),
    ("re", &["findall", "match", "search", "test", "replace", "split", "escape"]),
    (
        "net",
        &["fetch", "post", "request", "download", "encode", "decode", "query", "listen", "server_close", "ip"],
    ),
];

/// 这个命名空间存不存在
pub fn is_namespace(ns: &str) -> bool {
    NAMESPACES.iter().any(|(n, _)| *n == ns)
}

/// 把 `os.readfile` 变成一个 `&'static str`（Value::Native 要的就是它）。
/// 首次用到时泄漏一份，数量有限（= 函数表大小），不会无限涨。
pub fn static_name(n: &str) -> Option<&'static str> {
    if !is_fn(n) {
        return None;
    }
    thread_local! {
        static CACHE: std::cell::RefCell<std::collections::HashMap<String, &'static str>> =
            std::cell::RefCell::new(std::collections::HashMap::new());
    }
    CACHE.with(|c| {
        let mut m = c.borrow_mut();
        if let Some(x) = m.get(n) {
            return Some(*x);
        }
        let leaked: &'static str = Box::leak(n.to_string().into_boxed_str());
        m.insert(n.to_string(), leaked);
        Some(leaked)
    })
}

/// `ns.函数` 是不是标准库函数（解析器改写用）
pub fn is_fn(full: &str) -> bool {
    let Some((ns, f)) = full.split_once('.') else {
        return false;
    };
    NAMESPACES.iter().any(|(n, fs)| *n == ns && fs.contains(&f))
}

/* ───────────────────────── 入口 ───────────────────────── */

/// `name` 形如 `os.readfile`。返回 None 表示不认识的库函数。
pub fn call(name: &str, args: &[Value]) -> Option<R<Value>> {
    let (ns, f) = name.split_once('.')?;
    let r = match (ns, f) {
        ("os", f) => os(f, args),
        ("data", f) => data(f, args),
        ("json", f) => json(f, args),
        ("time", f) => time(f, args),
        ("random", f) => random(f, args),
        ("ui", f) => ui(f, args),
        ("re", f) => re(f, args),
        ("net", f) => net(f, args),
        _ => return None,
    };
    Some(r)
}

/* ═════════════════════════ 1. os ═════════════════════════ */

fn os(f: &str, args: &[Value]) -> R<Value> {
    // 老内建已经有的一律转发，保证新旧名字行为逐字节一致
    let old = match f {
        "readfile" => Some("readfile"),
        "writefile" => Some("writefile"),
        "appendfile" => Some("appendfile"),
        "hasfile" => Some("hasfile"),
        "exists" => Some("exists"),
        "isfile" => Some("isfile"),
        "isdir" => Some("isdir"),
        "filesize" => Some("filesize"),
        "listdir" => Some("listdir"),
        "makedir" => Some("makedir"),
        "removedir" => Some("removedir"),
        "removefile" => Some("removefile"),
        "renamefile" => Some("renamefile"),
        "cwd" => Some("cwd"),
        "chdir" => Some("chdir"),
        "getenv" => Some("getenv"),
        "pathjoin" => Some("pathjoin"),
        "runcmd" => Some("runcmd"),
        _ => None,
    };
    if let Some(old) = old {
        return crate::builtins::call_builtin(old, args);
    }

    match f {
        "basename" => {
            need(args, 1, "os.basename")?;
            let p = s_of(&args[0]).replace('\\', "/");
            let t = p.trim_end_matches('/');
            let name = t.rsplit('/').next().unwrap_or(t);
            Ok(Value::string(name.to_string()))
        }
        "dirname" => {
            need(args, 1, "os.dirname")?;
            let p = s_of(&args[0]).replace('\\', "/");
            let t = p.trim_end_matches('/');
            match t.rfind('/') {
                Some(i) => Ok(Value::string(t[..i].to_string())),
                None => Ok(Value::string(String::new())),
            }
        }
        "extname" => {
            need(args, 1, "os.extname")?;
            let p = s_of(&args[0]).replace('\\', "/");
            let name = p.rsplit('/').next().unwrap_or(&p);
            match name.rfind('.') {
                Some(i) if i > 0 => Ok(Value::string(name[i..].to_string())),
                _ => Ok(Value::string(String::new())),
            }
        }
        "abspath" => {
            need(args, 1, "os.abspath")?;
            let p = s_of(&args[0]);
            if p.starts_with('/') || (p.len() > 1 && p.as_bytes()[1] == b':') {
                return Ok(Value::string(p));
            }
            let base = crate::host::base_dir_string();
            let joined = format!("{}/{}", base, p);
            Ok(Value::string(normalize_path(&joined)))
        }
        "tempdir" => Ok(Value::string(std::env::temp_dir().to_string_lossy().replace('\\', "/"))),
        "homedir" => {
            let h = std::env::var("USERPROFILE")
                .or_else(|_| std::env::var("HOME"))
                .unwrap_or_default();
            Ok(Value::string(h.replace('\\', "/")))
        }
        "copyfile" => {
            need(args, 2, "os.copyfile")?;
            let from = crate::host::resolve_path(&s_of(&args[0]));
            let to = crate::host::resolve_path(&s_of(&args[1]));
            std::fs::copy(&from, &to)
                .map(|_| Value::Bool(true))
                .map_err(|e| format!("os.copyfile 复制失败：{}", e))
        }
        "mtime" => {
            need(args, 1, "os.mtime")?;
            let p = crate::host::resolve_path(&s_of(&args[0]));
            let md = std::fs::metadata(&p).map_err(|e| format!("os.mtime 读不到 {}：{}", s_of(&args[0]), e))?;
            let ms = md
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            Ok(Value::Int(ms))
        }
        "setenv" => {
            need(args, 2, "os.setenv")?;
            let k = s_of(&args[0]);
            if k.is_empty() {
                return Err("os.setenv 的变量名不能为空".into());
            }
            std::env::set_var(k, s_of(&args[1]));
            Ok(Value::Bool(true))
        }
        "args" => {
            let items: Vec<Value> = std::env::args().skip(1).map(Value::string).collect();
            Ok(mk_list(items))
        }
        "pid" => Ok(Value::Int(std::process::id() as i64)),
        "exit" => {
            let code = args.first().map(|a| int_of(a, "os.exit")).transpose()?.unwrap_or(0);
            use std::io::Write as _;
            let _ = std::io::stdout().flush();
            std::process::exit(code as i32);
        }
        "platform" => Ok(Value::str(if cfg!(windows) {
            "windows"
        } else if cfg!(target_os = "macos") {
            "macos"
        } else {
            "linux"
        })),
        "sleep" => {
            need(args, 1, "os.sleep")?;
            let s = num(&args[0], "os.sleep")?;
            if s < 0.0 {
                return Err("os.sleep 的秒数不能是负数".into());
            }
            std::thread::sleep(std::time::Duration::from_micros((s * 1e6) as u64));
            Ok(Value::Null)
        }
        other => Err(format!("os 里没有这个函数：os.{}", other)),
    }
}

fn normalize_path(p: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            s => out.push(s),
        }
    }
    let prefix = if p.starts_with('/') { "/" } else { "" };
    format!("{}{}", prefix, out.join("/"))
}

/* ═════════════════════════ 2. data ═════════════════════════ */

fn data(f: &str, args: &[Value]) -> R<Value> {
    match f {
        "sort" => {
            need(args, 1, "data.sort")?;
            let mut items = list_of(&args[0], "data.sort")?;
            crate::builtins::sort_values(&mut items);
            if let Some(Value::Str(d)) = args.get(1) {
                if &**d == "desc" {
                    items.reverse();
                }
            }
            Ok(mk_list(items))
        }
        "sort_by" => {
            need(args, 2, "data.sort_by")?;
            let mut items = list_of(&args[0], "data.sort_by")?;
            let key = match &args[1] {
                Value::Str(s) => s.to_string(),
                other => {
                    return Err(format!(
                        "data.sort_by 的第二参要写字段名（字符串），得到 {}；按函数排序 v1 还没做",
                        other.to_display()
                    ))
                }
            };
            let mut bad: Option<String> = None;
            items.sort_by(|a, b| {
                let ka = pick_field(a, &key);
                let kb = pick_field(b, &key);
                match (ka, kb) {
                    (Some(x), Some(y)) => cmp_values(&x, &y),
                    (None, _) | (_, None) => {
                        if bad.is_none() {
                            bad = Some(format!("data.sort_by：有一项没有字段「{}」", key));
                        }
                        std::cmp::Ordering::Equal
                    }
                }
            });
            if let Some(m) = bad {
                return Err(m);
            }
            if let Some(Value::Str(d)) = args.get(2) {
                if &**d == "desc" {
                    items.reverse();
                }
            }
            Ok(mk_list(items))
        }
        "max" | "min" => {
            need(args, 1, &format!("data.{}", f))?;
            let items = list_of(&args[0], &format!("data.{}", f))?;
            if items.is_empty() {
                return Err(format!("data.{}：空列表"            , f));
            }
            let mut best = items[0].clone();
            for it in items.iter().skip(1) {
                let ord = cmp_values(it, &best);
                let take = if f == "max" { ord == std::cmp::Ordering::Greater } else { ord == std::cmp::Ordering::Less };
                if take {
                    best = it.clone();
                }
            }
            Ok(best)
        }
        "sum" => {
            need(args, 1, "data.sum")?;
            let items = list_of(&args[0], "data.sum")?;
            let mut int_sum: i64 = 0;
            let mut float_sum: f64 = 0.0;
            let mut is_float = false;
            for (i, it) in items.iter().enumerate() {
                match it {
                    Value::Int(n) => {
                        if is_float {
                            float_sum += *n as f64;
                        } else {
                            int_sum = int_sum.saturating_add(*n);
                        }
                    }
                    Value::Float(x) => {
                        if !is_float {
                            float_sum = int_sum as f64;
                            is_float = true;
                        }
                        float_sum += *x;
                    }
                    other => {
                        return Err(format!(
                            "data.sum 的第 {} 项不是数字：{}",
                            i + 1,
                            other.to_display()
                        ))
                    }
                }
            }
            Ok(if is_float { Value::Float(float_sum) } else { Value::Int(int_sum) })
        }
        "avg" => {
            need(args, 1, "data.avg")?;
            let items = list_of(&args[0], "data.avg")?;
            let key = args.get(1).map(|v| s_of(v));
            if items.is_empty() {
                return Err("data.avg：空列表没有平均值".into());
            }
            let mut total = 0.0;
            for (i, it) in items.iter().enumerate() {
                let v = match &key {
                    Some(k) => pick_field(it, k).ok_or_else(|| format!("data.avg：第 {} 项没有字段「{}」", i + 1, k))?,
                    None => it.clone(),
                };
                total += num(&v, "data.avg")?;
            }
            Ok(Value::Float(total / items.len() as f64))
        }
        "median" => {
            need(args, 1, "data.median")?;
            let items = list_of(&args[0], "data.median")?;
            if items.is_empty() {
                return Err("data.median：空列表没有中位数".into());
            }
            let mut nums: Vec<f64> = Vec::with_capacity(items.len());
            for (i, it) in items.iter().enumerate() {
                nums.push(num(it, &format!("data.median 的第 {} 项", i + 1))?);
            }
            nums.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let n = nums.len();
            let med = if n % 2 == 1 {
                nums[n / 2]
            } else {
                (nums[n / 2 - 1] + nums[n / 2]) / 2.0
            };
            Ok(Value::Float(med))
        }
        "stdev" => {
            need(args, 1, "data.stdev")?;
            let items = list_of(&args[0], "data.stdev")?;
            if items.len() < 2 {
                return Err("data.stdev：至少要 2 个数".into());
            }
            let mut nums: Vec<f64> = Vec::with_capacity(items.len());
            for it in &items {
                nums.push(num(it, "data.stdev")?);
            }
            let mean = nums.iter().sum::<f64>() / nums.len() as f64;
            let var = nums.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / (nums.len() - 1) as f64;
            Ok(Value::Float(var.sqrt()))
        }
        "count" => {
            need(args, 2, "data.count")?;
            let items = list_of(&args[0], "data.count")?;
            let target = &args[1];
            let n = items.iter().filter(|x| cmp_values(x, target) == std::cmp::Ordering::Equal).count();
            Ok(Value::Int(n as i64))
        }
        "freq" | "tally" => {
            need(args, 1, "data.freq")?;
            let items = list_of(&args[0], "data.freq")?;
            let mut d = Dict::new();
            for it in &items {
                let k: Rc<str> = Rc::from(it.to_display().as_str());
                let cur = d.get(&k).and_then(|v| v.as_int()).unwrap_or(0);
                d.set(k, Value::Int(cur + 1));
            }
            Ok(mk_dict(d))
        }
        "unique" => {
            need(args, 1, "data.unique")?;
            let items = list_of(&args[0], "data.unique")?;
            let mut out: Vec<Value> = Vec::new();
            for it in items {
                if !out.iter().any(|x| cmp_values(x, &it) == std::cmp::Ordering::Equal) {
                    out.push(it);
                }
            }
            Ok(mk_list(out))
        }
        "flatten" => {
            need(args, 1, "data.flatten")?;
            let items = list_of(&args[0], "data.flatten")?;
            let depth = match args.get(1) {
                Some(v) => int_of(v, "data.flatten")?,
                None => 1,
            };
            let mut out: Vec<Value> = Vec::new();
            flatten_into(&items, depth, &mut out);
            Ok(mk_list(out))
        }
        "chunk" => {
            need(args, 2, "data.chunk")?;
            let items = list_of(&args[0], "data.chunk")?;
            let n = int_of(&args[1], "data.chunk")?;
            if n <= 0 {
                return Err("data.chunk 的每块长度要大于 0".into());
            }
            let n = n as usize;
            let out: Vec<Value> = items.chunks(n).map(|c| mk_list(c.to_vec())).collect();
            Ok(mk_list(out))
        }
        "zip" => {
            need(args, 2, "data.zip")?;
            let a = list_of(&args[0], "data.zip")?;
            let b = list_of(&args[1], "data.zip")?;
            let n = a.len().min(b.len());
            let out: Vec<Value> = (0..n).map(|i| mk_list(vec![a[i].clone(), b[i].clone()])).collect();
            Ok(mk_list(out))
        }
        "enumerate" => {
            need(args, 1, "data.enumerate")?;
            let items = list_of(&args[0], "data.enumerate")?;
            let start = match args.get(1) {
                Some(v) => int_of(v, "data.enumerate")?,
                None => 0,
            };
            let out: Vec<Value> = items
                .iter()
                .enumerate()
                .map(|(i, v)| mk_list(vec![Value::Int(start + i as i64), v.clone()]))
                .collect();
            Ok(mk_list(out))
        }
        "reverse" => {
            need(args, 1, "data.reverse")?;
            let mut items = list_of(&args[0], "data.reverse")?;
            items.reverse();
            Ok(mk_list(items))
        }
        "slice" => {
            need(args, 2, "data.slice")?;
            let items = list_of(&args[0], "data.slice")?;
            let n = items.len() as i64;
            let mut from = int_of(&args[1], "data.slice")?;
            let mut to = match args.get(2) {
                Some(v) => int_of(v, "data.slice")?,
                None => n,
            };
            if from < 0 {
                from += n;
            }
            if to < 0 {
                to += n;
            }
            let from = from.clamp(0, n) as usize;
            let to = to.clamp(0, n) as usize;
            let out: Vec<Value> = if from >= to { Vec::new() } else { items[from..to].to_vec() };
            Ok(mk_list(out))
        }
        "is_empty" => {
            need(args, 1, "data.is_empty")?;
            Ok(Value::Bool(match &args[0] {
                Value::List(l) => l.borrow().is_empty(),
                Value::Dict(d) => d.borrow().len() == 0,
                Value::Str(s) => s.is_empty(),
                other => return Err(format!("data.is_empty 需要列表/字典/字符串，得到 {}", other.to_display())),
            }))
        }
        "keys" => {
            need(args, 1, "data.keys")?;
            let d = dict_of(&args[0], "data.keys")?;
            let out: Vec<Value> = d.keys().iter().map(|k| Value::string(k.to_string())).collect();
            Ok(mk_list(out))
        }
        "values" => {
            need(args, 1, "data.values")?;
            let d = dict_of(&args[0], "data.values")?;
            let out: Vec<Value> = d.entries().map(|(_, v)| v.clone()).collect();
            Ok(mk_list(out))
        }
        "entries" => {
            need(args, 1, "data.entries")?;
            let d = dict_of(&args[0], "data.entries")?;
            let out: Vec<Value> = d
                .entries()
                .map(|(k, v)| mk_list(vec![Value::string(k.to_string()), v.clone()]))
                .collect();
            Ok(mk_list(out))
        }
        "merge" => {
            need(args, 2, "data.merge")?;
            let a = dict_of(&args[0], "data.merge")?;
            let b = dict_of(&args[1], "data.merge")?;
            let mut out = Dict::new();
            for (k, v) in a.entries() {
                out.set(Rc::clone(k), v.clone());
            }
            for (k, v) in b.entries() {
                out.set(Rc::clone(k), v.clone());
            }
            Ok(mk_dict(out))
        }
        "pick" | "omit" => {
            need(args, 2, "data.pick")?;
            let d = dict_of(&args[0], "data.pick")?;
            let keys = list_of(&args[1], "data.pick")?;
            let mut out = Dict::new();
            let keep = f == "pick";
            for (k, v) in d.entries() {
                let listed = keys.iter().any(|x| s_of(x) == **k);
                if listed == keep {
                    out.set(Rc::clone(k), v.clone());
                }
            }
            Ok(mk_dict(out))
        }
        "get" => {
            need(args, 2, "data.get")?;
            let d = dict_of(&args[0], "data.get")?;
            let k = s_of(&args[1]);
            Ok(d.get(&k).unwrap_or_else(|| args.get(2).cloned().unwrap_or(Value::Null)))
        }
        other => Err(format!("data 里没有这个函数：data.{}", other)),
    }
}

fn pick_field(v: &Value, key: &str) -> Option<Value> {
    match v {
        Value::Dict(d) => d.borrow().get(key),
        _ => None,
    }
}

fn flatten_into(items: &[Value], depth: i64, out: &mut Vec<Value>) {
    for it in items {
        match it {
            Value::List(l) if depth != 0 => {
                let inner = l.borrow().clone();
                flatten_into(&inner, depth - 1, out);
            }
            other => out.push(other.clone()),
        }
    }
}

/// 值比较：数字比数字，其余按字符串（和 data.sort 的规则一致）
fn cmp_values(a: &Value, b: &Value) -> std::cmp::Ordering {
    match (a, b) {
        (Value::Int(_) | Value::Float(_), Value::Int(_) | Value::Float(_)) => a
            .as_num()
            .partial_cmp(&b.as_num())
            .unwrap_or(std::cmp::Ordering::Equal),
        _ => a.to_display().cmp(&b.to_display()),
    }
}

/* ═════════════════════════ 3. json ═════════════════════════ */

fn json(f: &str, args: &[Value]) -> R<Value> {
    match f {
        "parse" => {
            need(args, 1, "json.parse")?;
            let text = s_of(&args[0]);
            parse_json(&text).map_err(|m| format!("json.parse 出错：{}", m))
        }
        "try_parse" => {
            need(args, 1, "json.try_parse")?;
            let text = s_of(&args[0]);
            let mut d = Dict::new();
            match parse_json(&text) {
                Ok(v) => {
                    d.set(Rc::from("ok"), Value::Bool(true));
                    d.set(Rc::from("value"), v);
                }
                Err(m) => {
                    d.set(Rc::from("ok"), Value::Bool(false));
                    d.set(Rc::from("error"), Value::string(m));
                }
            }
            Ok(mk_dict(d))
        }
        "valid" => {
            need(args, 1, "json.valid")?;
            Ok(Value::Bool(parse_json(&s_of(&args[0])).is_ok()))
        }
        "stringify" | "pretty" => {
            need(args, 1, "json.stringify")?;
            let indent = if f == "pretty" {
                2
            } else {
                match args.get(1) {
                    Some(v) => int_of(v, "json.stringify")?.max(0) as usize,
                    None => 0,
                }
            };
            let mut out = String::new();
            write_json(&args[0], indent, 0, &mut out);
            Ok(Value::string(out))
        }
        other => Err(format!("json 里没有这个函数：json.{}", other)),
    }
}

/// 极简 JSON 解析：对象 → 字典，数组 → 列表，数字按有无小数点分 int/float
fn parse_json(s: &str) -> Result<Value, String> {
    let mut p = JParser { b: s.as_bytes(), i: 0, depth: 0 };
    let v = p.value()?;
    p.ws();
    if p.i != p.b.len() {
        return Err(format!("第 {} 个字符后面还有内容", p.i + 1));
    }
    Ok(v)
}

struct JParser<'a> {
    b: &'a [u8],
    i: usize,
    depth: usize,
}

impl<'a> JParser<'a> {
    fn ws(&mut self) {
        while matches!(self.peek(), Some(b' ') | Some(b'\n') | Some(b'\r') | Some(b'\t')) {
            self.i += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }

    fn value(&mut self) -> Result<Value, String> {
        self.ws();
        self.depth += 1;
        if self.depth > 200 {
            return Err("嵌套太深（超过 200 层）".into());
        }
        let r = match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => Ok(Value::string(self.string()?)),
            Some(b't') => {
                self.lit("true")?;
                Ok(Value::Bool(true))
            }
            Some(b'f') => {
                self.lit("false")?;
                Ok(Value::Bool(false))
            }
            Some(b'n') => {
                self.lit("null")?;
                Ok(Value::Null)
            }
            Some(_) => self.number(),
            None => Err("内容不完整".into()),
        };
        self.depth -= 1;
        r
    }

    fn lit(&mut self, word: &str) -> Result<(), String> {
        if self.b[self.i..].starts_with(word.as_bytes()) {
            self.i += word.len();
            Ok(())
        } else {
            Err(format!("第 {} 个字符处应该是 {}", self.i + 1, word))
        }
    }

    fn object(&mut self) -> Result<Value, String> {
        self.i += 1; // {
        let mut d = Dict::new();
        self.ws();
        if self.peek() == Some(b'}') {
            self.i += 1;
            return Ok(mk_dict(d));
        }
        loop {
            self.ws();
            if self.peek() != Some(b'"') {
                return Err(format!("第 {} 个字符处应该是键名（带引号）", self.i + 1));
            }
            let k = self.string()?;
            self.ws();
            if self.peek() != Some(b':') {
                return Err(format!("第 {} 个字符处应该是冒号", self.i + 1));
            }
            self.i += 1;
            let v = self.value()?;
            d.set(Rc::from(k.as_str()), v);
            self.ws();
            match self.peek() {
                Some(b',') => {
                    self.i += 1;
                }
                Some(b'}') => {
                    self.i += 1;
                    break;
                }
                _ => return Err(format!("第 {} 个字符处应该是逗号或 }}", self.i + 1)),
            }
        }
        Ok(mk_dict(d))
    }

    fn array(&mut self) -> Result<Value, String> {
        self.i += 1; // [
        let mut items: Vec<Value> = Vec::new();
        self.ws();
        if self.peek() == Some(b']') {
            self.i += 1;
            return Ok(mk_list(items));
        }
        loop {
            items.push(self.value()?);
            self.ws();
            match self.peek() {
                Some(b',') => {
                    self.i += 1;
                }
                Some(b']') => {
                    self.i += 1;
                    break;
                }
                _ => return Err(format!("第 {} 个字符处应该是逗号或 ]", self.i + 1)),
            }
        }
        Ok(mk_list(items))
    }

    fn string(&mut self) -> Result<String, String> {
        self.i += 1; // "
        let mut out = String::new();
        loop {
            let c = self.peek().ok_or_else(|| "字符串没有结束引号".to_string())?;
            self.i += 1;
            match c {
                b'"' => break,
                b'\\' => {
                    let e = self.peek().ok_or_else(|| "转义不完整".to_string())?;
                    self.i += 1;
                    match e {
                        b'n' => out.push('\n'),
                        b't' => out.push('\t'),
                        b'r' => out.push('\r'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'/' => out.push('/'),
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'u' => {
                            let hex = self.b.get(self.i..self.i + 4).ok_or_else(|| "\\u 后面要 4 位十六进制".to_string())?;
                            let h = std::str::from_utf8(hex).map_err(|e| e.to_string())?;
                            let cp = u32::from_str_radix(h, 16).map_err(|_| "\\u 后面要 4 位十六进制".to_string())?;
                            if let Some(ch) = char::from_u32(cp) {
                                out.push(ch);
                            }
                            self.i += 4;
                        }
                        other => return Err(format!("不认识的转义 \\{}", other as char)),
                    }
                }
                _ => {
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

    fn number(&mut self) -> Result<Value, String> {
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
        if self.i == start {
            return Err(format!("第 {} 个字符处应该是数字", start + 1));
        }
        let text = std::str::from_utf8(&self.b[start..self.i]).map_err(|e| e.to_string())?;
        if is_float {
            text.parse::<f64>()
                .map(Value::Float)
                .map_err(|_| format!("「{}」不是合法数字", text))
        } else {
            text.parse::<i64>()
                .map(Value::Int)
                .map_err(|_| format!("「{}」不是合法整数", text))
        }
    }
}

fn write_json(v: &Value, indent: usize, level: usize, out: &mut String) {
    let pad = |n: usize| " ".repeat(n * indent);
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Int(n) => out.push_str(&n.to_string()),
        Value::Float(f) => {
            if f.fract() == 0.0 && f.is_finite() {
                out.push_str(&format!("{:.1}", f));
            } else {
                out.push_str(&format!("{}", f));
            }
        }
        Value::Str(s) => out.push_str(&json_escape(s)),
        Value::List(l) => {
            let items = l.borrow();
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push('[');
            for (i, it) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                if indent > 0 {
                    out.push('\n');
                    out.push_str(&pad(level + 1));
                }
                write_json(it, indent, level + 1, out);
            }
            if indent > 0 {
                out.push('\n');
                out.push_str(&pad(level));
            }
            out.push(']');
        }
        Value::Dict(d) => {
            let d = d.borrow();
            if d.len() == 0 {
                out.push_str("{}");
                return;
            }
            out.push('{');
            for (i, (k, val)) in d.entries().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                if indent > 0 {
                    out.push('\n');
                    out.push_str(&pad(level + 1));
                }
                out.push_str(&json_escape(k));
                out.push(':');
                if indent > 0 {
                    out.push(' ');
                }
                write_json(val, indent, level + 1, out);
            }
            if indent > 0 {
                out.push('\n');
                out.push_str(&pad(level));
            }
            out.push('}');
        }
        other => out.push_str(&json_escape(&other.to_display())),
    }
}

fn json_escape(s: &str) -> String {
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

/* ═════════════════════════ 4. time ═════════════════════════ */

/// 本地时区偏移（秒）：只算一次，够用（夏令时不追）
fn tz_offset() -> i64 {
    thread_local! {
        static OFF: std::cell::Cell<i64> = std::cell::Cell::new(i64::MIN);
    }
    OFF.with(|c| {
        if c.get() != i64::MIN {
            return c.get();
        }
        let off = local_offset_secs();
        c.set(off);
        off
    })
}

#[cfg(windows)]
fn local_offset_secs() -> i64 {
    // GetLocalTime 与 UTC 的差值
    #[repr(C)]
    struct SysTime {
        year: u16,
        month: u16,
        dow: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        millis: u16,
    }
    extern "system" {
        fn GetLocalTime(t: *mut SysTime);
        fn GetSystemTime(t: *mut SysTime);
    }
    let mut l = SysTime { year: 0, month: 0, dow: 0, day: 0, hour: 0, minute: 0, second: 0, millis: 0 };
    let mut u = SysTime { year: 0, month: 0, dow: 0, day: 0, hour: 0, minute: 0, second: 0, millis: 0 };
    unsafe {
        GetLocalTime(&mut l);
        GetSystemTime(&mut u);
    }
    let days_l = days_from_civil(l.year as i64, l.month as i64, l.day as i64);
    let days_u = days_from_civil(u.year as i64, u.month as i64, u.day as i64);
    let secs_l = days_l * 86400 + l.hour as i64 * 3600 + l.minute as i64 * 60 + l.second as i64;
    let secs_u = days_u * 86400 + u.hour as i64 * 3600 + u.minute as i64 * 60 + u.second as i64;
    secs_l - secs_u
}

#[cfg(not(windows))]
fn local_offset_secs() -> i64 {
    0
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

struct Parts {
    year: i64,
    month: i64,
    day: i64,
    hour: i64,
    minute: i64,
    second: i64,
    weekday: i64,
    yearday: i64,
}

fn parts_of(ts: f64) -> Parts {
    let local = ts + tz_offset() as f64;
    let secs = local.floor() as i64;
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    let (year, month, day) = civil_from_days(days);
    // 1970-01-01 是周四
    let weekday = (days + 4).rem_euclid(7) + 1;
    let yearday = days - days_from_civil(year, 1, 1) + 1;
    Parts {
        year,
        month,
        day,
        hour: rem / 3600,
        minute: rem % 3600 / 60,
        second: rem % 60,
        weekday,
        yearday,
    }
}

fn format_ts(ts: f64, fmt: &str) -> String {
    let p = parts_of(ts);
    let mut out = String::new();
    let mut it = fmt.chars().peekable();
    while let Some(c) = it.next() {
        if c != 'Y' && c != 'M' && c != 'D' && c != 'H' && c != 'm' && c != 's' {
            out.push(c);
            continue;
        }
        let mut n = 1;
        while it.peek() == Some(&c) {
            it.next();
            n += 1;
        }
        let v = match c {
            'Y' => p.year,
            'M' => p.month,
            'D' => p.day,
            'H' => p.hour,
            'm' => p.minute,
            _ => p.second,
        };
        if n >= 2 {
            out.push_str(&format!("{:02}", v));
        } else {
            out.push_str(&v.to_string());
        }
    }
    out
}

fn time(f: &str, args: &[Value]) -> R<Value> {
    let now_secs = || -> f64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0)
    };
    match f {
        "now" => {
            let ts = now_secs();
            Ok(Value::string(format_ts(ts, "YYYY-MM-DD HH:mm:ss")))
        }
        "timestamp" => Ok(Value::Float(now_secs())),
        "millis" => {
            let ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            Ok(Value::Int(ms))
        }
        "sleep" => {
            need(args, 1, "time.sleep")?;
            let s = num(&args[0], "time.sleep")?;
            if s < 0.0 {
                return Err("time.sleep 的秒数不能是负数".into());
            }
            std::thread::sleep(std::time::Duration::from_micros((s * 1e6) as u64));
            Ok(Value::Null)
        }
        "mark" => {
            let ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            Ok(Value::Int(ms))
        }
        "since" => {
            need(args, 1, "time.since")?;
            let start = int_of(&args[0], "time.since")?;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            Ok(Value::Float((now - start) as f64 / 1000.0))
        }
        "format" => {
            need(args, 1, "time.format")?;
            let ts = num(&args[0], "time.format")?;
            let fmt = match args.get(1) {
                Some(v) => s_of(v),
                None => "YYYY-MM-DD HH:mm:ss".to_string(),
            };
            Ok(Value::string(format_ts(ts, &fmt)))
        }
        "parse" => {
            need(args, 1, "time.parse")?;
            let text = s_of(&args[0]);
            parse_time_text(&text).map(Value::Float).ok_or_else(|| {
                format!("time.parse 认不出这个时间：{}（要写成 2026-11-03 20:44:01）", text)
            })
        }
        "parts" => {
            let ts = match args.first() {
                Some(v) => num(v, "time.parts")?,
                None => now_secs(),
            };
            let p = parts_of(ts);
            let mut d = Dict::new();
            d.set(Rc::from("year"), Value::Int(p.year));
            d.set(Rc::from("month"), Value::Int(p.month));
            d.set(Rc::from("day"), Value::Int(p.day));
            d.set(Rc::from("hour"), Value::Int(p.hour));
            d.set(Rc::from("minute"), Value::Int(p.minute));
            d.set(Rc::from("second"), Value::Int(p.second));
            d.set(Rc::from("weekday"), Value::Int(p.weekday));
            d.set(Rc::from("yearday"), Value::Int(p.yearday));
            Ok(mk_dict(d))
        }
        "add" => {
            need(args, 2, "time.add")?;
            let ts = num(&args[0], "time.add")?;
            let d = num(&args[1], "time.add")?;
            Ok(Value::Float(ts + d))
        }
        "diff" => {
            need(args, 2, "time.diff")?;
            let a = num(&args[0], "time.diff")?;
            let b = num(&args[1], "time.diff")?;
            Ok(Value::Float(b - a))
        }
        "iso" => {
            let ts = match args.first() {
                Some(v) => num(v, "time.iso")?,
                None => now_secs(),
            };
            Ok(Value::string(format_ts(ts, "YYYY-MM-DDTHH:mm:ss")))
        }
        other => Err(format!("time 里没有这个函数：time.{}", other)),
    }
}

fn parse_time_text(text: &str) -> Option<f64> {
    let cleaned = text.trim().replace('T', " ");
    let date_part = cleaned.split(' ').next()?;
    let mut it = date_part.split('-');
    let y: i64 = it.next()?.parse().ok()?;
    let mo: i64 = it.next()?.parse().ok()?;
    let d: i64 = it.next()?.parse().ok()?;
    let time_part = cleaned.split(' ').nth(1).unwrap_or("00:00:00");
    let mut tp = time_part.split(':');
    let h: i64 = tp.next().unwrap_or("0").parse().unwrap_or(0);
    let mi: i64 = tp.next().unwrap_or("0").parse().unwrap_or(0);
    let s: i64 = tp.next().unwrap_or("0").parse().unwrap_or(0);
    let days = days_from_civil(y, mo, d);
    Some((days * 86400 + h * 3600 + mi * 60 + s - tz_offset()) as f64)
}

/* ═════════════════════════ 5. random ═════════════════════════ */

thread_local! {
    static RNG_STATE: std::cell::Cell<u64> = std::cell::Cell::new(0);
}

fn rng_next() -> u64 {
    RNG_STATE.with(|s| {
        let mut x = s.get();
        if x == 0 {
            // 没 seed：用时间做种
            x = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0x9E37_79B9_7F4A_7C15)
                | 1;
        }
        // xorshift64*
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        s.set(x);
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    })
}

fn rng_float() -> f64 {
    (rng_next() >> 11) as f64 / (1u64 << 53) as f64
}

fn random(f: &str, args: &[Value]) -> R<Value> {
    match f {
        "seed" => {
            need(args, 1, "random.seed")?;
            let n = int_of(&args[0], "random.seed")?;
            RNG_STATE.with(|s| s.set(n as u64 | 1));
            Ok(Value::Null)
        }
        "rand" => {
            need(args, 2, "random.rand")?;
            let a = int_of(&args[0], "random.rand")?;
            let b = int_of(&args[1], "random.rand")?;
            let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
            let span = (hi - lo + 1).max(1) as f64;
            Ok(Value::Int(lo + (rng_float() * span) as i64))
        }
        "float" => Ok(Value::Float(rng_float())),
        "choice" => {
            need(args, 1, "random.choice")?;
            let items = list_of(&args[0], "random.choice")?;
            if items.is_empty() {
                return Err("random.choice：空列表没得选".into());
            }
            let i = (rng_float() * items.len() as f64) as usize % items.len();
            Ok(items[i].clone())
        }
        "shuffle" => {
            need(args, 1, "random.shuffle")?;
            let v = &args[0];
            let l = match v {
                Value::List(l) => Rc::clone(l),
                other => return Err(format!("random.shuffle 需要列表，得到 {}", other.to_display())),
            };
            let mut items = l.borrow().clone();
            let n = items.len();
            for i in (1..n).rev() {
                let j = (rng_float() * (i + 1) as f64) as usize % (i + 1);
                items.swap(i, j);
            }
            *l.borrow_mut() = items;
            Ok(Value::List(l))
        }
        "sample" => {
            need(args, 2, "random.sample")?;
            let items = list_of(&args[0], "random.sample")?;
            let k = int_of(&args[1], "random.sample")?;
            if k < 0 || k as usize > items.len() {
                return Err(format!(
                    "random.sample：要从 {} 项里抽 {} 个，数量不对",
                    items.len(),
                    k
                ));
            }
            let mut pool = items;
            let mut out: Vec<Value> = Vec::with_capacity(k as usize);
            for i in 0..k as usize {
                let j = i + (rng_float() * (pool.len() - i) as f64) as usize % (pool.len() - i);
                pool.swap(i, j);
                out.push(pool[i].clone());
            }
            Ok(mk_list(out))
        }
        "gauss" => {
            need(args, 2, "random.gauss")?;
            let mu = num(&args[0], "random.gauss")?;
            let sigma = num(&args[1], "random.gauss")?;
            // Box-Muller
            let mut u1 = rng_float();
            if u1 <= 0.0 {
                u1 = 1e-12;
            }
            let u2 = rng_float();
            let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
            Ok(Value::Float(mu + sigma * z))
        }
        "weighted" => {
            need(args, 2, "random.weighted")?;
            let items = list_of(&args[0], "random.weighted")?;
            let weights = list_of(&args[1], "random.weighted")?;
            if items.len() != weights.len() {
                return Err(format!(
                    "random.weighted：值有 {} 个，权重有 {} 个，数量要一样",
                    items.len(),
                    weights.len()
                ));
            }
            let mut total = 0.0;
            for w in &weights {
                let n = num(w, "random.weighted 的权重")?;
                if n < 0.0 {
                    return Err("random.weighted 的权重不能是负数".into());
                }
                total += n;
            }
            if total <= 0.0 {
                return Err("random.weighted 的权重加起来要大于 0".into());
            }
            let mut pick = rng_float() * total;
            for (i, w) in weights.iter().enumerate() {
                pick -= num(w, "random.weighted 的权重")?;
                if pick <= 0.0 {
                    return Ok(items[i].clone());
                }
            }
            Ok(items[items.len() - 1].clone())
        }
        other => Err(format!("random 里没有这个函数：random.{}", other)),
    }
}

/* ═════════════════════════ 6. ui ═════════════════════════ */

fn ui(f: &str, args: &[Value]) -> R<Value> {
    match f {
        "color" => {
            need(args, 2, "ui.color")?;
            let color = s_of(&args[0]);
            let text = s_of(&args[1]);
            let code = match color.to_lowercase().as_str() {
                "red" => "31",
                "green" => "32",
                "yellow" => "33",
                "blue" => "34",
                "magenta" => "35",
                "cyan" => "36",
                "white" => "37",
                "gray" | "grey" => "90",
                other => {
                    return Err(format!(
                        "ui.color 不认识这个颜色：{}（可用 red/green/yellow/blue/magenta/cyan/white/gray）",
                        other
                    ))
                }
            };
            Ok(Value::string(format!("\u{1b}[{}m{}\u{1b}[0m", code, text)))
        }
        "bar" => {
            need(args, 2, "ui.bar")?;
            let v = num(&args[0], "ui.bar")?;
            let max = num(&args[1], "ui.bar")?;
            let width = match args.get(2) {
                Some(w) => int_of(w, "ui.bar")?.clamp(4, 80) as usize,
                None => 30,
            };
            Ok(Value::string(make_bar(v, max, width)))
        }
        "title" => {
            need(args, 1, "ui.title")?;
            let t = s_of(&args[0]);
            let line = "─".repeat(t.chars().count().max(8));
            Ok(Value::string(format!("{}\n{}\n{}", line, t, line)))
        }
        // 剩下的都靠"输出控制字符"实现，交给 VM 的输出通道会原样打印
        "clear" => Ok(Value::string("\u{1b}[2J\u{1b}[H".to_string())),
        "clear_line" => {
            let n = match args.first() {
                Some(v) => int_of(v, "ui.clear_line")?.clamp(0, 100),
                None => 0,
            };
            let mut s = String::new();
            for _ in 0..=n {
                s.push_str("\u{1b}[2K\r");
                if n > 0 {
                    s.push_str("\u{1b}[1A");
                }
            }
            Ok(Value::string(s))
        }
        "progress" => {
            need(args, 2, "ui.progress")?;
            let v = num(&args[0], "ui.progress")?;
            let max = num(&args[1], "ui.progress")?;
            let pct = if max <= 0.0 { 0.0 } else { (v / max * 100.0).clamp(0.0, 100.0) };
            Ok(Value::string(format!("\r{} {:>3.0}%", make_bar(v, max, 30), pct)))
        }
        "menu" => Err("ui.menu 需要读键盘输入，只能在 VM 里用（IDE 里按 F5/F6 跑即可）；AOT 编译的程序暂不支持".into()),
        "input" => Err("ui.input 需要读键盘输入，只能在 VM 里用（IDE 里按 F5/F6 跑即可）；AOT 编译的程序暂不支持".into()),
        "table" => {
            need(args, 2, "ui.table")?;
            let head = list_of(&args[0], "ui.table")?;
            let rows = list_of(&args[1], "ui.table")?;
            let mut cols = head.len();
            for r in &rows {
                if let Value::List(l) = r {
                    cols = cols.max(l.borrow().len());
                }
            }
            let mut widths = vec![0usize; cols];
            for (i, h) in head.iter().enumerate() {
                widths[i] = widths[i].max(display_width(&s_of(h)));
            }
            for r in &rows {
                if let Value::List(l) = r {
                    for (i, c) in l.borrow().iter().enumerate() {
                        widths[i] = widths[i].max(display_width(&s_of(c)));
                    }
                }
            }
            let mut out = String::new();
            out.push_str(&render_row(&head, &widths));
            out.push('\n');
            for r in &rows {
                if let Value::List(l) = r {
                    out.push_str(&render_row(&l.borrow(), &widths));
                    out.push('\n');
                }
            }
            Ok(Value::string(out.trim_end_matches('\n').to_string()))
        }
        "spinner" => Err("ui.spinner 还没做（后台动画需要宿主线程），先用 ui.progress".into()),
        other => Err(format!("ui 里没有这个函数：ui.{}", other)),
    }
}

fn make_bar(v: f64, max: f64, width: usize) -> String {
    let ratio = if max <= 0.0 { 0.0 } else { (v / max).clamp(0.0, 1.0) };
    let filled = (ratio * width as f64).round() as usize;
    format!("[{}{}]", "█".repeat(filled), "░".repeat(width.saturating_sub(filled)))
}

/// 显示宽度：中日韩全角算 2，其余算 1（表格对齐用）
fn display_width(s: &str) -> usize {
    s.chars()
        .map(|c| {
            let u = c as u32;
            if (0x1100..=0x115F).contains(&u)
                || (0x2E80..=0xA4CF).contains(&u)
                || (0xAC00..=0xD7A3).contains(&u)
                || (0xF900..=0xFAFF).contains(&u)
                || (0xFE30..=0xFE6F).contains(&u)
                || (0xFF00..=0xFF60).contains(&u)
                || (0xFFE0..=0xFFE6).contains(&u)
            {
                2
            } else {
                1
            }
        })
        .sum()
}

fn render_row(cells: &[Value], widths: &[usize]) -> String {
    let mut out = String::new();
    for (i, w) in widths.iter().enumerate() {
        let text = cells.get(i).map(s_of).unwrap_or_default();
        out.push_str(&text);
        let pad = w.saturating_sub(display_width(&text));
        out.push_str(&" ".repeat(pad + 2));
    }
    out.trim_end().to_string()
}

/* ═════════════════════════ 7. re ═════════════════════════ */

fn re(f: &str, args: &[Value]) -> R<Value> {
    // 用 float-lang 自带的简易正则引擎（避免引第三方依赖）
    match f {
        "test" => {
            need(args, 2, "re.test")?;
            let pat = s_of(&args[0]);
            let text = s_of(&args[1]);
            Ok(Value::Bool(regex_find_all(&pat, &text).map(|v| !v.is_empty()).map_err(|m| format!("re.test：{}", m))?))
        }
        "findall" => {
            need(args, 2, "re.findall")?;
            let pat = s_of(&args[0]);
            let text = s_of(&args[1]);
            let hits = regex_find_all(&pat, &text).map_err(|m| format!("re.findall：{}", m))?;
            Ok(mk_list(hits.into_iter().map(Value::string).collect()))
        }
        "replace" => {
            need(args, 3, "re.replace")?;
            let pat = s_of(&args[0]);
            let rep = s_of(&args[1]);
            let text = s_of(&args[2]);
            regex_replace(&pat, &rep, &text).map(Value::string).map_err(|m| format!("re.replace：{}", m))
        }
        "split" => {
            need(args, 2, "re.split")?;
            let pat = s_of(&args[0]);
            let text = s_of(&args[1]);
            let parts = regex_split(&pat, &text).map_err(|m| format!("re.split：{}", m))?;
            Ok(mk_list(parts.into_iter().map(Value::string).collect()))
        }
        "search" | "match" => {
            need(args, 2, "re.search")?;
            let pat = s_of(&args[0]);
            let text = s_of(&args[1]);
            let hits = regex_find_all(&pat, &text).map_err(|m| format!("re.search：{}", m))?;
            match hits.first() {
                Some(h) => {
                    let mut d = Dict::new();
                    d.set(Rc::from("text"), Value::string(h.clone()));
                    d.set(Rc::from("start"), Value::Int(text.find(h).unwrap_or(0) as i64));
                    d.set(Rc::from("end"), Value::Int((text.find(h).unwrap_or(0) + h.len()) as i64));
                    d.set(Rc::from("groups"), mk_list(vec![]));
                    Ok(mk_dict(d))
                }
                None => Ok(Value::Null),
            }
        }
        "escape" => {
            need(args, 1, "re.escape")?;
            let s = s_of(&args[0]);
            let mut out = String::new();
            for c in s.chars() {
                if "\\.^$|?*+()[]{}".contains(c) {
                    out.push('\\');
                }
                out.push(c);
            }
            Ok(Value::string(out))
        }
        other => Err(format!("re 里没有这个函数：re.{}", other)),
    }
}

/* ── 迷你正则：支持 . * + ? [abc] [a-z] ^ $ \d \w \s 与分组 ──
 * 不引第三方库是为了保持 float-lang 零依赖；语法范围见 docs/标准库规划.md */

/// 取模式里唯一的一对括号内容（捕获组）。没有 / 多组 / 有嵌套 → None。
/// v1 只支持"一个分组"的提取，够 `href="([^"]+)"` 这类爬取用。
fn single_group(pat: &str) -> Option<(String, String)> {
    let chars: Vec<char> = pat.chars().collect();
    let mut open: Option<usize> = None;
    let mut close: Option<usize> = None;
    let mut depth = 0i32;
    let mut i = 0usize;
    while i < chars.len() {
        match chars[i] {
            '\\' => {
                i += 2;
                continue;
            }
            '(' => {
                depth += 1;
                if depth > 1 {
                    return None; // 嵌套 → 不支持
                }
                open = Some(i);
            }
            ')' => {
                depth -= 1;
                if depth == 0 {
                    if close.is_some() {
                        return None; // 多组 → 不支持
                    }
                    close = Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    match (open, close) {
        // 前缀 + 组：`href="([^"]+)"` → (前缀 `href="`, 组 `[^"]+`)，这样能算出组在匹配里的偏移
        (Some(a), Some(b)) if b > a + 1 => Some((
            chars[..a].iter().collect::<String>(),
            chars[a + 1..b].iter().collect::<String>(),
        )),
        _ => None,
    }
}

fn regex_find_all(pat: &str, text: &str) -> Result<Vec<String>, String> {
    let prog = re_compile(pat)?;
    let group = match single_group(pat) {
        Some((pre, inner)) => Some((re_compile(&pre)?, re_compile(&inner)?)),
        None => None,
    };
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i <= chars.len() {
        match prog_match_at(&prog, &chars, i) {
            Some((len, _groups)) => {
                let whole: String = chars[i..i + len].iter().collect();
                // 有分组就返回分组内容（`href="([^"]+)"` → 只要 URL）
                let hit = match &group {
                    Some((pre, inner)) => {
                        // 先匹配前缀拿到组的起点，再在起点上匹配组本身
                        let off = prog_match_at(pre, &chars, i).map(|(l, _)| l).unwrap_or(0);
                        match prog_match_at(inner, &chars, i + off) {
                            Some((glen, _)) => chars[i + off..i + off + glen].iter().collect::<String>(),
                            None => whole,
                        }
                    }
                    None => whole,
                };
                out.push(hit);
                i += if len == 0 { 1 } else { len };
            }
            None => i += 1,
        }
    }
    Ok(out)
}

fn regex_replace(pat: &str, rep: &str, text: &str) -> Result<String, String> {
    let prog = re_compile(pat)?;
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0usize;
    while i < chars.len() {
        match prog_match_at(&prog, &chars, i) {
            Some((len, _)) => {
                out.push_str(rep);
                i += if len == 0 { 1 } else { len };
            }
            None => {
                out.push(chars[i]);
                i += 1;
            }
        }
    }
    Ok(out)
}

fn regex_split(pat: &str, text: &str) -> Result<Vec<String>, String> {
    let prog = re_compile(pat)?;
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut i = 0usize;
    while i < chars.len() {
        match prog_match_at(&prog, &chars, i) {
            Some((len, _)) if len > 0 => {
                out.push(std::mem::take(&mut cur));
                i += len;
            }
            _ => {
                cur.push(chars[i]);
                i += 1;
            }
        }
    }
    out.push(cur);
    Ok(out)
}

#[derive(Clone)]
enum ReTok {
    Lit(char),
    Any,
    Class(Vec<(char, char)>, bool),
    Digit(bool),
    Word(bool),
    Space(bool),
    Start,
    End,
}

#[derive(Clone)]
struct ReProg {
    toks: Vec<(ReTok, u8, u8)>, // (token, min, max)；max=255 表示无限
}

fn re_compile(pat: &str) -> Result<ReProg, String> {
    let chars: Vec<char> = pat.chars().collect();
    let mut toks: Vec<(ReTok, u8, u8)> = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        let tok = match c {
            '.' => {
                i += 1;
                ReTok::Any
            }
            '^' => {
                i += 1;
                ReTok::Start
            }
            '$' => {
                i += 1;
                ReTok::End
            }
            '\\' => {
                i += 1;
                let e = *chars.get(i).ok_or("正则末尾的反斜杠没有内容")?;
                i += 1;
                match e {
                    'd' => ReTok::Digit(false),
                    'D' => ReTok::Digit(true),
                    'w' => ReTok::Word(false),
                    'W' => ReTok::Word(true),
                    's' => ReTok::Space(false),
                    'S' => ReTok::Space(true),
                    other => ReTok::Lit(other),
                }
            }
            '[' => {
                i += 1;
                let mut neg = false;
                if chars.get(i) == Some(&'^') {
                    neg = true;
                    i += 1;
                }
                let mut ranges: Vec<(char, char)> = Vec::new();
                let mut first = true;
                while i < chars.len() && (chars[i] != ']' || first) {
                    first = false;
                    let a = chars[i];
                    if chars.get(i + 1) == Some(&'-') && chars.get(i + 2).is_some() && chars[i + 2] != ']' {
                        ranges.push((a, chars[i + 2]));
                        i += 3;
                    } else {
                        ranges.push((a, a));
                        i += 1;
                    }
                }
                if i >= chars.len() {
                    return Err("正则里的 [ 没有闭合".into());
                }
                i += 1; // ]
                ReTok::Class(ranges, neg)
            }
            '(' | ')' => {
                // v1 不支持分组捕获（只当分组用），直接跳过
                i += 1;
                continue;
            }
            other => {
                i += 1;
                ReTok::Lit(other)
            }
        };
        // 量词
        let mut min = 1u8;
        let mut max = 1u8;
        if let Some(&q) = chars.get(i) {
            match q {
                '*' => {
                    min = 0;
                    max = 255;
                    i += 1;
                }
                '+' => {
                    min = 1;
                    max = 255;
                    i += 1;
                }
                '?' => {
                    min = 0;
                    max = 1;
                    i += 1;
                }
                '{' => {
                    // {n} / {n,m}
                    if let Some(end) = chars[i..].iter().position(|c| *c == '}') {
                        let body: String = chars[i + 1..i + end].iter().collect();
                        let mut it = body.split(',');
                        let a: u8 = it.next().unwrap_or("1").trim().parse().unwrap_or(1);
                        let b: u8 = match it.next() {
                            Some(s) if !s.trim().is_empty() => s.trim().parse().unwrap_or(a),
                            Some(_) => 255,
                            None => a,
                        };
                        min = a;
                        max = b;
                        i += end + 1;
                    }
                }
                _ => {}
            }
        }
        toks.push((tok, min, max));
    }
    Ok(ReProg { toks })
}

fn tok_match(tok: &ReTok, c: char, chars: &[char], pos: usize) -> bool {
    match tok {
        ReTok::Lit(x) => *x == c,
        ReTok::Any => c != '\n',
        ReTok::Class(ranges, neg) => {
            let inside = ranges.iter().any(|(a, b)| c >= *a && c <= *b);
            inside != *neg
        }
        ReTok::Digit(neg) => c.is_ascii_digit() != *neg,
        ReTok::Word(neg) => (c.is_alphanumeric() || c == '_') != *neg,
        ReTok::Space(neg) => c.is_whitespace() != *neg,
        ReTok::Start => pos == 0,
        ReTok::End => pos == chars.len(),
    }
}

fn prog_match_at(prog: &ReProg, chars: &[char], start: usize) -> Option<(usize, Vec<String>)> {
    fn go(p: &ReProg, ti: usize, chars: &[char], pos: usize) -> Option<usize> {
        if ti == p.toks.len() {
            return Some(pos);
        }
        let (tok, min, max) = &p.toks[ti];
        // 贪婪：先吃到最多，再回溯
        let mut count = 0usize;
        let mut cur = pos;
        while count < *max as usize && cur < chars.len() && tok_match(tok, chars[cur], chars, cur) {
            cur += 1;
            count += 1;
        }
        loop {
            if count >= *min as usize {
                if let Some(end) = go(p, ti + 1, chars, pos + count) {
                    return Some(end);
                }
            }
            if count == 0 {
                // 零宽断言（^ $）
                if matches!(tok, ReTok::Start | ReTok::End) {
                    return go(p, ti + 1, chars, pos);
                }
                return None;
            }
            count -= 1;
        }
    }
    go(prog, 0, chars, start).map(|end| (end - start, Vec::new()))
}

/* ═════════════════════════ 8. net ═════════════════════════ */

fn net(f: &str, args: &[Value]) -> R<Value> {
    match f {
        "fetch" => {
            need(args, 1, "net.fetch")?;
            crate::builtins::call_builtin("fetch", args)
        }
        "post" => {
            need(args, 2, "net.post")?;
            crate::builtins::call_builtin("post", args)
        }
        "encode" => {
            need(args, 1, "net.encode")?;
            Ok(Value::string(url_encode(&s_of(&args[0]))))
        }
        "decode" => {
            need(args, 1, "net.decode")?;
            Ok(Value::string(url_decode(&s_of(&args[0]))))
        }
        "query" => {
            need(args, 1, "net.query")?;
            let url = s_of(&args[0]);
            let q = url.split('?').nth(1).unwrap_or("");
            let mut d = Dict::new();
            for pair in q.split('&') {
                if pair.is_empty() {
                    continue;
                }
                let mut it = pair.splitn(2, '=');
                let k = url_decode(it.next().unwrap_or(""));
                let v = url_decode(it.next().unwrap_or(""));
                d.set(Rc::from(k.as_str()), Value::string(v));
            }
            Ok(mk_dict(d))
        }
        "ip" => Ok(Value::str(local_ip().as_str())),
        "download" => {
            need(args, 2, "net.download")?;
            let url = s_of(&args[0]);
            let body = crate::builtins::call_builtin("fetch", &[Value::string(url)])?;
            let text = match body {
                Value::Str(s) => s.to_string(),
                other => other.to_display(),
            };
            let path = crate::host::resolve_path(&s_of(&args[1]));
            std::fs::write(&path, text.as_bytes())
                .map(|_| Value::Bool(true))
                .map_err(|e| format!("net.download 写不了文件：{}", e))
        }
        "request" => {
            need(args, 2, "net.request")?;
            let method = s_of(&args[0]).to_uppercase();
            let url = s_of(&args[1]);
            let body = match &method[..] {
                "GET" => crate::builtins::call_builtin("fetch", &[Value::string(url)])?,
                _ => {
                    let payload = args.get(2).cloned().unwrap_or_else(|| Value::str(""));
                    crate::builtins::call_builtin("post", &[Value::string(url), payload])?
                }
            };
            let mut d = Dict::new();
            d.set(Rc::from("code"), Value::Int(200));
            d.set(Rc::from("body"), body);
            d.set(Rc::from("headers"), mk_dict(Dict::new()));
            Ok(mk_dict(d))
        }
        "listen" | "server_close" => Err(format!(
            "net.{} 需要在宿主里起 HTTP 服务，v1 先没做（要用可以做成本地脚本 + 外部服务）",
            f
        )),
        other => Err(format!("net 里没有这个函数：net.{}", other)),
    }
}

fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let h = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                match u8::from_str_radix(h, 16) {
                    Ok(v) => {
                        out.push(v);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

fn local_ip() -> String {
    use std::net::UdpSocket;
    if let Ok(sock) = UdpSocket::bind("0.0.0.0:0") {
        if sock.connect("8.8.8.8:80").is_ok() {
            if let Ok(addr) = sock.local_addr() {
                return addr.ip().to_string();
            }
        }
    }
    "127.0.0.1".to_string()
}
