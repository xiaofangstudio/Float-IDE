//! 语法分析：行式 + 缩进块 + 递归下降表达式。
//!
//! 结构与 `src/lang/parse.ts` 对齐，行为上刻意保持一致的关键点：
//! * 一次看一行，行尾的 `{` 视为"块开始"而不是字典（字典字面量必然以 `}` 结尾）
//! * `} else {` / `} elif (…) {` 能与上一块的 if 合并
//! * 括号在 `if (…)` / `print(…)` 里是可选的
//! * `for i = 1 to 10` 含两端，`for i in range(n)` 不含终点
//! * 中文标识符直接可用
//!
//! 与 TS 版的差异：Rust 版把"这版还没实现"的语句（class / try / import / ask / wait）
//! 明确解析成 `Unsupported`，而不是静默失败——这样 VM 能给出清楚的错误。

use crate::ast::*;
use crate::error::{FloatError, Span};
use crate::token::{code_part, tokenize_line, TokKind, Token};
use crate::TypeName;
use std::rc::Rc;

const TYPE_WORDS: [&str; 8] = ["int", "float", "str", "bool", "list", "dict", "ptr", "var"];
const FUNC_WORDS: [&str; 5] = ["function", "fn", "Function", "func", "f"];
const ASSIGN_OPS: [&str; 7] = ["=", "+=", "-=", "*=", "/=", "%=", "**="];

#[derive(Clone, Debug)]
#[allow(dead_code)]
struct Line {
    no: u32,
    indent: usize,
    code: String,
    raw: String,
    is_comment: bool,
    skipped: bool,
}

/// 把源码切成"有效行"：去 BOM、统一换行、Tab 展开成 4 空格、剥掉尾注释。
fn split_source(src: &str) -> Vec<Line> {
    let text = src.trim_start_matches('\u{feff}').replace("\r\n", "\n").replace('\r', "\n");
    let mut out = Vec::new();
    for (idx, raw_line) in text.split('\n').enumerate() {
        let expanded: String = raw_line.replace('\t', "    ");
        let indent = expanded.chars().take_while(|c| *c == ' ').count();
        let trimmed = expanded.trim();
        if trimmed.is_empty() {
            continue;
        }
        let no = idx as u32 + 1;
        if trimmed.starts_with("--") {
            out.push(Line {
                no,
                indent,
                code: String::new(),
                raw: trimmed.to_string(),
                is_comment: false,
                skipped: true,
            });
            continue;
        }
        let (code, comment) = code_part(trimmed);
        let is_comment = trimmed.starts_with("//");
        let cleaned = if is_comment {
            String::new()
        } else {
            // 行尾的 { 是"块开始"，不是字典字面量
            let c = code.trim();
            let c = c.strip_suffix('{').map(|s| s.trim_end()).unwrap_or(c);
            c.trim().to_string()
        };
        let _ = comment;
        out.push(Line {
            no,
            indent,
            code: cleaned,
            raw: trimmed.to_string(),
            is_comment,
            skipped: false,
        });
    }
    out
}

/// 表达式优先级。数值越大越紧。
fn prec_of(op: &str) -> Option<u8> {
    Some(match op {
        "||" => 10,
        "&&" => 15,
        "|" => 20,
        "^" => 25,
        "&" => 30,
        "==" | "!=" | "===" | "!==" => 35,
        "<" | ">" | "<=" | ">=" => 40,
        "<<" | ">>" => 45,
        "+" | "-" => 50,
        "*" | "/" | "%" => 60,
        "**" => 70,
        _ => return None,
    })
}

struct ExprParser<'a> {
    toks: &'a [Token],
    pos: usize,
    file: &'a Rc<str>,
}

type EResult<T> = Result<T, FloatError>;

impl<'a> ExprParser<'a> {
    fn new(toks: &'a [Token], file: &'a Rc<str>) -> ExprParser<'a> {
        ExprParser { toks, pos: 0, file }
    }

    fn cur(&self) -> Option<&Token> {
        self.toks.get(self.pos)
    }

    fn at_end(&self) -> bool {
        self.pos >= self.toks.len()
    }

    fn at(&self, text: &str) -> bool {
        self.cur().map(|t| t.text == text).unwrap_or(false)
    }

    fn bump(&mut self) -> Option<&'a Token> {
        let t = self.toks.get(self.pos);
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn expect(&mut self, text: &str, hint: &str) -> EResult<()> {
        if self.at(text) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.err(format!("{}（期望 {}，实际是 {}）", hint, text, self.describe())))
        }
    }

    fn describe(&self) -> String {
        match self.cur() {
            None => "行尾".to_string(),
            Some(t) => match t.kind {
                TokKind::Eof => "行尾".to_string(),
                TokKind::Str => format!("字符串「{}」", t.text),
                _ => format!("「{}」", t.text),
            },
        }
    }

    fn err(&self, msg: impl Into<String>) -> FloatError {
        let (line, col, end_col) = match self.cur() {
            Some(t) => (t.line, t.col, t.end_col),
            None => {
                let last = self.toks.last();
                match last {
                    Some(t) => (t.line, t.end_col, t.end_col),
                    None => (1, 1, 2),
                }
            }
        };
        FloatError::new(msg, Span::new(self.file, line, col, end_col))
    }

    fn span_of(&self, t: &Token) -> Span {
        Span::new(self.file, t.line, t.col, t.end_col)
    }

    /// 尾随整行的表达式：必须把 token 吃干净
    fn parse_all(&mut self) -> EResult<Expr> {
        let e = self.parse_expr(0)?;
        if !self.at_end() {
            return Err(self.err(format!("这条语句看不懂：{}", self.describe())));
        }
        Ok(e)
    }

    fn parse_expr(&mut self, min_prec: u8) -> EResult<Expr> {
        let mut left = self.parse_unary()?;
        loop {
            let (op_text, tok) = match self.cur() {
                Some(t) if t.kind == TokKind::Op => (t.text.clone(), t.clone()),
                _ => break,
            };
            let prec = match prec_of(&op_text) {
                Some(p) => p,
                None => break,
            };
            if prec < min_prec {
                break;
            }
            self.pos += 1;
            let op = Op::from_str(&op_text).ok_or_else(|| self.err("不认识的运算符"))?;
            // ** 右结合，其余左结合
            let next_min = if op == Op::Pow { prec } else { prec + 1 };
            let right = self.parse_expr(next_min)?;
            let span = Span::new(self.file, tok.line, tok.col, right.span.end_col);
            left = Expr::new(
                ExprKind::Bin { op, a: Box::new(left), b: Box::new(right) },
                span,
            );
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> EResult<Expr> {
        let t = match self.cur() {
            Some(t) => t.clone(),
            None => return Err(self.err("缺少表达式")),
        };
        if t.kind == TokKind::Op {
            let op = match t.text.as_str() {
                "-" => Some(Op::Neg),
                "!" => Some(Op::Not),
                "~" => Some(Op::BitNot),
                _ => None,
            };
            if let Some(op) = op {
                self.pos += 1;
                let a = self.parse_unary()?;
                let span = Span::new(self.file, t.line, t.col, a.span.end_col);
                return Ok(Expr::new(ExprKind::Un { op, a: Box::new(a) }, span));
            }
        }
        // 前缀 new：new 类名(参数)
        if t.kind == TokKind::Kw && t.text == "new" {
            self.pos += 1;
            let name_tok = self.bump().ok_or_else(|| self.err("new 后面要跟类名"))?.clone();
            if !name_tok.is_ident() && name_tok.kind != TokKind::Kw {
                return Err(self.err("new 后面要跟类名"));
            }
            let args = if self.at("(") { self.parse_args()? } else { Vec::new() };
            let span = Span::new(self.file, t.line, t.col, name_tok.end_col);
            return Ok(Expr::new(ExprKind::New { class: Rc::from(name_tok.text.as_str()), args }, span));
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> EResult<Expr> {
        let mut e = self.parse_primary()?;
        loop {
            let t = match self.cur() {
                Some(t) => t.clone(),
                None => break,
            };
            if t.is_punc("(") {
                let args = self.parse_args()?;
                let span = Span::new(self.file, e.span.line, e.span.col, t.end_col);
                e = Expr::new(ExprKind::Call { callee: Box::new(e), args }, span);
                continue;
            }
            if t.is_punc("[") {
                self.pos += 1;
                let index = self.parse_expr(0)?;
                let end = self.cur().map(|x| x.end_col).unwrap_or(t.end_col);
                self.expect("]", "下标要闭合")?;
                let span = Span::new(self.file, e.span.line, e.span.col, end);
                e = Expr::new(ExprKind::Idx { obj: Box::new(e), index: Box::new(index) }, span);
                continue;
            }
            if t.is_punc(".") {
                self.pos += 1;
                let name_tok = self.bump().cloned().ok_or_else(|| self.err("点号后面要跟名字"))?;
                if !name_tok.is_ident() {
                    return Err(self.err("点号后面要跟名字"));
                }
                let span = Span::new(self.file, e.span.line, e.span.col, name_tok.end_col);
                e = Expr::new(
                    ExprKind::Member { obj: Box::new(e), name: Rc::from(name_tok.text.as_str()) },
                    span,
                );
                continue;
            }
            break;
        }
        Ok(e)
    }

    fn parse_args(&mut self) -> EResult<Vec<Expr>> {
        self.expect("(", "函数调用要写括号")?;
        let mut args = Vec::new();
        if self.at(")") {
            self.pos += 1;
            return Ok(args);
        }
        loop {
            args.push(self.parse_expr(0)?);
            if self.at(",") {
                self.pos += 1;
                if self.at(")") {
                    self.pos += 1;
                    break;
                }
                continue;
            }
            self.expect(")", "参数列表要闭合")?;
            break;
        }
        Ok(args)
    }

    /// 逗号分隔的表达式列表，一直吃到行尾（print a, b 用）
    fn parse_expr_list(&mut self) -> EResult<Vec<Expr>> {
        let mut out = Vec::new();
        if self.at_end() {
            return Ok(out);
        }
        loop {
            out.push(self.parse_expr(0)?);
            if self.at(",") {
                self.pos += 1;
                if self.at_end() {
                    break;
                }
                continue;
            }
            break;
        }
        Ok(out)
    }

    fn parse_primary(&mut self) -> EResult<Expr> {
        let t = self.bump().cloned().ok_or_else(|| self.err("缺少表达式"))?;
        let span = self.span_of(&t);
        match t.kind {
            TokKind::Num => Ok(Expr::new(ExprKind::Num(t.num.unwrap_or(f64::NAN)), span)),
            TokKind::Str => Ok(Expr::new(ExprKind::Str(Rc::from(t.text.as_str())), span)),
            TokKind::Ident | TokKind::Kw => match t.text.as_str() {
                "true" => Ok(Expr::new(ExprKind::Bool(true), span)),
                "false" => Ok(Expr::new(ExprKind::Bool(false), span)),
                "null" | "nil" => Ok(Expr::new(ExprKind::Null, span)),
                "self" | "this" => Ok(Expr::new(ExprKind::SelfRef, span)),
                _ => Ok(Expr::new(ExprKind::Var(Rc::from(t.text.as_str())), span)),
            },
            TokKind::Punc => match t.text.as_str() {
                "(" => {
                    let e = self.parse_expr(0)?;
                    self.expect(")", "括号要闭合")?;
                    Ok(e)
                }
                "[" => {
                    let mut items = Vec::new();
                    if self.at("]") {
                        self.pos += 1;
                        return Ok(Expr::new(ExprKind::List(items), span));
                    }
                    loop {
                        items.push(self.parse_expr(0)?);
                        if self.at(",") {
                            self.pos += 1;
                            if self.at("]") {
                                self.pos += 1;
                                break;
                            }
                            continue;
                        }
                        self.expect("]", "列表要闭合")?;
                        break;
                    }
                    Ok(Expr::new(ExprKind::List(items), span))
                }
                "{" => {
                    let mut entries = Vec::new();
                    if self.at("}") {
                        self.pos += 1;
                        return Ok(Expr::new(ExprKind::Dict(entries), span));
                    }
                    loop {
                        let k = self.parse_expr(0)?;
                        self.expect(":", "字面量字典要写成 {\"键\": 值}")?;
                        let v = self.parse_expr(0)?;
                        entries.push((k, v));
                        if self.at(",") {
                            self.pos += 1;
                            if self.at("}") {
                                self.pos += 1;
                                break;
                            }
                            continue;
                        }
                        self.expect("}", "字典要闭合")?;
                        break;
                    }
                    Ok(Expr::new(ExprKind::Dict(entries), span))
                }
                other => Err(FloatError::new(
                    format!("这里不该出现「{}」", other),
                    span,
                )),
            },
            TokKind::Eof => Err(FloatError::new("表达式没写完", span)),
            TokKind::Op => Err(FloatError::new(
                format!("运算符「{}」前面缺少东西", t.text),
                span,
            )),
        }
    }
}

pub struct Parser {
    file: Rc<str>,
    lines: Vec<Line>,
    pos: usize,
    errors: Vec<FloatError>,
}

/// 入口：把源码解析成 Program
pub fn parse_program(file: &str, src: &str) -> Program {
    let file: Rc<str> = Rc::from(file);
    let mut p = Parser { file: Rc::clone(&file), lines: split_source(src), pos: 0, errors: Vec::new() };
    let body = p.parse_block(0);
    Program { file, body, errors: p.errors }
}

impl Parser {
    fn tokens_of(&mut self, ln: &Line, code: &str, col: u32) -> Vec<Token> {
        match tokenize_line(code, ln.no, col) {
            Ok(mut toks) => {
                toks.pop(); // 去掉 Eof
                toks
            }
            Err(e) => {
                self.errors.push(FloatError::new(
                    e.message,
                    Span::point(&self.file, e.line, e.col),
                ));
                Vec::new()
            }
        }
    }

    fn span_at(&self, ln: &Line, col: u32, end_col: u32) -> Span {
        Span::new(&self.file, ln.no, col, end_col)
    }

    fn line_span(&self, ln: &Line) -> Span {
        let col = ln.indent as u32 + 1;
        let end = ln.indent as u32 + ln.code.chars().count().max(1) as u32;
        Span::new(&self.file, ln.no, col, end)
    }

    fn expr_of(&mut self, toks: &[Token]) -> EResult<Expr> {
        ExprParser::new(toks, &self.file).parse_all()
    }

    fn to_target(&self, e: Expr) -> EResult<Target> {
        match e.kind {
            ExprKind::Var(name) => Ok(Target::Var(name)),
            ExprKind::Member { obj, name } => Ok(Target::Member(*obj, name)),
            ExprKind::Idx { obj, index } => Ok(Target::Idx(*obj, *index)),
            _ => Err(FloatError::new("这里只能给变量、成员或下标赋值", e.span)),
        }
    }

    fn parse_block(&mut self, min_indent: usize) -> Vec<Stmt> {
        let mut body = Vec::new();
        while self.pos < self.lines.len() {
            let ln = self.lines[self.pos].clone();
            if ln.skipped {
                self.pos += 1;
                continue;
            }
            if ln.indent < min_indent {
                break;
            }
            if let Some(s) = self.parse_statement() {
                body.push(s);
            }
        }
        body
    }

    fn parse_body_of(&mut self, indent: usize) -> Vec<Stmt> {
        self.parse_block(indent + 1)
    }

    fn unsupported(&mut self, ln: &Line, what: &str) -> Stmt {
        self.pos += 1;
        // 把子块吞掉，避免它被当成上层语句
        while self.pos < self.lines.len() && self.lines[self.pos].indent > ln.indent {
            self.pos += 1;
        }
        Stmt::new(StmtKind::Unsupported { what: what.to_string() }, self.line_span(ln))
    }

    fn parse_statement(&mut self) -> Option<Stmt> {
        let ln = self.lines[self.pos].clone();
        if ln.skipped {
            self.pos += 1;
            return None;
        }
        if ln.is_comment {
            self.pos += 1;
            return Some(Stmt::new(StmtKind::Nop, self.line_span(&ln)));
        }
        match self.parse_line(&ln) {
            Ok(s) => s,
            Err(e) => {
                self.errors.push(e);
                // 跳过这一行以及它的子块，恢复到能继续解析的状态
                let base = ln.indent;
                if self.pos < self.lines.len() && self.lines[self.pos].no == ln.no {
                    self.pos += 1;
                }
                while self.pos < self.lines.len() && self.lines[self.pos].indent > base {
                    self.pos += 1;
                }
                None
            }
        }
    }

    fn parse_line(&mut self, ln: &Line) -> EResult<Option<Stmt>> {
        let mut code = ln.code.as_str();
        if let Some(rest) = code.strip_prefix('}') {
            code = rest.trim();
        }
        if code.is_empty() {
            self.pos += 1;
            return Ok(None);
        }
        let toks = self.tokens_of(ln, code, ln.indent as u32 + 1);
        if toks.is_empty() {
            self.pos += 1;
            return Ok(None);
        }
        let first = toks[0].clone();
        let word = first.text.as_str();
        let is_ident = first.kind == TokKind::Ident || first.kind == TokKind::Kw;

        if is_ident {
            match word {
                "if" => return Ok(Some(self.parse_if(ln, &toks)?)),
                "elif" => {
                    return Err(FloatError::new("elif 前面没有对应的 if", self.span_at(ln, first.col, first.end_col)))
                }
                "else" => {
                    return Err(FloatError::new("else 前面没有对应的 if", self.span_at(ln, first.col, first.end_col)))
                }
                "while" => {
                    self.pos += 1;
                    let cond = self.expr_of(&toks[1..])?;
                    let body = self.parse_body_of(ln.indent);
                    return Ok(Some(Stmt::new(StmtKind::While { cond, body }, self.line_span(ln))));
                }
                "repeat" => {
                    self.pos += 1;
                    let count = self.expr_of(&toks[1..])?;
                    let body = self.parse_body_of(ln.indent);
                    return Ok(Some(Stmt::new(StmtKind::Repeat { count, body }, self.line_span(ln))));
                }
                "forever" => {
                    self.pos += 1;
                    let body = self.parse_body_of(ln.indent);
                    let cond = Expr::new(ExprKind::Bool(true), self.line_span(ln));
                    return Ok(Some(Stmt::new(StmtKind::While { cond, body }, self.line_span(ln))));
                }
                "main" if toks.get(1).map(|t| t.text == "loop").unwrap_or(false) => {
                    self.pos += 1;
                    let body = self.parse_body_of(ln.indent);
                    let cond = Expr::new(ExprKind::Bool(true), self.line_span(ln));
                    return Ok(Some(Stmt::new(StmtKind::While { cond, body }, self.line_span(ln))));
                }
                "for" => return Ok(Some(self.parse_for(ln, &toks)?)),
                "break" | "out" if toks.len() == 1 => {
                    self.pos += 1;
                    return Ok(Some(Stmt::new(StmtKind::Break, self.line_span(ln))));
                }
                "balo" | "continue" if toks.len() == 1 => {
                    self.pos += 1;
                    return Ok(Some(Stmt::new(StmtKind::Continue, self.line_span(ln))));
                }
                "stop" => {
                    // stop / stop all / stop now / stop script
                    if toks.len() == 1
                        || matches!(toks.get(1).map(|t| t.text.as_str()), Some("all") | Some("now") | Some("script"))
                    {
                        self.pos += 1;
                        return Ok(Some(Stmt::new(StmtKind::Stop, self.line_span(ln))));
                    }
                }
                "return" => {
                    self.pos += 1;
                    let rest = &toks[1..];
                    if rest.is_empty() {
                        return Ok(Some(Stmt::new(StmtKind::Return(None), self.line_span(ln))));
                    }
                    let mut ep = ExprParser::new(rest, &self.file);
                    let first_e = ep.parse_expr(0)?;
                    if ep.at(",") {
                        // return a, b → 返回列表 [a, b]
                        let mut items = vec![first_e];
                        while ep.at(",") {
                            ep.pos += 1;
                            items.push(ep.parse_expr(0)?);
                        }
                        if !ep.at_end() {
                            return Err(FloatError::new("return 后面多了内容", self.line_span(ln)));
                        }
                        let list = Expr::new(ExprKind::List(items), self.line_span(ln));
                        return Ok(Some(Stmt::new(StmtKind::Return(Some(list)), self.line_span(ln))));
                    }
                    if !ep.at_end() {
                        return Err(FloatError::new("return 后面多了内容", self.line_span(ln)));
                    }
                    return Ok(Some(Stmt::new(StmtKind::Return(Some(first_e)), self.line_span(ln))));
                }
                "报告" | "report" => {
                    self.pos += 1;
                    let mut ep = ExprParser::new(&toks[1..], &self.file);
                    let args = if ep.at("(") { ep.parse_args()? } else { ep.parse_expr_list()? };
                    if !ep.at_end() {
                        return Err(FloatError::new("报告 后面多了内容", self.line_span(ln)));
                    }
                    return Ok(Some(Stmt::new(StmtKind::Report(args), self.line_span(ln))));
                }
                "print" => {
                    self.pos += 1;
                    let mut ep = ExprParser::new(&toks[1..], &self.file);
                    let args = if ep.at("(") { ep.parse_args()? } else { ep.parse_expr_list()? };
                    if !ep.at_end() {
                        return Err(FloatError::new("print 后面多了内容", self.line_span(ln)));
                    }
                    return Ok(Some(Stmt::new(StmtKind::Print(args), self.line_span(ln))));
                }
                "class" => return Ok(Some(self.unsupported(ln, "class（类定义）这版 VM 还没实现"))),
                "try" => return Ok(Some(self.unsupported(ln, "try / catch 这版 VM 还没实现"))),
                "throw" => return Ok(Some(self.unsupported(ln, "throw 这版 VM 还没实现"))),
                "import" => return Ok(Some(self.unsupported(ln, "import（模块）这版 VM 还没实现"))),
                "ask" => {
                    self.pos += 1;
                    let mut ep = ExprParser::new(&toks[1..], &self.file);
                    let prompt = if ep.at("(") {
                        match ep.parse_args() {
                            Ok(mut a) => a.pop(),
                            Err(e) => return Err(e),
                        }
                    } else if ep.at_end() {
                        None
                    } else {
                        Some(ep.parse_expr(0)?)
                    };
                    if !ep.at_end() {
                        return Err(FloatError::new("ask 后面多了内容", self.line_span(ln)));
                    }
                    return Ok(Some(Stmt::new(StmtKind::Ask(prompt), self.line_span(ln))));
                }
                "wait" => return Ok(Some(self.unsupported(ln, "wait（挂起）这版 VM 还没实现"))),
                "time" => return Ok(Some(self.unsupported(ln, "time read / time reset 这版 VM 还没实现"))),
                _ => {}
            }

            if FUNC_WORDS.contains(&word) && (word != "f" || toks.get(1).map(|t| t.is_ident()).unwrap_or(false)) {
                return Ok(Some(self.parse_func_def(ln, &toks)?));
            }
        }

        // 解构赋值：a, b = f()
        if is_ident {
            let mut k = 0usize;
            let mut names: Vec<Rc<str>> = Vec::new();
            let mut ok = true;
            loop {
                if let Some(t) = toks.get(k) {
                    if t.is_ident() && TYPE_WORDS.contains(&t.text.as_str()) && toks.get(k + 1).map(|x| x.is_ident()).unwrap_or(false) {
                        k += 1;
                    }
                }
                match toks.get(k) {
                    Some(t) if t.is_ident() => {
                        names.push(Rc::from(t.text.as_str()));
                        k += 1;
                    }
                    _ => {
                        ok = false;
                        break;
                    }
                }
                if toks.get(k).map(|t| t.text == ",").unwrap_or(false) {
                    k += 1;
                    continue;
                }
                break;
            }
            if ok && names.len() >= 2 && toks.get(k).map(|t| t.text == "=").unwrap_or(false) {
                let mut ep = ExprParser::new(&toks[k + 1..], &self.file);
                let value = ep.parse_expr(0)?;
                if !ep.at_end() {
                    return Err(FloatError::new("解构赋值右边多了内容", self.line_span(ln)));
                }
                self.pos += 1;
                return Ok(Some(Stmt::new(StmtKind::Destructure { names, value }, self.line_span(ln))));
            }
        }

        // 带类型的变量声明：int a = 1
        if is_ident && TYPE_WORDS.contains(&word) {
            if let Some(name_tok) = toks.get(1) {
                if name_tok.is_ident() {
                    let mut ep = ExprParser::new(&toks[2..], &self.file);
                    let mut init = None;
                    if ep.at("=") {
                        ep.pos += 1;
                        init = Some(ep.parse_expr(0)?);
                    } else if !ep.at_end() {
                        return Err(FloatError::new(
                            format!("变量声明「{} {}」后面多出了内容", word, name_tok.text),
                            self.line_span(ln),
                        ));
                    }
                    self.pos += 1;
                    let ty = TypeName::from_word(word).unwrap_or(TypeName::Var);
                    return Ok(Some(Stmt::new(
                        StmtKind::Decl { ty, name: Rc::from(name_tok.text.as_str()), init },
                        self.line_span(ln),
                    )));
                }
            }
        }

        // 类类型声明：Person p = new Person(…)，类型只作标注
        if is_ident && toks.len() >= 4 && toks[1].is_ident() && toks[2].text == "=" {
            let mut ep = ExprParser::new(&toks[3..], &self.file);
            let init = ep.parse_expr(0)?;
            if !ep.at_end() {
                return Err(FloatError::new("声明语句右边多了内容", self.line_span(ln)));
            }
            self.pos += 1;
            return Ok(Some(Stmt::new(
                StmtKind::Decl { ty: TypeName::Var, name: Rc::from(toks[1].text.as_str()), init: Some(init) },
                self.line_span(ln),
            )));
        }

        // 赋值 / 自增自减 / 表达式语句
        let mut ep = ExprParser::new(&toks, &self.file);
        let first_expr = ep.parse_expr(0)?;
        if !ep.at_end() {
            if let Some(t) = ep.cur() {
                if t.kind == TokKind::Op && ASSIGN_OPS.contains(&t.text.as_str()) {
                    let op_text = t.text.clone();
                    ep.pos += 1;
                    let value = ep.parse_expr(0)?;
                    if !ep.at_end() {
                        return Err(FloatError::new("赋值号右边多了内容", self.line_span(ln)));
                    }
                    self.pos += 1;
                    let target = self.to_target(first_expr)?;
                    return Ok(Some(Stmt::new(
                        StmtKind::Assign { target, op: AssignOp::from_str(&op_text).unwrap(), value },
                        self.line_span(ln),
                    )));
                }
                if t.text == "++" || t.text == "--" {
                    let delta = if t.text == "++" { 1 } else { -1 };
                    ep.pos += 1;
                    if !ep.at_end() {
                        return Err(FloatError::new("自增自减后面不能再有内容", self.line_span(ln)));
                    }
                    self.pos += 1;
                    let target = self.to_target(first_expr)?;
                    return Ok(Some(Stmt::new(
                        StmtKind::IncDec { target, delta, prefix: false },
                        self.line_span(ln),
                    )));
                }
            }
            return Err(FloatError::new(
                format!("这条语句看不懂：{}", ep.describe()),
                self.line_span(ln),
            ));
        }

        self.pos += 1;
        Ok(Some(Stmt::new(StmtKind::ExprStmt(first_expr), self.line_span(ln))))
    }

    fn parse_if(&mut self, ln: &Line, toks: &[Token]) -> EResult<Stmt> {
        self.pos += 1;
        let mut rest = &toks[1..];
        let mut is_if_out = false;
        if rest.first().map(|t| t.text == "out").unwrap_or(false) {
            is_if_out = true;
            rest = &rest[1..];
        }
        let cond = if rest.is_empty() { None } else { Some(self.expr_of(rest)?) };
        if is_if_out {
            let _cond = cond.ok_or_else(|| FloatError::new("if out 需要一个条件", self.line_span(ln)))?;
            while self.pos < self.lines.len() && self.lines[self.pos].indent > ln.indent {
                self.pos += 1;
            }
            return Ok(Stmt::new(StmtKind::Unsupported { what: "if out（条件跳出）这版 VM 还没实现".into() }, self.line_span(ln)));
        }
        let cond = cond.ok_or_else(|| FloatError::new("if 后面要跟条件，例如 if (a > 1)", self.line_span(ln)))?;
        let mut branches = vec![(Some(cond), self.parse_body_of(ln.indent))];
        let mut else_body = None;
        self.merge_branches(&mut branches, &mut else_body, ln.indent);
        Ok(Stmt::new(StmtKind::If { branches, else_body }, self.line_span(ln)))
    }

    /// 把 `} else {` / `} elif (…) {` 合并回上一个 if
    fn merge_branches(
        &mut self,
        branches: &mut Vec<(Option<Expr>, Vec<Stmt>)>,
        else_body: &mut Option<Vec<Stmt>>,
        indent: usize,
    ) {
        loop {
            let ln = match self.lines.get(self.pos) {
                Some(l) => l.clone(),
                None => return,
            };
            if ln.skipped {
                return;
            }
            let mut code = ln.code.clone();
            let mut brace_close = false;
            if let Some(rest) = code.strip_prefix('}') {
                brace_close = true;
                code = rest.trim().to_string();
            }
            if code.is_empty() {
                return;
            }
            let is_elif = code.starts_with("elif");
            let is_else = code.starts_with("else");
            if !is_elif && !is_else {
                return;
            }
            let same_indent = ln.indent == indent || (brace_close && ln.indent <= indent + 1);
            if !same_indent {
                return;
            }
            self.pos += 1;
            if is_else {
                *else_body = Some(self.parse_body_of(ln.indent));
                return;
            }
            let cond_src = code.trim_start_matches("elif").trim();
            let cond_src = strip_parens(cond_src);
            let cond = if cond_src.is_empty() {
                None
            } else {
                let col = ln.indent as u32 + 1 + code.find("elif").unwrap_or(0) as u32 + 4;
                let toks = self.tokens_of(&ln, &cond_src, col);
                match ExprParser::new(&toks, &self.file).parse_all() {
                    Ok(e) => Some(e),
                    Err(e) => {
                        self.errors.push(e);
                        return;
                    }
                }
            };
            let body = self.parse_body_of(ln.indent);
            branches.push((cond, body));
        }
    }

    fn parse_for(&mut self, ln: &Line, toks: &[Token]) -> EResult<Stmt> {
        self.pos += 1;
        let var_tok = toks.get(1).cloned().ok_or_else(|| FloatError::new("for 后面要跟循环变量名", self.line_span(ln)))?;
        if !var_tok.is_ident() {
            return Err(FloatError::new("for 后面要跟循环变量名", self.line_span(ln)));
        }
        let mut value_name: Option<Rc<str>> = None;
        let mut cursor = 2usize;
        if toks.get(2).map(|t| t.text == ",").unwrap_or(false) {
            let v2 = toks.get(3).cloned().ok_or_else(|| FloatError::new("for a, b 里的第二项要是变量名", self.line_span(ln)))?;
            if !v2.is_ident() {
                return Err(FloatError::new("for a, b 里的第二项要是变量名", self.line_span(ln)));
            }
            value_name = Some(Rc::from(v2.text.as_str()));
            cursor = 4;
        }

        // for i = 1 to 10 [step 2]
        if toks.get(cursor).map(|t| t.text == "=").unwrap_or(false) {
            let mut ep = ExprParser::new(&toks[cursor + 1..], &self.file);
            let from = ep.parse_expr(0)?;
            if !ep.at("to") {
                return Err(FloatError::new("计数循环要写成「for i = 1 to 10」", self.line_span(ln)));
            }
            ep.pos += 1;
            let to = ep.parse_expr(0)?;
            let mut step = None;
            if !ep.at_end() {
                if !ep.at("step") {
                    return Err(FloatError::new("这里只能写 step 步长", self.line_span(ln)));
                }
                ep.pos += 1;
                step = Some(ep.parse_expr(0)?);
            }
            let body = self.parse_body_of(ln.indent);
            return Ok(Stmt::new(
                StmtKind::ForRange {
                    var: Rc::from(var_tok.text.as_str()),
                    from: Some(from),
                    to,
                    step,
                    inclusive: true,
                    body,
                },
                self.line_span(ln),
            ));
        }

        if !toks.get(cursor).map(|t| t.text == "in").unwrap_or(false) {
            return Err(FloatError::new(
                "for 的写法：「for i in 集合」/「for i in range(n)」/「for i = 1 to 10」",
                self.line_span(ln),
            ));
        }
        let mut ep = ExprParser::new(&toks[cursor + 1..], &self.file);
        let iter = ep.parse_expr(0)?;

        // range(n) 特化成计数循环，省掉建列表
        if let ExprKind::Call { callee, args } = &iter.kind {
            if let ExprKind::Var(name) = &callee.kind {
                if &**name == "range" {
                    if args.is_empty() {
                        return Err(FloatError::new("range 需要至少一个参数", self.line_span(ln)));
                    }
                    if args.len() > 3 {
                        return Err(FloatError::new("range 最多接受三个参数（起点, 终点, 步长）", self.line_span(ln)));
                    }
                    let from = if args.len() >= 2 { Some(args[0].clone()) } else { None };
                    let to = if args.len() >= 2 { args[1].clone() } else { args[0].clone() };
                    let step = if args.len() >= 3 { Some(args[2].clone()) } else { None };
                    let body = self.parse_body_of(ln.indent);
                    return Ok(Stmt::new(
                        StmtKind::ForRange {
                            var: Rc::from(var_tok.text.as_str()),
                            from,
                            to,
                            step,
                            inclusive: false,
                            body,
                        },
                        self.line_span(ln),
                    ));
                }
            }
        }

        let body = self.parse_body_of(ln.indent);
        Ok(Stmt::new(
            StmtKind::ForIn { var: Rc::from(var_tok.text.as_str()), value_name, iter, body },
            self.line_span(ln),
        ))
    }

    fn parse_func_def(&mut self, ln: &Line, toks: &[Token]) -> EResult<Stmt> {
        self.pos += 1;
        let name_tok = toks.get(1).cloned().ok_or_else(|| FloatError::new("function 后面要跟函数名", self.line_span(ln)))?;
        if !name_tok.is_ident() && name_tok.kind != TokKind::Kw {
            return Err(FloatError::new("function 后面要跟函数名", self.line_span(ln)));
        }
        let mut ep = ExprParser::new(&toks[2..], &self.file);
        let mut params: Vec<Param> = Vec::new();
        if ep.at("(") {
            ep.pos += 1;
            if !ep.at(")") {
                loop {
                    // 参数可带类型：function f(int a, str b = "x")
                    let mut ty: Option<TypeName> = None;
                    if let Some(t) = ep.cur() {
                        if t.is_ident() && TYPE_WORDS.contains(&t.text.as_str()) {
                            ty = TypeName::from_word(&t.text);
                            ep.pos += 1;
                        }
                    }
                    let nt = ep.bump().cloned().ok_or_else(|| FloatError::new("参数列表没写完", self.line_span(ln)))?;
                    if !nt.is_ident() {
                        return Err(FloatError::new("参数名不能是关键字", self.line_span(ln)));
                    }
                    let mut default = None;
                    if ep.at("=") {
                        ep.pos += 1;
                        default = Some(ep.parse_expr(0)?);
                    }
                    params.push(Param {
                        name: Rc::from(nt.text.as_str()),
                        ty,
                        default,
                        line: nt.line,
                        col: nt.col,
                    });
                    if ep.at(",") {
                        ep.pos += 1;
                        if ep.at(")") {
                            break;
                        }
                        continue;
                    }
                    break;
                }
            }
            ep.expect(")", "参数列表要闭合")?;
        }
        if !ep.at_end() {
            return Err(FloatError::new("函数签名后面多了内容", self.line_span(ln)));
        }
        let body = self.parse_body_of(ln.indent);
        Ok(Stmt::new(
            StmtKind::FuncDef { name: Rc::from(name_tok.text.as_str()), params, ret: None, body },
            self.line_span(ln),
        ))
    }
}

fn strip_parens(s: &str) -> String {
    let t = s.trim();
    let t = t.strip_suffix('{').map(|x| x.trim_end()).unwrap_or(t).trim();
    if t.starts_with('(') && t.ends_with(')') && t.len() >= 2 {
        t[1..t.len() - 1].trim().to_string()
    } else {
        t.to_string()
    }
}
