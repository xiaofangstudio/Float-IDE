//! AST 对拍基准：把一份 .co 源码解析成 AST，再打成规范化文本。
//!
//! 用法：`ast_dump <源码.co>`，stdout 输出规范化 AST 文本。
//! `bootstrap/copper_parse.co` 必须产出逐字节一致的同一份文本。
//!
//! 格式约定（两侧必须完全一致）：
//!   - 每条语句一行，行首用两个空格表示嵌套层级
//!   - 表达式压成单行紧凑式：`Bin(Add,Var(n),Num(1))`
//!   - 数字用 float_lang::value::fmt_float，字符串按 esc 转义包双引号
//!   - 空体 / 缺省项统一写 `-`

use float_lang::ast::{AssignOp, Expr, ExprKind, Op, Param, Program, Stmt, StmtKind, Target};
use float_lang::TypeName;
use std::rc::Rc;

fn ty_name(t: &TypeName) -> &'static str {
    match t {
        TypeName::Int => "int",
        TypeName::Float => "float",
        TypeName::Str => "str",
        TypeName::Bool => "bool",
        TypeName::List => "list",
        TypeName::Dict => "dict",
        TypeName::Ptr => "ptr",
        TypeName::Var => "var",
        TypeName::Named => "named",
    }
}

fn op_name(o: &Op) -> &'static str {
    match o {
        Op::Add => "Add",
        Op::Sub => "Sub",
        Op::Mul => "Mul",
        Op::Div => "Div",
        Op::IDiv => "IDiv",
        Op::Mod => "Mod",
        Op::Pow => "Pow",
        Op::Eq => "Eq",
        Op::Ne => "Ne",
        Op::Lt => "Lt",
        Op::Le => "Le",
        Op::Gt => "Gt",
        Op::Ge => "Ge",
        Op::And => "And",
        Op::Or => "Or",
        Op::BitAnd => "BitAnd",
        Op::BitOr => "BitOr",
        Op::BitXor => "BitXor",
        Op::Shl => "Shl",
        Op::Shr => "Shr",
        Op::Neg => "Neg",
        Op::Not => "Not",
        Op::BitNot => "BitNot",
        Op::Addr => "Addr",
        Op::Deref => "Deref",
    }
}

fn asg_name(o: &AssignOp) -> &'static str {
    match o {
        AssignOp::Set => "Set",
        AssignOp::Add => "Add",
        AssignOp::Sub => "Sub",
        AssignOp::Mul => "Mul",
        AssignOp::Div => "Div",
        AssignOp::Mod => "Mod",
        AssignOp::Pow => "Pow",
    }
}

/// 字符串按词法层同样的规则转义（与 dump_tokens 一致）
fn esc(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out
}

fn expr_str(e: &Expr) -> String {
    match &e.kind {
        ExprKind::Int(n) => format!("Num({})", n),
        ExprKind::Num(f) => format!("Num({})", float_lang::value::fmt_float(*f)),
        ExprKind::Str(s) => format!("Str(\"{}\")", esc(s)),
        ExprKind::Bool(b) => format!("Bool({})", b),
        ExprKind::Null => "Null".to_string(),
        ExprKind::Var(v) => format!("Var({})", v),
        ExprKind::SelfRef => "Self".to_string(),
        ExprKind::List(items) => {
            let parts: Vec<String> = items.iter().map(expr_str).collect();
            format!("List([{}])", parts.join(","))
        }
        ExprKind::Dict(items) => {
            let parts: Vec<String> = items
                .iter()
                .map(|(k, v)| format!("({},{})", expr_str(k), expr_str(v)))
                .collect();
            format!("Dict([{}])", parts.join(","))
        }
        ExprKind::Un { op, a } => format!("Un({},{})", op_name(op), expr_str(a)),
        ExprKind::Bin { op, a, b } => {
            format!("Bin({},{},{})", op_name(op), expr_str(a), expr_str(b))
        }
        ExprKind::Ternary { cond, a, b } => {
            format!("Tern({},{},{})", expr_str(cond), expr_str(a), expr_str(b))
        }
        ExprKind::Lambda { params, body } => {
            let ps: Vec<String> = params.iter().map(param_str).collect();
            format!("Lambda([{}],{})", ps.join(","), expr_str(body))
        }
        ExprKind::Idx { obj, index } => format!("Idx({},{})", expr_str(obj), expr_str(index)),
        ExprKind::Slice { obj, from, to } => {
            let f = from.as_ref().map(|x| expr_str(x)).unwrap_or_else(|| "-".into());
            let t = to.as_ref().map(|x| expr_str(x)).unwrap_or_else(|| "-".into());
            format!("Slice({},{},{})", expr_str(obj), f, t)
        }
        ExprKind::Member { obj, name } => format!("Member({},{})", expr_str(obj), name),
        ExprKind::Call { callee, args } => {
            let parts: Vec<String> = args.iter().map(expr_str).collect();
            format!("Call({},[{}])", expr_str(callee), parts.join(","))
        }
        ExprKind::New { class, args } => {
            let parts: Vec<String> = args.iter().map(expr_str).collect();
            format!("New({},[{}])", class, parts.join(","))
        }
    }
}

fn param_str(p: &Param) -> String {
    let ty = p.ty.as_ref().map(ty_name).unwrap_or("-");
    let def = p.default.as_ref().map(|e| expr_str(e)).unwrap_or_else(|| "-".into());
    format!(
        "({}:{}:{}{})",
        p.name,
        ty,
        def,
        if p.lazy { ":lazy" } else { "" }
    )
}

fn target_str(t: &Target) -> String {
    match t {
        Target::Var(n) => format!("Var({})", n),
        Target::Member(obj, name) => format!("Member({},{})", expr_str(obj), name),
        Target::Idx(obj, idx) => format!("Idx({},{})", expr_str(obj), expr_str(idx)),
        Target::Deref(e) => format!("Deref({})", expr_str(e)),
    }
}

fn args_str(items: &[Expr]) -> String {
    let parts: Vec<String> = items.iter().map(expr_str).collect();
    format!("[{}]", parts.join(","))
}

fn line(out: &mut String, level: usize, s: &str) {
    for _ in 0..level {
        out.push_str("  ");
    }
    out.push_str(s);
    out.push('\n');
}

/// 打印一组语句（level 为当前层级）
fn dump_stmts(out: &mut String, level: usize, stmts: &[Stmt]) {
    for st in stmts {
        dump_stmt(out, level, st);
    }
}

fn dump_stmt(out: &mut String, level: usize, st: &Stmt) {
    match &st.kind {
        StmtKind::Decl { ty, name, init } => {
            let i = init.as_ref().map(expr_str).unwrap_or_else(|| "-".into());
            line(out, level, &format!("Decl {} {} {}", ty_name(ty), name, i));
        }
        StmtKind::Assign { target, op, value } => {
            line(
                out,
                level,
                &format!("Assign {} {} {}", target_str(target), asg_name(op), expr_str(value)),
            );
        }
        StmtKind::IncDec { target, delta, prefix } => {
            line(
                out,
                level,
                &format!("IncDec {} {} {}", target_str(target), delta, prefix),
            );
        }
        StmtKind::ExprStmt(e) => line(out, level, &format!("Expr {}", expr_str(e))),
        StmtKind::Print(items) => line(out, level, &format!("Print {}", args_str(items))),
        StmtKind::Report(items) => line(out, level, &format!("Report {}", args_str(items))),
        StmtKind::Debug { level: lv, args } => {
            line(out, level, &format!("Debug {} {}", lv, args_str(args)))
        }
        StmtKind::Ask(e) => {
            let s = e.as_ref().map(expr_str).unwrap_or_else(|| "-".into());
            line(out, level, &format!("Ask {}", s));
        }
        StmtKind::If { branches, else_body } => {
            line(out, level, "If");
            for (cond, body) in branches {
                let c = cond.as_ref().map(expr_str).unwrap_or_else(|| "else".into());
                line(out, level + 1, &format!("Branch {}", c));
                dump_stmts(out, level + 2, body);
            }
            if let Some(b) = else_body {
                line(out, level + 1, "Else");
                dump_stmts(out, level + 2, b);
            }
        }
        StmtKind::While { cond, body } => {
            line(out, level, &format!("While {}", expr_str(cond)));
            dump_stmts(out, level + 1, body);
        }
        StmtKind::WaitUntil { cond, body } => {
            line(out, level, &format!("WaitUntil {}", expr_str(cond)));
            dump_stmts(out, level + 1, body);
        }
        StmtKind::IfOut(e) => line(out, level, &format!("IfOut {}", expr_str(e))),
        StmtKind::TimeRead => line(out, level, "TimeRead"),
        StmtKind::TimeReset => line(out, level, "TimeReset"),
        StmtKind::Repeat { count, body } => {
            line(out, level, &format!("Repeat {}", expr_str(count)));
            dump_stmts(out, level + 1, body);
        }
        StmtKind::ForRange {
            var,
            from,
            to,
            step,
            inclusive,
            body,
        } => {
            let f = from.as_ref().map(expr_str).unwrap_or_else(|| "-".into());
            let s = step.as_ref().map(expr_str).unwrap_or_else(|| "-".into());
            line(
                out,
                level,
                &format!(
                    "ForRange {} {} {} {} {}",
                    var,
                    f,
                    expr_str(to),
                    s,
                    if *inclusive { 1 } else { 0 }
                ),
            );
            dump_stmts(out, level + 1, body);
        }
        StmtKind::ForIn {
            var,
            value_name,
            iter,
            body,
        } => {
            let v = value_name.as_deref().unwrap_or("-");
            line(
                out,
                level,
                &format!("ForIn {} {} {}", var, v, expr_str(iter)),
            );
            dump_stmts(out, level + 1, body);
        }
        StmtKind::Destructure { names, value } => {
            let parts: Vec<&str> = names.iter().map(|n| n.as_ref()).collect();
            line(
                out,
                level,
                &format!("Destructure [{}] {}", parts.join(","), expr_str(value)),
            );
        }
        StmtKind::Break => line(out, level, "Break"),
        StmtKind::Continue => line(out, level, "Continue"),
        StmtKind::Stop => line(out, level, "Stop"),
        StmtKind::Return(e) => {
            let s = e.as_ref().map(expr_str).unwrap_or_else(|| "-".into());
            line(out, level, &format!("Return {}", s));
        }
        StmtKind::FuncDef {
            name,
            params,
            ret,
            body,
        } => {
            let ps: Vec<String> = params.iter().map(param_str).collect();
            let r = ret.as_deref().unwrap_or("-");
            line(
                out,
                level,
                &format!("FuncDef {} [{}] {}", name, ps.join(","), r),
            );
            dump_stmts(out, level + 1, body);
        }
        StmtKind::Try {
            body,
            catch_name,
            catch_body,
            finally,
        } => {
            line(out, level, "Try");
            dump_stmts(out, level + 1, body);
            line(
                out,
                level + 1,
                &format!("Catch {}", catch_name.as_deref().unwrap_or("-")),
            );
            if let Some(cb) = catch_body {
                dump_stmts(out, level + 2, cb);
            }
            line(out, level + 1, "Finally");
            if let Some(fb) = finally {
                dump_stmts(out, level + 2, fb);
            }
        }
        StmtKind::Throw(e) => line(out, level, &format!("Throw {}", expr_str(e))),
        // Nop（空行/纯注释）不带语义，两侧统一不打印，便于逐行对拍
        StmtKind::Nop => {}
        StmtKind::Group(list) => {
            line(out, level, "Group");
            dump_stmts(out, level + 1, list);
        }
        StmtKind::Unsupported { what } => line(out, level, &format!("Unsupported {}", what)),
    }
}

fn dump_program(p: &Program) -> String {
    let mut out = String::new();
    line(&mut out, 0, "Program");
    dump_stmts(&mut out, 1, &p.body);
    for e in &p.errors {
        line(
            &mut out,
            0,
            &format!("ERROR {}:{} {}", e.span.line, e.span.col, e.message),
        );
    }
    out
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("用法：ast_dump <源码.co>");
        std::process::exit(2);
    }
    let path = &args[0];
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("读不了 {}：{}", path, e);
            std::process::exit(2);
        }
    };
    let file: Rc<str> = Rc::from(path.as_str());
    let p = float_lang::parser::parse_program(&file, &src);
    print!("{}", dump_program(&p));
}
