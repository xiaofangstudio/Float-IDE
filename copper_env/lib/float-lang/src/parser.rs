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

const TYPE_WORDS: [&str; 9] = ["int", "float", "str", "bool", "list", "dict", "ptr", "addr", "var"];
const FUNC_WORDS: [&str; 5] = ["function", "fn", "Function", "func", "f"];
const ASSIGN_OPS: [&str; 7] = ["=", "+=", "-=", "*=", "/=", "%=", "**="];

#[derive(Clone, Debug)]
#[allow(dead_code)]
struct Line {
    no: u32,
    indent: usize,
    /// 用 Rc<String>：解析时每行会被克隆好几次（语句、块、类体），
    /// 用 String 就是每行复制源码；Rc 只是一个引用计数。
    /// 跨行合并用 Rc::make_mut 原地追加，仍然是 O(n)。
    code: Rc<String>,
    is_comment: bool,
    skipped: bool,
    /// 这一行（含续行）里还有多少括号没闭合；增量维护，避免 O(n²)
    open_depth: i32,
}

/// 把源码切成"有效行"：去 BOM、统一换行、Tab 展开成 4 空格、剥掉 `--` 注释。
fn split_source(src: &str) -> (Vec<Line>, Option<(u32, String)>) {
    // 老式 `--` 注释撞上时的报错要带行号，但这里没有 Parser（拿不到 file），
    // 所以先记下来，由 parse_program 落成正式错误。
    let mut lex_err: Option<(u32, String)> = None;
    let text = src.trim_start_matches('\u{feff}').replace("\r\n", "\n").replace('\r', "\n");
    let mut out: Vec<Line> = Vec::new();
    for (idx, raw_line) in text.split('\n').enumerate() {
        let expanded: String = raw_line.replace('\t', "    ");
        let indent = expanded.chars().take_while(|c| *c == ' ').count();
        let trimmed = expanded.trim();
        if trimmed.is_empty() {
            continue;
        }
        let no = idx as u32 + 1;
        // 注释只有一种写法：`/*` 到行尾（不用闭合）。老写法 `--` 在 code_part 里报错。
        // `//` 不是注释，它是整除运算符。
        let (code, comment) = match code_part(trimmed) {
            Ok(x) => x,
            Err(m) => {
                lex_err.get_or_insert((no, m));
                (trimmed.to_string(), None)
            }
        };
        let is_comment = code.trim().is_empty();
        // 行尾分号忽略（core.json 承诺「行尾 ; 可选」）：`print(3);` 合法，
        // 但一行多句 `print(1); print(2)` 不支持——中间的分号不在行尾，照旧报错。
        let code_trimmed = code.trim().trim_end_matches(|c: char| c == ';' || c.is_whitespace())
            .trim_end().to_string();
        let c = if is_comment { String::new() } else { code_trimmed };
        let cleaned = if is_comment {
            String::new()
        } else {
            // 行尾的 { 只在「块开始」时删掉（`if x {` / `fn f() {`）。
            // `dict cfg = {` / `print({` / `"键": {` 这种是字典字面量开头，
            // 必须留着：下面的 bracket_depth 会算出 1，续行合并自动生效。
            // 以前无条件删，导致字典只能挤在一行里写完。
            if is_block_brace(&c) {
                c.strip_suffix('{').map(|s| s.trim_end()).unwrap_or(&c).trim().to_string()
            } else {
                c.trim().to_string()
            }
        };
        let _ = comment;
        let line_depth = bracket_depth(&cleaned);
        /* 多行字面量：上一行括号还没闭合，就把它接到上一行后面。
         * 关键：**增量维护**累积行的括号深度，不要每次把整段重扫一遍——
         * 5000 行的地图表那样写就是 O(n²)，实测 1.8 秒。这里是 O(n)。 */
        if let Some(last) = out.last_mut() {
            if last.open_depth > 0 {
                last.open_depth += bracket_depth(&cleaned);
                let merged = Rc::make_mut(&mut last.code);
                merged.push(' ');
                merged.push_str(&cleaned);
                continue;
            }
        }
        out.push(Line {
            no,
            indent,
            code: Rc::new(cleaned),
            is_comment,
            skipped: false,
            open_depth: line_depth,
        });
    }
    (out, lex_err)
}

/// 行尾的 `{` 是「块开始」还是「字典字面量开头」？
/// 前面还在等一个值（= ( [ , : ? 或运算符、return 之类）→ 字典，要留着；
/// 前面已经是一个完整表达式（if x / for i in xs / fn f()）→ 块开始，删掉。
fn is_block_brace(code: &str) -> bool {
    let t = code.trim_end();
    if !t.ends_with('{') {
        return false;
    }
    let before = t[..t.len() - 1].trim_end();
    if before.is_empty() {
        return true;
    }
    let last = before.chars().last().unwrap_or(' ');
    if "=([,:?+-*/%&|<>!".contains(last) {
        return false;
    }
    // `f{…}` 是拼接字面量，不是块开始：f 必须是紧贴 { 的独立标识符
    if last == 'f' {
        let mut it = before.chars();
        it.next_back();
        let prev = it.next_back();
        let ident_part = |c: char| c.is_alphanumeric() || c == '_' || (c >= '\u{4e00}' && c <= '\u{9fa5}');
        if prev.map(|c| !ident_part(c)).unwrap_or(true) {
            return false;
        }
    }
    if let Some(word) = before.split_whitespace().last() {
        let w = word.to_ascii_lowercase();
        if matches!(w.as_str(), "return" | "in" | "to" | "into" | "and" | "or" | "not") {
            return false;
        }
    }
    true
}

/// 头部行内联花括号体检查：`if (a) { print(1) }` 这种把块体挤在头部的写法
/// 会被静默丢掉（或错位报成下一行"缩进不够"），这里提前拦住指路。
/// 不算内联体的：字典字面量（内容有 `:`，或只有裸键/逗号）、`f{…}` 拼接、
/// 括号里的 `{`、行尾单独开块的 `{`（那个在 split_source 已经剥掉了）。
/// 返回 Some(列号) 表示发现了内联花括号体。与 parse.ts 的 findInlineBraceBody 对齐。
fn find_inline_brace_body(code: &str) -> Option<usize> {
    let chars: Vec<char> = code.chars().collect();
    let ident_part = |c: char| c.is_alphanumeric() || c == '_' || (c >= '\u{4e00}' && c <= '\u{9fa5}');
    let mut in_str: Option<char> = None;
    let mut depth = 0i32;
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if let Some(q) = in_str {
            if c == '\\' { i += 2; continue; }
            if c == q { in_str = None; }
            i += 1;
            continue;
        }
        if c == '"' || c == '\'' { in_str = Some(c); i += 1; continue; }
        if c == '(' || c == '[' { depth += 1; i += 1; continue; }
        if c == ')' || c == ']' { depth -= 1; i += 1; continue; }
        if c != '{' || depth > 0 { i += 1; continue; }
        // f{ 拼接：f 紧贴 {
        let mut before_end = i;
        while before_end > 0 && chars[before_end - 1].is_whitespace() { before_end -= 1; }
        if before_end > 0 && chars[before_end - 1] == 'f' {
            let prev_ok = before_end == 1 || (before_end >= 2 && !ident_part(chars[before_end - 2]));
            if prev_ok { i += 1; continue; }
        }
        // 找配对的 }，顺带判断是不是字典（有 `:` 或只有裸键/逗号）
        let mut d2 = 0i32;
        let mut s2: Option<char> = None;
        let mut has_colon = false;
        let mut has_code = false;
        let mut bare_keys = true;
        let mut j = i + 1;
        while j < chars.len() {
            let c2 = chars[j];
            if let Some(q) = s2 {
                if c2 == '\\' { j += 2; continue; }
                if c2 == q { s2 = None; }
                j += 1;
                continue;
            }
            if c2 == '"' || c2 == '\'' { s2 = Some(c2); bare_keys = false; j += 1; continue; }
            if c2 == '{' { d2 += 1; bare_keys = false; j += 1; continue; }
            if c2 == '}' {
                if d2 == 0 { break; }
                d2 -= 1;
                j += 1;
                continue;
            }
            if d2 == 0 {
                if c2 == ':' { has_colon = true; }
                if !c2.is_whitespace() { has_code = true; }
                if !(c2.is_alphanumeric() || c2 == '_' || c2 == ',' || (c2 >= '\u{4e00}' && c2 <= '\u{9fa5}')) { bare_keys = false; }
            }
            j += 1;
        }
        if j >= chars.len() { i += 1; continue; } // 没配对：不是同行内联体
        if has_colon || bare_keys { i += 1; continue; } // 字典字面量（有键值分隔，或只有裸键/逗号）
        if has_code {
            return Some(code.chars().take(i).map(|c| c.len_utf8()).sum::<usize>() + 1);
        }
        i += 1;
    }
    None
}

/// 数一行里"没闭合的括号"（字符串里的不算）
fn bracket_depth(code: &str) -> i32 {
    let mut d = 0i32;
    let mut in_str: Option<char> = None;
    let mut esc = false;
    for c in code.chars() {
        if let Some(q) = in_str {
            if esc { esc = false; }
            else if c == '\\' { esc = true; }
            else if c == q { in_str = None; }
            continue;
        }
        match c {
            '"' | '\'' => in_str = Some(c),
            '(' | '[' | '{' => d += 1,
            ')' | ']' | '}' => d -= 1,
            _ => {}
        }
    }
    d
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
        "*" | "/" | "%" | "//" => 60,
        "**" => 70,
        _ => return None,
    })
}

struct ExprParser<'a> {
    toks: &'a [Token],
    pos: usize,
    file: &'a Rc<str>,
    /// 表达式嵌套深度（括号 / 调用 / 列表都算），防栈溢出
    depth: usize,
}

type EResult<T> = Result<T, FloatError>;

impl<'a> ExprParser<'a> {
    fn new(toks: &'a [Token], file: &'a Rc<str>) -> ExprParser<'a> {
        ExprParser { toks, pos: 0, file, depth: 0 }
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

    /// 当前 token 是裸标识符（含关键字形的 Ident）时给出它的文本。
    /// 数字、字符串、标点都不算。
    fn cur_ident(&self) -> Option<String> {
        match self.cur() {
            Some(t) if matches!(t.kind, TokKind::Ident | TokKind::Kw) => Some(t.text.to_string()),
            _ => None,
        }
    }

    /// 下一个 token 是不是给定文本（不消费）。
    fn peek_is(&self, text: &str) -> bool {
        self.toks
            .get(self.pos + 1)
            .map(|t| t.text == text)
            .unwrap_or(false)
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
        // 嵌套保护：括号 / 调用 / 列表都会递归进来，几千层会把栈打穿
        self.depth += 1;
        if self.depth > MAX_EXPR_DEPTH {
            self.depth -= 1;
            return Err(self.err(format!(
                "表达式嵌套太深（超过 {} 层）停下来了。拆成几个变量或几行会更清楚",
                MAX_EXPR_DEPTH
            )));
        }
        let r = self.parse_expr_inner(min_prec);
        self.depth -= 1;
        r
    }

    fn parse_expr_inner(&mut self, min_prec: u8) -> EResult<Expr> {
        let left0 = self.parse_unary()?;
        // 先把这一层的左结合链收集起来（不立刻折叠），再决定怎么折叠
        let mut chain: Vec<(Op, Expr, Span)> = Vec::new();
        loop {
            let (op_text, tok) = match self.cur() {
                Some(t) if t.kind == TokKind::Op => (t.text.clone(), t.clone()),
                // `a mod b`：mod 只在中缀位置当取余；mod(a,b) 与汇编式 mod a b 不受影响
                Some(t) if t.kind == TokKind::Ident && t.text == "mod" => ("%".to_string(), t.clone()),
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
            chain.push((op, right, span));
        }
        let mut left = left0;
        if !chain.is_empty() {
            left = self.fold_chain(left, chain)?;
        }
        // 三元：`条件 ? 这个 : 那个`，优先级最低、右结合
        // （只在最外层解析，避免把 dict 里的冒号吃掉）
        if min_prec == 0 && self.at("?") {
            self.pos += 1;
            let a = self.parse_expr(0)?;
            if !self.at(":") {
                return Err(self.err("三元的冒号丢了：写成 条件 ? 这个 : 那个"));
            }
            self.pos += 1;
            let b = self.parse_expr(0)?;
            let span = Span::new(self.file, left.span.line, left.span.col, b.span.end_col);
            left = Expr::new(
                ExprKind::Ternary { cond: Box::new(left), a: Box::new(a), b: Box::new(b) },
                span,
            );
        }
        Ok(left)
    }

    /// 折叠左结合链。要点：
    /// - 普通长度：按左结合叠成一棵左深树（保持原语义）
    /// - 很长且**运算符全是同一个结合律运算符**（+ × 位运算 && ||）：改成一棵平衡树，
    ///   深度从 n 降到 log₂n —— 这样几万项的拼接也不会把递归/析构的栈打穿，
    ///   结果内容完全一样（加法/拼接本来就满足结合律）
    /// - 很长但不是结合律（一连串 - 或 /）：明确报错，不硬撑
    fn fold_chain(&mut self, left: Expr, chain: Vec<(Op, Expr, Span)>) -> EResult<Expr> {
        const BALANCE_OVER: usize = 16;
        /// 非结合运算（一连串 - 或 /）最多这么长；再长就报错，因为没法平衡化，
        /// 左深树太深会把编译/析构的栈打穿
        const MAX_CHAIN: usize = 2000;

        let first_op = chain[0].0;
        let associative = matches!(
            first_op,
            Op::Add | Op::Mul | Op::BitAnd | Op::BitOr | Op::BitXor | Op::And | Op::Or
        );
        let same_op = chain.iter().all(|(o, _, _)| *o == first_op);

        if chain.len() > MAX_CHAIN && !(same_op && associative) {
            return Err(self.err(format!(
                "这一行的算式太长（超过 {} 个运算符，而且混了加减乘除）停下来了。拆成几行，或者用列表 + join 拼",
                MAX_CHAIN
            )));
        }

        if chain.len() > BALANCE_OVER && same_op && associative {
            let op = first_op;
            let mut level: Vec<Expr> = Vec::with_capacity(chain.len() + 1);
            level.push(left);
            for (_, e, _) in chain {
                level.push(e);
            }
            // 一轮轮两两合并，天然是平衡树；顺序仍是原顺序（短路语义不变）
            while level.len() > 1 {
                let mut next: Vec<Expr> = Vec::with_capacity(level.len() / 2 + 1);
                let mut it = level.into_iter();
                while let Some(a) = it.next() {
                    match it.next() {
                        Some(b) => {
                            let span = Span::new(self.file, a.span.line, a.span.col, b.span.end_col);
                            next.push(Expr::new(
                                ExprKind::Bin { op, a: Box::new(a), b: Box::new(b) },
                                span,
                            ));
                        }
                        None => next.push(a),
                    }
                }
                level = next;
            }
            return Ok(level.pop().expect("至少一个操作数"));
        }

        // 普通左结合折叠
        let mut acc = left;
        for (op, right, span) in chain {
            acc = Expr::new(ExprKind::Bin { op, a: Box::new(acc), b: Box::new(right) }, span);
        }
        Ok(acc)
    }

    fn parse_unary(&mut self) -> EResult<Expr> {
        let t = match self.cur() {
            Some(t) => t.clone(),
            None => return Err(self.err("缺少表达式")),
        };
        if t.kind == TokKind::Op {
            // 前缀位置的 ** 是两次解引用：幂运算不会出现在操作数位置
            if t.text == "**" {
                self.pos += 1;
                let a = self.parse_unary()?;
                let span = Span::new(self.file, t.line, t.col, a.span.end_col);
                let inner = Expr::new(
                    ExprKind::Un { op: Op::Deref, a: Box::new(a) },
                    span.clone(),
                );
                return Ok(Expr::new(
                    ExprKind::Un { op: Op::Deref, a: Box::new(inner) },
                    span,
                ));
            }
            let op = match t.text.as_str() {
                "-" => Some(Op::Neg),
                "!" => Some(Op::Not),
                "~" => Some(Op::BitNot),
                // &取地址、*解引用：只在操作数位置出现，不影响二元 & / *
                "&" => Some(Op::Addr),
                "*" => Some(Op::Deref),
                _ => None,
            };
            if let Some(op) = op {
                self.pos += 1;
                let a = self.parse_unary()?;
                let span = Span::new(self.file, t.line, t.col, a.span.end_col);
                // `-9223372036854775808`：正字面量 2^63 存不进 i64，只能走 f64；
                // 取负之后正好是 i64::MIN，这里折成精确整数，别让它变成浮点。
                if op == Op::Neg {
                    if let ExprKind::Num(f) = a.kind {
                        if f == 9_223_372_036_854_775_808.0 {
                            return Ok(Expr::new(ExprKind::Int(i64::MIN), span));
                        }
                    }
                }
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
            let e = Expr::new(ExprKind::New { class: Rc::from(name_tok.text.as_str()), args }, span);
            // new C().get() / new C()[0]：实例化之后还能继续接后缀
            return self.parse_postfix_from(e);
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> EResult<Expr> {
        let e = self.parse_primary()?;
        self.parse_postfix_from(e)
    }

    /// 从一个已有的表达式继续吃后缀（.成员 / 调用 / 下标）
    fn parse_postfix_from(&mut self, mut e: Expr) -> EResult<Expr> {
        loop {
            let t = match self.cur() {
                Some(t) => t.clone(),
                None => break,
            };
            if t.is_punc("(") {
                let args = self.parse_args()?;
                let span = Span::new(self.file, e.span.line, e.span.col, t.end_col);
                /* 指针新写法：全是词，没有 & 和 * 那些符号。
                 *   addr(x)      取变量地址
                 *   addr(xs, i)  取列表第 i 格的地址
                 *   read(p)      读地址里的值
                 * 都在解析期改写成原有的取地址/解引用节点，
                 * 所以 VM、AOT、TS 引擎不用各写一份。 */
                let rewritten = match &e.kind {
                    ExprKind::Var(name) => match (&**name, args.len()) {
                        ("addr", 1) => Some(Expr::new(
                            ExprKind::Un { op: Op::Addr, a: Box::new(args[0].clone()) },
                            span.clone(),
                        )),
                        ("addr", 2) => Some(Expr::new(
                            ExprKind::Un {
                                op: Op::Addr,
                                a: Box::new(Expr::new(
                                    ExprKind::Idx {
                                        obj: Box::new(args[0].clone()),
                                        index: Box::new(args[1].clone()),
                                    },
                                    span.clone(),
                                )),
                            },
                            span.clone(),
                        )),
                        ("read", 1) => Some(Expr::new(
                            ExprKind::Un { op: Op::Deref, a: Box::new(args[0].clone()) },
                            span.clone(),
                        )),
                        _ => None,
                    },
                    _ => None,
                };
                if let Some(x) = rewritten {
                    e = x;
                    continue;
                }
                e = Expr::new(ExprKind::Call { callee: Box::new(e), args }, span);
                continue;
            }
            if t.is_punc("[") {
                self.pos += 1;
                // 切片：xs[1:3] / s[:2] / xs[2:]——含头不含尾，负数从末尾数
                let mut from: Option<Box<Expr>> = None;
                let mut to: Option<Box<Expr>> = None;
                let mut is_slice = false;
                if !self.at(":") && !self.at("]") {
                    from = Some(Box::new(self.parse_expr(0)?));
                }
                if self.at(":") {
                    is_slice = true;
                    self.pos += 1;
                    if !self.at("]") {
                        to = Some(Box::new(self.parse_expr(0)?));
                    }
                } else if from.is_none() {
                    return Err(FloatError::new("下标里要有一个值", e.span.clone()));
                }
                let end = self.cur().map(|x| x.end_col).unwrap_or(t.end_col);
                self.expect("]", "下标要闭合")?;
                let span = Span::new(self.file, e.span.line, e.span.col, end);
                e = if is_slice {
                    Expr::new(ExprKind::Slice { obj: Box::new(e), from, to }, span)
                } else {
                    Expr::new(ExprKind::Idx { obj: Box::new(e), index: from.unwrap() }, span)
                };
                continue;
            }
            if t.is_punc(".") {
                self.pos += 1;
                let name_tok = self.bump().cloned().ok_or_else(|| self.err("点号后面要跟名字"))?;
                if !name_tok.is_ident() {
                    return Err(self.err("点号后面要跟名字"));
                }
                let span = Span::new(self.file, e.span.line, e.span.col, name_tok.end_col);
                /* 标准库：`os.readfile(x)` 里的 os 是命名空间，不是变量。
                 * 编译期直接收敛成一次内建调用（内建名就叫 "os.readfile"），
                 * 这样 VM / 解释器 / AOT 三处都不用为命名空间加新东西。 */
                /* 解析期不知道 `data` 是标准库还是用户变量：`str data = "a,b"` 之后
                 * 写 `data.split(",")` 是字符串方法。所以只在名字确实是标准库函数时
                 * 收敛成内建名，其余退回普通成员调用，运行期按实际值分派。 */
                if let ExprKind::Var(base) = &e.kind {
                    if crate::stdlib::is_namespace(base) {
                        let full = format!("{}.{}", base, name_tok.text);
                        if crate::stdlib::is_fn(&full) {
                            e = Expr::new(ExprKind::Var(Rc::from(full.as_str())), span);
                            continue;
                        }
                    }
                }
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

    /// 下一个 token 能不能当一个操作数（汇编式算术靠这个判断"还有没有下一个数"）
    fn operand_ahead(&self) -> bool {
        match self.toks.get(self.pos) {
            Some(t) => match t.kind {
                TokKind::Num | TokKind::Str | TokKind::Ident | TokKind::Kw => true,
                TokKind::Punc => matches!(t.text.as_str(), "(" | "[" | "{"),
                // 加减号不算操作数开头：它们留给外层的运算符，
                // 不然 `add a b + 10` / `mul 2 3 - 1` 会被吃成 add(a,b,+10)、mul(2,3,-1)
                TokKind::Op => matches!(t.text.as_str(), "!" | "~"),
                TokKind::Eof => false,
            },
            None => false,
        }
    }

    /// 字符串字面量 → 支持 `{表达式}` 插值，编译成一串字符串拼接。
    /// 和 TS 兼容引擎完全一致：
    /// * `"值是 {x} 结束"` → `"值是 " + x + " 结束"`
    /// * `\{` `\}` 是转义，当普通花括号
    /// * 插值里写错了就当普通文本（不在这里报错，交给外面的检查）
    fn string_expr(&mut self, text: String, span: Span) -> Expr {
        if !text.contains('{') {
            return Expr::new(ExprKind::Str(Rc::from(text.as_str())), span);
        }
        let chars: Vec<char> = text.chars().collect();
        let mut parts: Vec<Expr> = Vec::new();
        let mut buf = String::new();
        let mut i = 0usize;
        while i < chars.len() {
            let c = chars[i];
            // \{ \} 是转义
            if c == '\\' && i + 1 < chars.len() && (chars[i + 1] == '{' || chars[i + 1] == '}') {
                buf.push(chars[i + 1]);
                i += 2;
                continue;
            }
            if c != '{' {
                buf.push(c);
                i += 1;
                continue;
            }
            /* 只有 `{名字}` 才算插值。`{"key": 1}` 这种 JSON、`{}` 这种字面量里的花括号
             * 必须原样保留 —— 不然 json.parse("{\"a\":1}") 会被当成插值，报「变量 a 没有定义」。 */
            let starts_like_name = chars
                .get(i + 1)
                .map(|c2| c2.is_alphanumeric() || *c2 == '_')
                .unwrap_or(false);
            if !starts_like_name {
                buf.push(c);
                i += 1;
                continue;
            }
            // 找配对的 }
            let mut close: Option<usize> = None;
            let mut k = i + 1;
            while k < chars.len() {
                if chars[k] == '}' {
                    close = Some(k);
                    break;
                }
                k += 1;
            }
            let close = match close {
                Some(c2) => c2,
                None => {
                    buf.push(c);
                    i += 1;
                    continue;
                }
            };
            let inner: String = chars[i + 1..close].iter().collect();
            let inner = inner.trim();
            if inner.is_empty() {
                buf.push_str("{}");
                i = close + 1;
                continue;
            }
            if !buf.is_empty() {
                parts.push(Expr::new(ExprKind::Str(Rc::from(buf.as_str())), span.clone()));
                buf.clear();
            }
            match tokenize_line(inner, span.line, span.col + i as u32 + 1) {
                Ok(mut toks) => {
                    // 去掉行尾 Eof：at_end() 只认"用完 token"，不认 Eof 记号
                    if matches!(toks.last().map(|t| t.kind), Some(TokKind::Eof)) {
                        toks.pop();
                    }
                    match ExprParser::new(&toks, &self.file).parse_all() {
                        Ok(e) => parts.push(e),
                        Err(_) => {
                            // 插值里写错了：当普通文本，后面交给语法/语义检查
                            buf.push('{');
                            buf.push_str(inner);
                            buf.push('}');
                        }
                    }
                }
                Err(_) => {
                    buf.push('{');
                    buf.push_str(inner);
                    buf.push('}');
                }
            }
            i = close + 1;
        }
        if !buf.is_empty() {
            parts.push(Expr::new(ExprKind::Str(Rc::from(buf.as_str())), span.clone()));
        }
        if parts.is_empty() {
            return Expr::new(ExprKind::Str(Rc::from("")), span);
        }
        if parts.len() == 1 {
            return parts.pop().unwrap();
        }
        let mut acc = parts[0].clone();
        for p in parts[1..].iter() {
            acc = Expr::new(
                ExprKind::Bin { op: Op::Add, a: Box::new(acc), b: Box::new(p.clone()) },
                span.clone(),
            );
        }
        acc
    }

    /// 汇编式传参：空格分隔（顺带也认逗号），任意多个。
    /// `写文件 "a.txt" "内容"`、`调试 警告 a, "个"`。
    /// 每个参数都是完整表达式，所以 `调试 报告 x + 1` 里的 x + 1 算一个参数。
    fn parse_loose_args(&mut self) -> EResult<Vec<Expr>> {        let mut out = Vec::new();
        loop {
            if self.at(",") {
                self.pos += 1;
                continue;
            }
            if !self.operand_ahead() {
                break;
            }
            out.push(self.parse_expr(0)?);
        }
        Ok(out)
    }

    /// `add a b [c d …]`：空格分隔、左结合，任意多个数
    fn parse_math_chain(&mut self, op: Op, span: Span) -> EResult<Expr> {
        let mut acc: Option<Expr> = None;
        let mut n = 0usize;
        while self.operand_ahead() {
            let e = self.parse_unary()?;
            acc = Some(match acc {
                None => e,
                Some(prev) => Expr::new(
                    ExprKind::Bin { op, a: Box::new(prev), b: Box::new(e) },
                    span.clone(),
                ),
            });
            n += 1;
        }
        if n < 2 {
            return Err(self.err("add / sub / mul / div / mod 后面至少要两个数，例如 add a b"));
        }
        Ok(acc.unwrap())
    }

    fn parse_primary(&mut self) -> EResult<Expr> {
        let t = self.bump().cloned().ok_or_else(|| self.err("缺少表达式"))?;
        let span = self.span_of(&t);
        match t.kind {
            TokKind::Num => {
                /* 整数字面量超出 i64：以前会静默变成 f64，后面所有位运算/取模都跟着漂。
                 * 这里统一在解析期拦住；真要存大数请显式写成小数（加 .0）。 */
                let txt: &str = t.text.as_ref();
                let is_pure_int = !txt.contains('.')
                    && !txt.contains('e')
                    && !txt.contains('E')
                    && !txt.contains('x')
                    && !txt.contains('X');
                if is_pure_int {
                    let digits = txt.replace('_', "");
                    match digits.parse::<i128>() {
                        // 9223372036854775808 单独放行：`-9223372036854775808` 会被解析成
                        // "负号 + 正字面量"，这个数正好是 i64::MIN 的绝对值，不能拦。
                        Ok(v) if v > i64::MAX as i128 && v != i64::MAX as i128 + 1 => {
                            return Err(self.err(format!(
                                "整数 {} 超出 64 位整数范围（最大 9223372036854775807）。\
                                 要存这么大的数请写成小数形式（例如 {}.0），别让它悄悄变成浮点",
                                txt, txt
                            )));
                        }
                        Err(_) => {
                            return Err(self.err(format!(
                                "整数 {} 太长，超出 64 位整数范围（最大 9223372036854775807）",
                                txt
                            )));
                        }
                        // 能精确表示为 i64 的，原样保留（不要经过 f64，否则 2^53 以上会掉位）
                        Ok(v) => {
                            if let Ok(exact) = i64::try_from(v) {
                                return Ok(Expr::new(ExprKind::Int(exact), span));
                            }
                        }
                    }
                }
                Ok(Expr::new(ExprKind::Num(t.num.unwrap_or(f64::NAN)), span))
            }
            TokKind::Str => Ok(self.string_expr(t.text.clone(), span)),
            TokKind::Ident | TokKind::Kw => {
                // f{项, 项, …}：逗号拼接字面量。不带引号的项当表达式，每项自动转
                // 显示文本（不用手写 str()）；字符串里的空格原样保留。
                // 包一层 str() 再用 + 折叠，和 TS 侧同一套 AST 形状。
                if t.text == "f" && self.at("{") {
                    self.pos += 1;
                    let mut parts: Vec<Expr> = Vec::new();
                    if !self.at("}") {
                        loop {
                            let item = self.parse_expr(0)?;
                            let callee = Expr::new(ExprKind::Var(Rc::from("str")), span.clone());
                            parts.push(Expr::new(
                                ExprKind::Call { callee: Box::new(callee), args: vec![item] },
                                span.clone(),
                            ));
                            if self.at(",") {
                                self.pos += 1;
                                if self.at("}") { break; }
                                continue;
                            }
                            break;
                        }
                    }
                    self.expect("}", "f{…} 的右花括号 }")?;
                    let mut it = parts.into_iter();
                    let mut acc = match it.next() {
                        Some(e) => e,
                        None => return Ok(Expr::new(ExprKind::Str(Rc::from("")), span)),
                    };
                    for e in it {
                        acc = Expr::new(
                            ExprKind::Bin { op: Op::Add, a: Box::new(acc), b: Box::new(e) },
                            span.clone(),
                        );
                    }
                    return Ok(acc);
                }
                // 汇编式算术：`c = add a b`、`sub a b c`（空格分隔，能接任意多个数）。
                // 带括号的 `add(a, b)` 仍然走普通函数调用，两种写法都认。
                if let Some(op) = math_cmd_op(&t.text) {
                    if !self.at("(") && self.operand_ahead() {
                        return self.parse_math_chain(op, span);
                    }
                }
                // 文件/网络内置函数的汇编式传参：`x = 取网页 "url"`、`写文件 "a.txt" "内容"`。
                // 只对新内置函数启用，避免动到老语法；带括号的写法照常。
                if LOOSE_ARG_BUILTINS.contains(&t.text.as_str())
                    && !self.at("(")
                    && self.operand_ahead()
                {
                    let args = self.parse_loose_args()?;
                    let callee = Expr::new(ExprKind::Var(Rc::from(t.text.as_str())), span.clone());
                    return Ok(Expr::new(
                        ExprKind::Call { callee: Box::new(callee), args },
                        span,
                    ));
                }
                match t.text.as_str() {
                    // 内联函数：fn(x) return 表达式。必须是 fn( 紧连着，
                    // 否则 `fn g = double` 这种函数别名由语句层处理，别在这抢。
                    "fn" | "lambda" if self.at("(") => self.parse_lambda(span),
                    "true" => Ok(Expr::new(ExprKind::Bool(true), span)),
                    "false" => Ok(Expr::new(ExprKind::Bool(false), span)),
                    "null" => Ok(Expr::new(ExprKind::Null, span)),
                    "nil" | "none" => Err(FloatError::new(
                        format!("Copper 只有 null，没有 {}（旧写法已取消）。把 {} 换成 null", t.text, t.text),
                        span,
                    )),
                    "self" | "this" => Ok(Expr::new(ExprKind::SelfRef, span)),
                    _ => Ok(Expr::new(ExprKind::Var(Rc::from(t.text.as_str())), span)),
                }
            }
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
                        // 裸标识符键 = 字符串键（跟 d.k 成员访问、TS 引擎同一套语义）。
                        // 要表达式键就加括号：{(k): 1}，或者先建好再用 d[k] = …
                        let k = match (self.cur_ident(), self.peek_is(":")) {
                            (Some(name), true) => {
                                self.pos += 1;
                                Expr::new(ExprKind::Str(Rc::from(name.as_str())), span.clone())
                            }
                            _ => self.parse_expr(0)?,
                        };                        self.expect(":", "字面量字典要写成 {\"键\": 值}")?;
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

    /// 内联函数：fn(参数, …) return 表达式。
    /// 调用时 `fn` / `lambda` 已经被 parse_primary 吃掉，当前停在 `(`.
    fn parse_lambda(&mut self, span: Span) -> EResult<Expr> {
        self.expect("(", "内联函数要写成 fn(参数) return 表达式")?;
        let mut params: Vec<Param> = Vec::new();
        if !self.at(")") {
            loop {
                // lazy 参数：调用处不立即求值，函数体每次读取时重新算（by-name）
                let mut pt_lazy = false;
                if self.at("lazy") {
                    self.pos += 1;
                    pt_lazy = true;
                }
                let pt = self.bump().cloned().ok_or_else(|| self.err("内联函数的参数要是名字"))?;
                if !pt.is_ident() {
                    return Err(FloatError::new("内联函数的参数要是名字", self.span_of(&pt)));
                }
                let mut default = None;
                if self.at("=") {
                    self.pos += 1;
                    default = Some(self.parse_expr(0)?);
                }
                params.push(Param {
                    name: Rc::from(pt.text.as_str()),
                    ty: None,
                    default,
                    lazy: pt_lazy,
                    line: pt.line,
                    col: pt.col,
                });
                if self.at(",") {
                    self.pos += 1;
                    if self.at(")") { break; }
                    continue;
                }
                break;
            }
        }
        self.expect(")", "内联函数参数列表要闭合")?;
        if !self.at("return") {
            return Err(self.err("内联函数要写成 fn(参数) return 表达式——比如 fn(s) return s > 2"));
        }
        self.pos += 1; // return
        let body = self.parse_expr(0)?;
        let end = body.span.clone();
        let full = Span::new(self.file, span.line, span.col, end.end_col);
        Ok(Expr::new(ExprKind::Lambda { params, body: Box::new(body) }, full))
    }
}

pub struct Parser {
    /// 类名 → (父类名, 自己的字段名)：方法体里裸写继承来的字段也能补成 self.
    class_fields: std::collections::HashMap<Rc<str>, (Option<Rc<str>>, Vec<Rc<str>>)>,
    file: Rc<str>,
    lines: Vec<Line>,
    pos: usize,
    errors: Vec<FloatError>,
    /// 当前缩进块嵌套深度（防止把递归下降的栈吃穿）
    depth: usize,
}

/// 缩进块嵌套上限。正常人写代码很少超过 30 层；给了 200 层余量，
/// 既够用，又能在恶意/失控的输入下守住进程（不然递归会把栈直接吃掉）。
use crate::limits::NEST as MAX_NEST;

/// 单条表达式最多嵌套多少层（括号 / 调用 / 列表都算）。正常人写不到 50 层，
/// 给 64 层余量；真超了明确报错，而不是让递归把栈打穿（debug 构建栈帧很肥，
/// 256 层实测就能溢出，128 层也是边缘，64 层才稳）。
/// 表达式嵌套上限（括号 / 调用 / 列表都算），防栈溢出。
/// **前提：解析跑在大栈线程上**——debug 构建在默认 1~2MB 栈里 50 层左右会
/// 直接 abort（`lang_run` 与 `examples/check` 都已经套了 64MB 栈）。
/// 前端 parse.ts 的 MAX_EXPR_DEPTH 与它保持一致。
const MAX_EXPR_DEPTH: usize = 64;

/// 入口：把源码解析成 Program
pub fn parse_program(file: &str, src: &str) -> Program {
    let file: Rc<str> = Rc::from(file);
    // 块注释先剥掉（字符替换成空格，行列号不变），再走逐行流程。
    // 行内开块报 error（后面照旧按行注释处理，不中断整个文件的解析）；
    // 没闭合给 warning。
    let bc = crate::token::strip_block_comments(src);
    let (lines, lex_err) = split_source(&bc.text);
    let mut p = Parser {
        class_fields: std::collections::HashMap::new(),
        file: Rc::clone(&file),
        lines,
        pos: 0,
        errors: Vec::new(),
        depth: 0,
    };
    let mut warnings = Vec::new();
    for (l, c, msg) in &bc.errors {
        p.errors.push(FloatError::new(msg.clone(), Span::new(&file, *l, *c, *c + 2)));
    }
    for (l, c, msg) in &bc.warnings {
        warnings.push(FloatError::new(msg.clone(), Span::new(&file, *l, *c, *c + 2)));
    }
    if let Some((no, msg)) = lex_err {
        // 老式 `--` 注释：报错带改法，程序体不再解析
        p.errors.push(FloatError::new(msg, Span::new(&file, no, 1, 1)));
        let body = Vec::new();
        return Program { file, body, errors: p.errors, warnings };
    }
    let mut body = p.parse_block(0);
    // 类降级发生在逐行解析时，那时看不到别的类。这里补一遍：
    // 子类没写 init 的，继承父类的 init（参数和调用一起接过来）；
    // 顺手校验 `extends` 指向的父类真的存在（以前写错名字会静默当成没继承）。
    inherit_constructors(&mut body, &file, &mut p.errors);
    Program { file, body, errors: p.errors, warnings }
}

/// 子类没写 `init` 时，构造器要继承父类的 `init`：
/// `class Cat extends Animal` 就该能 `new Cat("咪咪", 8)`。
///
/// 以前构造器的参数只取**自己**的 init，子类不写 init 就变成零参数构造器，
/// 而调用点照样传两个实参 —— VM 报"类 Cat 没有 init 方法，不能接收参数"，
/// AOT 生成的 Rust 直接 E0061（参数个数对不上）。这不是某一份代码的问题，
/// 是"子类继承父类构造行为"这条规则没落地，所以在这里统一补上。
fn inherit_constructors(body: &mut [Stmt], file: &Rc<str>, errors: &mut Vec<FloatError>) {
    use std::collections::HashMap;
    // 类名 → (init 的参数, init 属于哪个类)
    let mut inits: HashMap<Rc<str>, (Vec<crate::ast::Param>, Rc<str>)> = HashMap::new();
    // 类名 → (直接父类, extends 那行所在的 span)
    let mut bases: HashMap<Rc<str>, (Rc<str>, Span)> = HashMap::new();
    // 需要补的构造器类名
    let mut ctors: Vec<Rc<str>> = Vec::new();

    // 类降级的产物可能被包在 Group 里（一行一个 class 也能成块），所以递归扫
    fn scan(
        stmts: &[Stmt],
        inits: &mut HashMap<Rc<str>, (Vec<crate::ast::Param>, Rc<str>)>,
        bases: &mut HashMap<Rc<str>, (Rc<str>, Span)>,
        ctors: &mut Vec<Rc<str>>,
    ) {
        for s in stmts {
            match &s.kind {
                StmtKind::Group(xs) => scan(xs, inits, bases, ctors),
                StmtKind::FuncDef { name, params, body: fb, .. } => {
                    let n = name.as_ref();
                    if let Some(cls) = n.strip_prefix("__init_").and_then(|r| r.strip_suffix('_')) {
                        // __init_ 的第 0 个参数是 self，构造器的参数表里不该有它
                        let ps: Vec<crate::ast::Param> = params
                            .iter()
                            .filter(|p| p.name.as_ref() != "self")
                            .cloned()
                            .collect();
                        inits.insert(Rc::from(cls), (ps, Rc::from(cls)));
                    } else if let Some(cls) = n.strip_prefix("__new_").and_then(|r| r.strip_suffix('_')) {
                        ctors.push(Rc::from(cls));
                        for st in fb {
                            if let StmtKind::Assign { target: Target::Idx(_, idx), value, .. } = &st.kind {
                                if let (ExprKind::Str(k), ExprKind::Str(v)) = (&idx.kind, &value.kind) {
                                    if k.as_ref() == "__base" {
                                        bases.insert(Rc::from(cls), (Rc::clone(v), s.span.clone()));
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    scan(body, &mut inits, &mut bases, &mut ctors);

    /* `class A extends Nope` 且 Nope 根本没定义：以前这条链会被静默当成"没有继承"，
     * 对象照样建出来、程序退出码 0 —— 典型的"静默改语义"。
     * 现在统一校验：所有 extends 指向的名字必须是同一份程序里定义过的类。 */
    for (cls, (parent, sp)) in &bases {
        if !ctors.iter().any(|c| c.as_ref() == parent.as_ref()) {
            errors.push(FloatError::new(
                format!(
                    "类 {} 继承的父类 {} 不存在（同一份程序里没有这个类，名字写错了吗？）",
                    cls, parent
                ),
                sp.clone(),
            ));
        }
    }

    // 父类自己也可能没 init：一路往上找到第一个有 init 的祖先
    fn resolve(
        cls: &Rc<str>,
        bases: &HashMap<Rc<str>, (Rc<str>, Span)>,
        inits: &HashMap<Rc<str>, (Vec<crate::ast::Param>, Rc<str>)>,
    ) -> Option<(Vec<crate::ast::Param>, Rc<str>)> {
        let mut cur = Rc::clone(cls);
        for _ in 0..32 {
            if let Some((ps, owner)) = inits.get(&cur) {
                return Some((ps.clone(), Rc::clone(owner)));
            }
            match bases.get(&cur) {
                Some((b, _)) => cur = Rc::clone(b),
                None => return None,
            }
        }
        None
    }

    fn patch(
        stmts: &mut [Stmt],
        inits: &HashMap<Rc<str>, (Vec<crate::ast::Param>, Rc<str>)>,
        bases: &HashMap<Rc<str>, (Rc<str>, Span)>,
        target: &Rc<str>,
    ) {
        for s in stmts.iter_mut() {
            match &mut s.kind {
                StmtKind::Group(xs) => patch(xs, inits, bases, target),
                StmtKind::FuncDef { name, params, body: fb, .. } => {
                    if name.as_ref() != format!("__new_{}_", target).as_str() {
                        continue;
                    }
                    // 自己已经有 init 的，本来就接好了（用类名判断，不是构造器函数名）
                    if inits.contains_key(target.as_ref()) {
                        continue;
                    }
                    let Some((iparams, owner)) = resolve(target, bases, inits) else { continue };
                    if owner.as_ref() == target.as_ref() {
                        continue; // 找到的是自己的 init（上面已判过，双保险）
                    }
                    if params.is_empty() {
                        *params = iparams.clone();
                    }
                    // 在 return self 之前插入 `__init_父类_(self, …)`
                    let mut args = vec![Expr::new(ExprKind::Var(Rc::from("self")), s.span.clone())];
                    for p in &iparams {
                        args.push(Expr::new(ExprKind::Var(Rc::clone(&p.name)), s.span.clone()));
                    }
                    let call = Stmt::new(
                        StmtKind::ExprStmt(Expr::new(
                            ExprKind::Call {
                                callee: Box::new(Expr::new(
                                    ExprKind::Var(Rc::from(format!("__init_{}_", owner).as_str())),
                                    s.span.clone(),
                                )),
                                args,
                            },
                            s.span.clone(),
                        )),
                        s.span.clone(),
                    );
                    let pos = fb
                        .iter()
                        .position(|st| matches!(st.kind, StmtKind::Return(_)))
                        .unwrap_or(fb.len());
                    fb.insert(pos, call);
                }
                _ => {}
            }
        }
    }

    for cls in ctors {
        patch(body, &inits, &bases, &cls);
    }
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

    /// 把一个 class 降级成几条普通函数（不改 VM 的值模型）：
    ///
    /// - 实例就是字典，多存一个 `"__cls"` 键记住类名
    /// - 方法编译成 `__m_类_方法(self, ...)`，`self` 就是第 0 个参数
    /// - 构造编译成 `__new_类(...)`：建字典 → 铺字段默认值 → 调 init → 返回
    /// - 方法体里裸写字段名（`hp -= damage`）会自动补成 `self.hp`
    fn parse_class(&mut self, ln: &Line, toks: &[Token]) -> Result<Stmt, FloatError> {
        let span = self.line_span(ln);
        self.check_inline_brace_body(ln, "class")?;
        let cname: Rc<str> = match toks.get(1) {
            Some(t) if t.kind == TokKind::Ident || t.kind == TokKind::Kw => Rc::from(t.text.as_str()),
            _ => return Err(FloatError::new("class 后面要写类名", span.clone())),
        };
        let base: Option<Rc<str>> = match toks.iter().position(|t| t.text == "extends") {
            Some(i) => toks.get(i + 1).map(|t| Rc::from(t.text.as_str())),
            None => None,
        };
        // 类体：缩进更深的行
        let indent = ln.indent;
        let start = self.pos;
        let mut end = start;
        while end < self.lines.len() && self.lines[end].indent > indent {
            end += 1;
        }
        let body_lines: Vec<Line> = self.lines[start..end].to_vec();

        // 拆字段与方法
        let mut fields: Vec<(Rc<str>, Option<Expr>)> = Vec::new();
        let mut methods: Vec<(Rc<str>, Vec<Param>, Vec<Stmt>)> = Vec::new();
        let mut i = 0;
        while i < body_lines.len() {
            let bl = &body_lines[i];
            // bl.code 已经剥过注释，理论上不会再撞上老 `--` 注释；真撞上就把错误带出去
            let code_l = code_part(&bl.code).map_err(|m| {
                FloatError::new(m, Span::new(&self.file, bl.no, bl.indent as u32 + 1, bl.indent as u32 + 1))
            })?.0;
            let col = bl.indent + 1;
            let toks_l = tokenize_line(&code_l, bl.no, col as u32)
                .map_err(|e| FloatError::new(e.message, Span::point(&self.file, e.line, e.col)))?;
            if toks_l.is_empty() {
                i += 1;
                continue;
            }
            let first = toks_l[0].text.clone();
            if matches!(first.as_str(), "fn" | "function" | "Function" | "f") {
                // 方法：名字 + 参数
                let mname: Rc<str> = match toks_l.get(1) {
                    Some(t) => Rc::from(t.text.as_str()),
                    None => return Err(FloatError::new("方法要有名字", self.line_span(bl))),
                };
                let mut params: Vec<Param> = Vec::new();
                let mut k = 2;
                if toks_l.get(k).map(|t| t.text.as_str()) == Some("(") {
                    k += 1;
                    while k < toks_l.len() && toks_l[k].text != ")" {
                        if toks_l[k].text == "," {
                            k += 1;
                            continue;
                        }
                        /* 方法参数和普通函数一样可以带类型 / lazy：
                         * `fn set(int v)` 以前把 "int" 当成参数名，这个方法就被算成
                         * 两个参数，调用 `d.set(4)` 直接报"需要 2 个参数"。
                         * 这里必须先把可选的 lazy 和类型词吃掉，再取真正的参数名。 */
                        let mut lazy = false;
                        if toks_l[k].text == "lazy" {
                            k += 1;
                            lazy = true;
                        }
                        let mut ty: Option<TypeName> = None;
                        if let Some(t) = toks_l.get(k) {
                            if t.kind == TokKind::Ident && TYPE_WORDS.contains(&t.text.as_str()) {
                                ty = TypeName::from_word(&t.text);
                                k += 1;
                            }
                        }
                        let pname: Rc<str> = match toks_l.get(k) {
                            Some(t) => Rc::from(t.text.as_str()),
                            None => return Err(FloatError::new("参数列表没写完", self.line_span(bl))),
                        };
                        let pcol = toks_l[k].col;
                        let mut default = None;
                        k += 1;
                        if toks_l.get(k).map(|t| t.text.as_str()) == Some("=") {
                            k += 1;
                            let mut ep = ExprParser::new(&toks_l[k..], &self.file);
                            default = Some(ep.parse_expr(0)?);
                            while k < toks_l.len() && toks_l[k].text != "," && toks_l[k].text != ")" {
                                k += 1;
                            }
                        }
                        params.push(Param { name: pname, ty, default, lazy, line: bl.no as u32, col: pcol });
                    }
                }
                // 方法体
                i += 1;
                self.pos = start + i;
                let mut mbody: Vec<Stmt> = Vec::new();
                while i < body_lines.len() && body_lines[i].indent > bl.indent {
                    let before = self.pos;
                    if let Some(st) = self.parse_statement() {
                        mbody.push(st);
                    }
                    if self.pos == before {
                        self.pos += 1; // 防御：不允许原地打转
                    }
                    i = self.pos - start;
                }
                methods.push((mname, params, mbody));
                continue;
            }
            // 字段：`int hp = 100` / `str name = ""`
            if toks_l.len() >= 2 {
                let mut fi = 0;
                if matches!(toks_l[0].text.as_str(), "int" | "float" | "str" | "bool" | "list" | "dict" | "ptr" | "addr" | "var") {
                    fi = 1;
                }
                if let Some(nt) = toks_l.get(fi) {
                    if nt.kind == TokKind::Ident {
                        let fname: Rc<str> = Rc::from(nt.text.as_str());
                        let mut default = None;
                        if toks_l.get(fi + 1).map(|t| t.text.as_str()) == Some("=") {
                            let mut ep = ExprParser::new(&toks_l[fi + 2..], &self.file);
                            default = Some(ep.parse_expr(0)?);
                        }
                        fields.push((fname, default));
                    }
                }
            }
            i += 1;
            self.pos = start + i;
        }
        self.pos = end;

        // 记下这个类自己的字段和父类，供继承者改写用
        self.class_fields.insert(
            Rc::clone(&cname),
            (base.clone(), fields.iter().map(|(n, _)| Rc::clone(n)).collect()),
        );
        let group = self.lower_class(&cname, &base, fields, methods, span.clone());
        Ok(Stmt::new(StmtKind::Group(group), span))
    }

    /// class → 函数集合（见 parse_class 的说明）
    fn lower_class(
        &mut self,
        cname: &Rc<str>,
        base: &Option<Rc<str>>,
        fields: Vec<(Rc<str>, Option<Expr>)>,
        methods: Vec<(Rc<str>, Vec<Param>, Vec<Stmt>)>,
        span: Span,
    ) -> Vec<Stmt> {
        let fname = |prefix: &str, cls: &Rc<str>, name: &str| -> Rc<str> {
            Rc::from(format!("{}{}_{}", prefix, cls, name).as_str())
        };
        let mut field_names: Vec<Rc<str>> = fields.iter().map(|(n, _)| Rc::clone(n)).collect();
        // 父类的字段也要算：方法里裸写 `hp -= 1` 时 hp 可能是继承来的
        let mut cur = base.clone();
        while let Some(cn) = cur {
            match self.class_fields.get(&cn) {
                Some((b, fs)) => {
                    for f in fs {
                        if !field_names.iter().any(|x| x == f) {
                            field_names.push(Rc::clone(f));
                        }
                    }
                    cur = b.clone();
                }
                None => break,
            }
        }
        let mut out: Vec<Stmt> = Vec::new();
        let mk_param = |n: Rc<str>| Param { name: n, ty: None, default: None, lazy: false, line: span.line as u32, col: 1 };

        for (mname, params, body) in &methods {
            let mut ps: Vec<Param> = vec![mk_param(Rc::from("self"))];
            ps.extend(params.iter().cloned());
            let rewritten = self.rewrite_self(body, &field_names, params, base, cname);
            out.push(Stmt::new(
                StmtKind::FuncDef {
                    name: fname("__m_", cname, mname),
                    params: ps,
                    ret: None,
                    body: rewritten,
                },
                span.clone(),
            ));
        }

        // init 单独编成一个函数（super(...) 会降级成调用父类的这个函数）
        let init = methods.iter().find(|(n, _, _)| &**n == "init");
        if let Some((_, iparams, ibody)) = init {
            let mut ps: Vec<Param> = vec![mk_param(Rc::from("self"))];
            ps.extend(iparams.iter().cloned());
            // 父类字段也算"字段"，这样 init 里裸写字段名也能补成 self.x
            let mut all_fields = field_names.clone();
            if let Some(b) = base {
                let _ = b;
            }
            let rewritten = self.rewrite_self(ibody, &all_fields, iparams, base, cname);
            all_fields.clear();
            out.push(Stmt::new(
                StmtKind::FuncDef {
                    name: fname("__init_", cname, ""),
                    params: ps,
                    ret: None,
                    body: rewritten,
                },
                span.clone(),
            ));
        }

        // 字段初始化函数：先调父类的 __fields_，再铺自己的
        {
            let mut fb: Vec<Stmt> = Vec::new();
            if let Some(b) = base {
                fb.push(Stmt::new(
                    StmtKind::ExprStmt(Expr::new(
                        ExprKind::Call {
                            callee: Box::new(Expr::new(ExprKind::Var(fname("__fields_", b, "")), span.clone())),
                            args: vec![Expr::new(ExprKind::Var(Rc::from("self")), span.clone())],
                        },
                        span.clone(),
                    )),
                    span.clone(),
                ));
            }
            for (f, default) in &fields {
                fb.push(Stmt::new(
                    StmtKind::Assign {
                        target: Target::Member(Expr::new(ExprKind::Var(Rc::from("self")), span.clone()), Rc::clone(f)),
                        op: AssignOp::Set,
                        value: default.clone().unwrap_or(Expr::new(ExprKind::Null, span.clone())),
                    },
                    span.clone(),
                ));
            }
            out.push(Stmt::new(
                StmtKind::FuncDef {
                    name: fname("__fields_", cname, ""),
                    params: vec![mk_param(Rc::from("self"))],
                    ret: None,
                    body: fb,
                },
                span.clone(),
            ));
        }

        // 构造
        let mut new_params: Vec<Param> = Vec::new();
        let mut body: Vec<Stmt> = Vec::new();
        // dict self = {}
        body.push(Stmt::new(
            StmtKind::Decl {
                ty: crate::TypeName::Dict,
                name: Rc::from("self"),
                init: Some(Expr::new(ExprKind::Dict(Vec::new()), span.clone())),
            },
            span.clone(),
        ));
        // self["__cls"] = 类名
        body.push(Stmt::new(
            StmtKind::Assign {
                target: Target::Idx(
                    Expr::new(ExprKind::Var(Rc::from("self")), span.clone()),
                    Expr::new(ExprKind::Str(Rc::from("__cls")), span.clone()),
                ),
                op: AssignOp::Set,
                value: Expr::new(ExprKind::Str(Rc::clone(cname)), span.clone()),
            },
            span.clone(),
        ));
        // 记下父类名，VM 找不到方法时会沿这条链往上找
        if let Some(b) = base {
            body.push(Stmt::new(
                StmtKind::Assign {
                    target: Target::Idx(
                        Expr::new(ExprKind::Var(Rc::from("self")), span.clone()),
                        Expr::new(ExprKind::Str(Rc::from("__base")), span.clone()),
                    ),
                    op: AssignOp::Set,
                    value: Expr::new(ExprKind::Str(Rc::clone(b)), span.clone()),
                },
                span.clone(),
            ));
        }

        // 铺字段（父类 → 自己）
        body.push(Stmt::new(
            StmtKind::ExprStmt(Expr::new(
                ExprKind::Call {
                    callee: Box::new(Expr::new(ExprKind::Var(fname("__fields_", cname, "")), span.clone())),
                    args: vec![Expr::new(ExprKind::Var(Rc::from("self")), span.clone())],
                },
                span.clone(),
            )),
            span.clone(),
        ));
        // 调 init
        if let Some((_, iparams, _)) = init {
            let mut args = vec![Expr::new(ExprKind::Var(Rc::from("self")), span.clone())];
            for p in iparams {
                new_params.push(p.clone());
                args.push(Expr::new(ExprKind::Var(Rc::clone(&p.name)), span.clone()));
            }
            body.push(Stmt::new(
                StmtKind::ExprStmt(Expr::new(
                    ExprKind::Call {
                        callee: Box::new(Expr::new(ExprKind::Var(fname("__init_", cname, "")), span.clone())),
                        args,
                    },
                    span.clone(),
                )),
                span.clone(),
            ));
        }
        body.push(Stmt::new(
            StmtKind::Return(Some(Expr::new(ExprKind::Var(Rc::from("self")), span.clone()))),
            span.clone(),
        ));
        out.push(Stmt::new(
            StmtKind::FuncDef {
                name: fname("__new_", cname, ""),
                params: new_params,
                ret: None,
                body,
            },
            span.clone(),
        ));
        out
    }

    /// 方法体改写：裸字段名 → self.字段；super(...) → 父类 init
    fn rewrite_self(
        &self,
        body: &[Stmt],
        fields: &[Rc<str>],
        params: &[Param],
        base: &Option<Rc<str>>,
        cname: &Rc<str>,
    ) -> Vec<Stmt> {
        let mut locals: Vec<Rc<str>> = params.iter().map(|p| Rc::clone(&p.name)).collect();
        locals.push(Rc::from("self"));
        self.rw_block(body, fields, &mut locals, base, cname)
    }

    fn rw_block(
        &self,
        body: &[Stmt],
        fields: &[Rc<str>],
        locals: &mut Vec<Rc<str>>,
        base: &Option<Rc<str>>,
        cname: &Rc<str>,
    ) -> Vec<Stmt> {
        let mut out = Vec::new();
        for st in body {
            out.push(self.rw_stmt(st, fields, locals, base, cname));
        }
        out
    }

    fn rw_stmt(
        &self,
        st: &Stmt,
        fields: &[Rc<str>],
        locals: &mut Vec<Rc<str>>,
        base: &Option<Rc<str>>,
        cname: &Rc<str>,
    ) -> Stmt {
        let sp = st.span.clone();
        let kind = match &st.kind {
            StmtKind::Decl { ty, name, init } => {
                let i = init.as_ref().map(|e| self.rw_expr(e, fields, locals, base, cname));
                locals.push(Rc::clone(name));
                StmtKind::Decl { ty: *ty, name: Rc::clone(name), init: i }
            }
            StmtKind::Assign { target, op, value } => StmtKind::Assign {
                target: self.rw_target(target, fields, locals, base, cname),
                op: *op,
                value: self.rw_expr(value, fields, locals, base, cname),
            },
            StmtKind::IncDec { target, delta, prefix } => StmtKind::IncDec {
                target: self.rw_target(target, fields, locals, base, cname),
                delta: *delta,
                prefix: *prefix,
            },
            StmtKind::ExprStmt(e) => StmtKind::ExprStmt(self.rw_expr(e, fields, locals, base, cname)),
            StmtKind::Print(a) => StmtKind::Print(a.iter().map(|e| self.rw_expr(e, fields, locals, base, cname)).collect()),
            StmtKind::Report(a) => StmtKind::Report(a.iter().map(|e| self.rw_expr(e, fields, locals, base, cname)).collect()),
            StmtKind::Debug { level, args } => StmtKind::Debug {
                level: *level,
                args: args.iter().map(|e| self.rw_expr(e, fields, locals, base, cname)).collect(),
            },
            StmtKind::Return(v) => StmtKind::Return(v.as_ref().map(|e| self.rw_expr(e, fields, locals, base, cname))),
            StmtKind::If { branches, else_body } => StmtKind::If {
                branches: branches
                    .iter()
                    .map(|(c, b)| (c.as_ref().map(|e| self.rw_expr(e, fields, locals, base, cname)), self.rw_block(b, fields, locals, base, cname)))
                    .collect(),
                else_body: else_body.as_ref().map(|b| self.rw_block(b, fields, locals, base, cname)),
            },
            StmtKind::While { cond, body } => StmtKind::While {
                cond: self.rw_expr(cond, fields, locals, base, cname),
                body: self.rw_block(body, fields, locals, base, cname),
            },
            StmtKind::Repeat { count, body } => StmtKind::Repeat {
                count: self.rw_expr(count, fields, locals, base, cname),
                body: self.rw_block(body, fields, locals, base, cname),
            },
            other => other.clone(),
        };
        Stmt::new(kind, sp)
    }

    fn rw_target(
        &self,
        t: &Target,
        fields: &[Rc<str>],
        locals: &mut Vec<Rc<str>>,
        _base: &Option<Rc<str>>,
        _cname: &Rc<str>,
    ) -> Target {
        match t {
            Target::Var(n) if fields.iter().any(|f| f == n) && !locals.iter().any(|l| l == n) => Target::Member(
                Expr::new(ExprKind::SelfRef, Span::point(&self.file, 1, 1)),
                Rc::clone(n),
            ),
            other => other.clone(),
        }
    }

    fn rw_expr(
        &self,
        e: &Expr,
        fields: &[Rc<str>],
        locals: &mut Vec<Rc<str>>,
        base: &Option<Rc<str>>,
        cname: &Rc<str>,
    ) -> Expr {
        let sp = e.span.clone();
        let kind = match &e.kind {
            ExprKind::Var(n) if fields.iter().any(|f| f == n) && !locals.iter().any(|l| l == n) => ExprKind::Member {
                obj: Box::new(Expr::new(ExprKind::SelfRef, sp.clone())),
                name: Rc::clone(n),
            },
            ExprKind::Var(n) => ExprKind::Var(Rc::clone(n)),
            ExprKind::Bin { op, a, b } => ExprKind::Bin {
                op: *op,
                a: Box::new(self.rw_expr(a, fields, locals, base, cname)),
                b: Box::new(self.rw_expr(b, fields, locals, base, cname)),
            },
            ExprKind::Un { op, a } => ExprKind::Un { op: *op, a: Box::new(self.rw_expr(a, fields, locals, base, cname)) },
            ExprKind::Idx { obj, index } => ExprKind::Idx {
                obj: Box::new(self.rw_expr(obj, fields, locals, base, cname)),
                index: Box::new(self.rw_expr(index, fields, locals, base, cname)),
            },
            ExprKind::Member { obj, name } => ExprKind::Member {
                obj: Box::new(self.rw_expr(obj, fields, locals, base, cname)),
                name: Rc::clone(name),
            },
            ExprKind::List(items) => ExprKind::List(items.iter().map(|x| self.rw_expr(x, fields, locals, base, cname)).collect()),
            ExprKind::Dict(es) => ExprKind::Dict(
                es.iter()
                    .map(|(k, v)| (self.rw_expr(k, fields, locals, base, cname), self.rw_expr(v, fields, locals, base, cname)))
                    .collect(),
            ),
            ExprKind::Call { callee, args } => {
                // super(args) → 父类 init(self, args)
                if let ExprKind::Var(n) = &callee.kind {
                    if &**n == "super" {
                        if let Some(b) = base {
                            let mut a2 = vec![Expr::new(ExprKind::SelfRef, sp.clone())];
                            for a in args {
                                a2.push(self.rw_expr(a, fields, locals, base, cname));
                            }
                            return Expr::new(
                                ExprKind::Call {
                                    callee: Box::new(Expr::new(ExprKind::Var(Rc::from(format!("__init_{}_", b).as_str())), sp.clone())),
                                    args: a2,
                                },
                                sp,
                            );
                        }
                    }
                }
                ExprKind::Call {
                    callee: Box::new(self.rw_expr(callee, fields, locals, base, cname)),
                    args: args.iter().map(|a| self.rw_expr(a, fields, locals, base, cname)).collect(),
                }
            }
            other => other.clone(),
        };
        Expr::new(kind, sp)
    }

    /// 老式命令语句 → 现有 AST（不改编译器、不改 VM）
    fn legacy_command(&mut self, ln: &Line, toks: &[Token]) -> Result<Option<Stmt>, FloatError> {
        let word = toks[0].text.clone();
        let span = self.line_span(ln);
        let mk_call = |recv: Rc<str>, method: &str, args: Vec<Expr>, sp: Span| {
            Expr::new(
                ExprKind::Call {
                    callee: Box::new(Expr::new(
                        ExprKind::Member {
                            obj: Box::new(Expr::new(ExprKind::Var(recv), sp.clone())),
                            name: Rc::from(method),
                        },
                        sp.clone(),
                    )),
                    args,
                },
                sp,
            )
        };
        let ident_at = |i: usize| -> Option<Rc<str>> {
            toks.get(i)
                .filter(|t| matches!(t.kind, TokKind::Ident | TokKind::Kw))
                .map(|t| Rc::from(t.text.as_str()))
        };

        match word.as_str() {
            // add X to L
            "add" => {
                let to = match toks.iter().position(|t| t.text == "to") { Some(i) => i, None => return Ok(None) };
                let name = match ident_at(to + 1) { Some(n) => n, None => return Ok(None) };
                let mut ep = ExprParser::new(&toks[1..to], &self.file);
                let value = ep.parse_expr(0)?;
                Ok(Some(Stmt::new(StmtKind::ExprStmt(mk_call(name, "push", vec![value], span.clone())), span)))
            }
            // insert X to L at N（方法签名是 insert(下标, 值)）
            "insert" => {
                let to = match toks.iter().position(|t| t.text == "to") { Some(i) => i, None => return Ok(None) };
                let at = match toks.iter().position(|t| t.text == "at") { Some(i) => i, None => return Ok(None) };
                let name = match ident_at(to + 1) { Some(n) => n, None => return Ok(None) };
                let mut ep = ExprParser::new(&toks[1..to], &self.file);
                let value = ep.parse_expr(0)?;
                let mut ip = ExprParser::new(&toks[at + 1..], &self.file);
                let at_expr = ip.parse_expr(0)?;
                Ok(Some(Stmt::new(StmtKind::ExprStmt(mk_call(name, "insert", vec![at_expr, value], span.clone())), span)))
            }
            // replace L N with V
            "replace" => {
                let with = match toks.iter().position(|t| t.text == "with") { Some(i) => i, None => return Ok(None) };
                let name = match ident_at(1) { Some(n) => n, None => return Ok(None) };
                let mut ip = ExprParser::new(&toks[2..with], &self.file);
                let idx = ip.parse_expr(0)?;
                let mut vp = ExprParser::new(&toks[with + 1..], &self.file);
                let value = vp.parse_expr(0)?;
                let obj = Expr::new(ExprKind::Var(name), span.clone());
                Ok(Some(Stmt::new(
                    StmtKind::Assign { target: Target::Idx(obj, idx), op: AssignOp::Set, value },
                    span,
                )))
            }
            // del L [N]
            "del" => {
                let name = match ident_at(1) { Some(n) => n, None => return Ok(None) };
                if toks.len() <= 2 {
                    Ok(Some(Stmt::new(StmtKind::ExprStmt(mk_call(name, "clear", vec![], span.clone())), span)))
                } else {
                    let mut ep = ExprParser::new(&toks[2..], &self.file);
                    let idx = ep.parse_expr(0)?;
                    Ok(Some(Stmt::new(StmtKind::ExprStmt(mk_call(name, "remove", vec![idx], span.clone())), span)))
                }
            }
            // read L N → __last_read
            "read" => {
                let name = match ident_at(1) { Some(n) => n, None => return Ok(None) };
                let mut ep = ExprParser::new(&toks[2..], &self.file);
                let idx = ep.parse_expr(0)?;
                let obj = Expr::new(ExprKind::Var(name), span.clone());
                let value = Expr::new(ExprKind::Idx { obj: Box::new(obj), index: Box::new(idx) }, span.clone());
                Ok(Some(Stmt::new(
                    StmtKind::Assign { target: Target::Var(Rc::from("__last_read")), op: AssignOp::Set, value },
                    span,
                )))
            }
            // len X → __last_len
            "len" => {
                let mut ep = ExprParser::new(&toks[1..], &self.file);
                let value = ep.parse_expr(0)?;
                let call = Expr::new(
                    ExprKind::Call {
                        callee: Box::new(Expr::new(ExprKind::Var(Rc::from("len")), span.clone())),
                        args: vec![value],
                    },
                    span.clone(),
                );
                Ok(Some(Stmt::new(
                    StmtKind::Assign { target: Target::Var(Rc::from("__last_len")), op: AssignOp::Set, value: call },
                    span,
                )))
            }
            // dict D key K = V / read K / del K
            "dict" => {
                let name = match ident_at(1) { Some(n) => n, None => return Ok(None) };
                let op = match toks.get(2).map(|t| t.text.clone()) { Some(v) => v, None => return Ok(None) };
                let obj = Expr::new(ExprKind::Var(name), span.clone());
                match op.as_str() {
                    "key" => {
                        let eq = match toks.iter().position(|t| t.text == "=") { Some(i) => i, None => return Ok(None) };
                        let mut kp = ExprParser::new(&toks[3..eq], &self.file);
                        let key = kp.parse_expr(0)?;
                        let mut vp = ExprParser::new(&toks[eq + 1..], &self.file);
                        let value = vp.parse_expr(0)?;
                        Ok(Some(Stmt::new(
                            StmtKind::Assign { target: Target::Idx(obj, key), op: AssignOp::Set, value },
                            span,
                        )))
                    }
                    "read" => {
                        let mut kp = ExprParser::new(&toks[3..], &self.file);
                        let key = kp.parse_expr(0)?;
                        let value = Expr::new(ExprKind::Idx { obj: Box::new(obj), index: Box::new(key) }, span.clone());
                        Ok(Some(Stmt::new(
                            StmtKind::Assign { target: Target::Var(Rc::from("__last_read")), op: AssignOp::Set, value },
                            span,
                        )))
                    }
                    "del" => {
                        let mut kp = ExprParser::new(&toks[3..], &self.file);
                        let key = kp.parse_expr(0)?;
                        let name = match ident_at(1) { Some(n) => n, None => return Ok(None) };
                        Ok(Some(Stmt::new(
                            StmtKind::ExprStmt(mk_call(name, "remove", vec![key], span.clone())),
                            span,
                        )))
                    }
                    _ => Ok(None),
                }
            }
            _ => Ok(None),
        }
    }

    fn expr_of(&mut self, toks: &[Token]) -> EResult<Expr> {
        ExprParser::new(toks, &self.file).parse_all()
    }

    fn to_target(&self, e: Expr) -> EResult<Target> {
        match e.kind {
            ExprKind::Var(name) => Ok(Target::Var(name)),
            ExprKind::Member { obj, name } => Ok(Target::Member(*obj, name)),
            ExprKind::Idx { obj, index } => Ok(Target::Idx(*obj, *index)),
            // *p = 值
            ExprKind::Un { op: Op::Deref, a } => Ok(Target::Deref(*a)),
            _ => Err(FloatError::new("这里只能给变量、成员、下标或 *指针 赋值", e.span)),
        }
    }

    fn parse_block(&mut self, min_indent: usize) -> Vec<Stmt> {
        // 嵌套太深就停下来说清楚，而不是让递归把栈吃穿（直接崩进程）
        if self.depth >= MAX_NEST {
            if let Some(ln) = self.lines.get(self.pos).cloned() {
                let span = self.line_span(&ln);
                self.errors.push(FloatError::new(
                    format!(
                        "缩进嵌套太深（超过 {} 层）停下来了。把这段拆成函数，或者把条件合并，都比往下堆层更清楚",
                        MAX_NEST
                    ),
                    span,
                ));
            }
            // 整块跳过，避免上层把它当普通语句再解析一遍
            while self.pos < self.lines.len() && self.lines[self.pos].indent >= min_indent {
                self.pos += 1;
            }
            return Vec::new();
        }
        self.depth += 1;
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
        self.depth -= 1;
        body
    }

    fn parse_body_of(&mut self, indent: usize) -> Vec<Stmt> {
        self.parse_block(indent + 1)
    }

    /// 控制流关键字的块体不能是空的：`if a == 1` 后面顶格写一句，
    /// 这句不会进 if，而是变成"下一句普通语句"——条件为假时照样执行，
    /// 用户看到的就是"条件失效"。这种静默改语义必须报错，不能放过。
    fn parse_body_required(&mut self, kw: &str, ln: &Line) -> Vec<Stmt> {
        let start = self.pos;
        let had_err = self.errors.len();
        let body = self.parse_body_of(ln.indent);
        if body.is_empty() && self.pos == start && self.errors.len() == had_err {
            let span = self.line_span(ln);
            self.errors.push(FloatError::new(
                format!(
                    "{} 后面要有块体：下一行必须比这一行多缩进；顶格写的那句会跑到 {} 外面去（条件为假也照样执行）",
                    kw, kw
                ),
                span,
            ));
        }
        body
    }

    /// 块体挤在头部同一行（`if x { print(1) }`）会被静默丢掉，这里定向拦住指路。
    /// 与 parse.ts 的 checkInlineBraceBody 对齐。
    fn check_inline_brace_body(&self, ln: &Line, kw: &str) -> Result<(), FloatError> {
        if let Some(col) = find_inline_brace_body(&ln.code) {
            let col = col as u32;
            return Err(FloatError::new(
                format!(
                    "{} 这一行把块体写在了花括号里——块体要另起一行多缩进 1 格写；行尾单独一个 {{ 可以开块，}} else {{ 可以接分支，但 {{ 语句 }} 挤在同一行不支持",
                    kw
                ),
                Span::new(&self.file, ln.no, col, col + 1),
            ));
        }
        Ok(())
    }

    /// 遇到这版 VM 还没实现的语法时用：明确报错，而不是静默失败
    #[allow(dead_code)]
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
        let mut code: &str = &ln.code;
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
                    self.check_inline_brace_body(ln, "while")?;
                    let cond = self.expr_of(&toks[1..])?;
                    let body = self.parse_body_of(ln.indent);
                    return Ok(Some(Stmt::new(StmtKind::While { cond, body }, self.line_span(ln))));
                }
                "repeat" => {
                    self.pos += 1;
                    self.check_inline_brace_body(ln, "repeat")?;
                    let count = self.expr_of(&toks[1..])?;
                    let body = self.parse_body_of(ln.indent);
                    return Ok(Some(Stmt::new(StmtKind::Repeat { count, body }, self.line_span(ln))));
                }
                "forever" => {
                    self.pos += 1;
                    self.check_inline_brace_body(ln, "forever")?;
                    let body = self.parse_body_of(ln.indent);
                    let cond = Expr::new(ExprKind::Bool(true), self.line_span(ln));
                    return Ok(Some(Stmt::new(StmtKind::While { cond, body }, self.line_span(ln))));
                }
                "main" if toks.get(1).map(|t| t.text == "loop").unwrap_or(false) => {
                    self.pos += 1;
                    self.check_inline_brace_body(ln, "main loop")?;
                    let body = self.parse_body_of(ln.indent);
                    let cond = Expr::new(ExprKind::Bool(true), self.line_span(ln));
                    return Ok(Some(Stmt::new(StmtKind::While { cond, body }, self.line_span(ln))));
                }
                "for" => return Ok(Some(self.parse_for(ln, &toks)?)),
                "break" if toks.len() == 1 => {
                    self.pos += 1;
                    return Ok(Some(Stmt::new(StmtKind::Break, self.line_span(ln))));
                }
                "out" if toks.len() == 1 => {
                    return Err(FloatError::new(
                        "out 已经取消，改成 break（两个名字做同一件事，留一个就够了）".to_string(),
                        self.line_span(ln),
                    ));
                }
                "balo" | "continue" if toks.len() == 1 => {
                    self.pos += 1;
                    return Ok(Some(Stmt::new(StmtKind::Continue, self.line_span(ln))));
                }
                "stop" => {
                    // 只留 stop（原先 stop / stop all / stop now / stop script 四个名字一个行为）
                    if toks.len() == 1 {
                        self.pos += 1;
                        return Ok(Some(Stmt::new(StmtKind::Stop, self.line_span(ln))));
                    }
                    if matches!(toks.get(1).map(|t| t.text.as_str()), Some("all") | Some("now") | Some("script")) {
                        return Err(FloatError::new(
                            format!("stop {} 已经合并：直接写 stop", toks[1].text),
                            self.line_span(ln),
                        ));
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
                // ── 老式命令写法（照单全收，落到现有的方法与下标操作上）──
                "add" | "insert" | "replace" | "del" | "read" | "len" | "dict"
                    if toks.get(1).map(|t| t.text.as_str()) != Some("(")
                        && (toks[0].text != "dict"
                            || matches!(toks.get(2).map(|t| t.text.as_str()), Some("key") | Some("read") | Some("del"))) =>
                {
                    // 必须推进行指针！否则外层 while 会把同一行反复解析（内存直接吃光）
                    self.pos += 1;
                    return match self.legacy_command(ln, &toks)? {
                        Some(st) => Ok(Some(st)),
                        None => Err(FloatError::new(
                            format!(
                                "「{}」这条命令没看懂。列表的写法是：add 值 to 列表 / insert 值 to 列表 at 下标 / replace 列表 下标 with 值 / del 列表 [下标] / read 列表 下标。\n  如果你是想做算术，那就不是这条 add：写成 c = add a b（c 要先写成空变量 int c）",
                                toks[0].text
                            ),
                            self.line_span(ln),
                        )),
                    };
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
                // 调试输出：`调试 报告 内容` / `调试 警告 内容` / `调试 报错 内容`
                // （也认函数式写法 调试(报告, 内容)）。纯给开发者看的，不影响程序。
                "调试" | "debug" => {
                    let level_word = |s: &str| -> Option<u8> {
                        match s {
                            "报告" | "report" | "info" => Some(1),
                            "警告" | "warn" | "warning" => Some(2),
                            "报错" | "error" | "err" => Some(3),
                            _ => None,
                        }
                    };
                    self.pos += 1;
                    let (level, rest): (u8, &[Token]) =
                        if toks.get(1).map(|t| t.text.as_str()) == Some("(") {
                            // 函数式：调试(报告, 内容)
                            match toks.get(2).map(|t| t.text.as_str()).and_then(level_word) {
                                Some(lv) => {
                                    if toks.get(3).map(|t| t.text.as_str()) != Some(",") {
                                        return Err(FloatError::new(
                                            "调试(级别, 内容) 里级别后面要跟逗号，比如 调试(报告, \"好\")",
                                            self.line_span(ln),
                                        ));
                                    }
                                    let inner = &toks[4..];
                                    if inner.last().map(|t| t.text.as_str()) != Some(")") {
                                        return Err(FloatError::new("调试(…) 少了右括号", self.line_span(ln)));
                                    }
                                    (lv, &inner[..inner.len() - 1])
                                }
                                None => return Err(FloatError::new(
                                    "调试 后面要写级别：报告 / 警告 / 报错，比如 调试(报告, \"好\")",
                                    self.line_span(ln),
                                )),
                            }
                        } else {
                            match toks.get(1).map(|t| t.text.as_str()).and_then(level_word) {
                                Some(lv) => (lv, &toks[2..]),
                                None => return Err(FloatError::new(
                                    "调试 后面要写级别：调试 报告 …（蓝）/ 调试 警告 …（黄）/ 调试 报错 …（红）",
                                    self.line_span(ln),
                                )),
                            }
                        };
                    let mut ep = ExprParser::new(rest, &self.file);
                    let args = if rest.is_empty() {
                        Vec::new()
                    } else if ep.at("(") {
                        ep.parse_args()?
                    } else {
                        // 汇编式：调试 报告 x "个" / 调试 警告 a, "个" 都认
                        ep.parse_loose_args()?
                    };
                    if !ep.at_end() {
                        return Err(FloatError::new("调试 后面多了内容", self.line_span(ln)));
                    }
                    return Ok(Some(Stmt::new(StmtKind::Debug { level, args }, self.line_span(ln))));
                }
                "class" => {
                    self.pos += 1;
                    return Ok(Some(self.parse_class(ln, &toks)?));
                }
                "try" | "尝试" => return Ok(Some(self.parse_try(ln, &toks)?)),
                "throw" | "抛出" => {
                    self.pos += 1;
                    let mut ep = ExprParser::new(&toks[1..], &self.file);
                    let v = ep.parse_expr(0)?;
                    if !ep.at_end() {
                        return Err(FloatError::new("throw 后面多了内容", self.line_span(ln)));
                    }
                    return Ok(Some(Stmt::new(StmtKind::Throw(v), self.line_span(ln))));
                }
                // import 在进解析器之前就被展开成源码了（见 import.rs）。
                // 还能走到这里 = 写在了缩进块里，那就明确告诉他写错位置了。
                "import" | "引入" => {
                    return Err(FloatError::new(
                        "import 只能写在文件最外层（顶格），不能放进函数或 if 里面",
                        self.line_span(ln),
                    ))
                }
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
                "wait" => {
                    // wait until (条件)：条件不成立就一直转，成立才往下走
                    if toks.get(1).map(|t| t.text == "until").unwrap_or(false) {
                        self.pos += 1;
                        let cond = self.expr_of(&toks[2..])?;
                        let body = self.parse_body_of(ln.indent);
                        return Ok(Some(Stmt::new(StmtKind::WaitUntil { cond, body }, self.line_span(ln))));
                    }
                    self.pos += 1;
                    // 纯 wait 秒：桌面版没有"等一会儿再刷新界面"这回事，直接跳过
                    return Ok(Some(Stmt::new(StmtKind::Nop, self.line_span(ln))));
                }
                "time" => {
                    match toks.get(1).map(|t| t.text.as_str()) {
                        Some("read") => {
                            self.pos += 1;
                            return Ok(Some(Stmt::new(StmtKind::TimeRead, self.line_span(ln))));
                        }
                        Some("reset") => {
                            self.pos += 1;
                            return Ok(Some(Stmt::new(StmtKind::TimeReset, self.line_span(ln))));
                        }
                        _ => return Err(FloatError::new(
                            "time 只有两种写法：time read（读秒数到 __last_time）、time reset（把计时器归零）",
                            self.line_span(ln),
                        )),
                    }
                }
                _ => {}
            }

            if FUNC_WORDS.contains(&word) && (word != "f" || toks.get(1).map(|t| t.is_ident()).unwrap_or(false)) {
                // fn 别名：fn g = double → g = double（函数当值）
                if toks.get(1).map(|t| t.is_ident()).unwrap_or(false)
                    && toks.get(2).map(|t| t.text.as_str()) == Some("=")
                {
                    let mut vp = ExprParser::new(&toks[2..], &self.file);
                    vp.pos += 1; // 跳过 =
                    let value = vp.parse_expr(0)?;
                    if !vp.at_end() {
                        return Err(FloatError::new("赋值号右边多了内容", self.line_span(ln)));
                    }
                    self.pos += 1;
                    let name = Rc::from(toks[1].text.as_str());
                    return Ok(Some(Stmt::new(
                        StmtKind::Assign {
                            target: Target::Var(name),
                            op: AssignOp::Set,
                            value,
                        },
                        self.line_span(ln),
                    )));
                }
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

        // 带返回类型的函数：`int add(a, b)`（返回类型写在函数名前面，和变量声明一个写法）
        if is_ident
            && TYPE_WORDS.contains(&word)
            && toks.get(1).map(|t| t.is_ident()).unwrap_or(false)
            && toks.get(2).map(|t| t.text == "(").unwrap_or(false)
        {
            return Ok(Some(self.parse_func_def(ln, &toks)?));
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

        // 前缀自增自减：++i / --i（后缀 i++ 在下面表达式语句里处理）
        if toks.len() >= 2
            && toks[0].kind == TokKind::Op
            && (toks[0].text == "++" || toks[0].text == "--")
            && (toks[1].is_ident() || toks[1].kind == TokKind::Kw)
        {
            let delta = if toks[0].text == "++" { 1 } else { -1 };
            let mut tp = ExprParser::new(&toks[1..], &self.file);
            let target_e = tp.parse_expr(0)?;
            if !tp.at_end() {
                return Err(FloatError::new("自增自减后面不能再有内容", self.line_span(ln)));
            }
            self.pos += 1;
            let target = self.to_target(target_e)?;
            return Ok(Some(Stmt::new(
                StmtKind::IncDec { target, delta, prefix: true },
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
        /* 指针新写法：write(p, 值) 单独成句 → 等价的 *p = 值。
         * 只当语句用（它没有返回值），写成表达式会明确报错。 */
        if let ExprKind::Call { callee, args } = &first_expr.kind {
            if let ExprKind::Var(name) = &callee.kind {
                if &**name == "write" && args.len() == 2 {
                    let span = self.line_span(ln);
                    return Ok(Some(Stmt::new(
                        StmtKind::Assign {
                            target: crate::ast::Target::Deref(args[0].clone()),
                            op: crate::ast::AssignOp::Set,
                            value: args[1].clone(),
                        },
                        span,
                    )));
                }
                if &**name == "write" {
                    return Err(FloatError::new(
                        "write 要写成 write(地址, 值)，两个参数",
                        self.line_span(ln),
                    ));
                }
            }
        }
        Ok(Some(Stmt::new(StmtKind::ExprStmt(first_expr), self.line_span(ln))))
    }

    /// try / catch / finally。
    ///
    /// ```text
    /// try
    ///     危险动作()
    /// catch e
    ///     print "出错了：" + e.消息
    /// finally
    ///     收尾()
    /// ```
    fn parse_try(&mut self, ln: &Line, toks: &[Token]) -> EResult<Stmt> {
        if toks.len() > 1 {
            return Err(FloatError::new(
                "try 后面不用写条件，换行直接写块就行",
                self.line_span(ln),
            ));
        }
        self.pos += 1;
        let body = self.parse_body_of(ln.indent);
        let mut catch_name: Option<Rc<str>> = None;
        let mut catch_body: Option<Vec<Stmt>> = None;
        let mut finally: Option<Vec<Stmt>> = None;

        loop {
            let next = match self.lines.get(self.pos) {
                Some(l) => l.clone(),
                None => break,
            };
            if next.skipped {
                self.pos += 1;
                continue;
            }
            let code_owned = next.code.to_string();
            let mut code: &str = &code_owned;
            if let Some(rest) = code.strip_prefix('}') {
                code = rest.trim();
            }
            if code.is_empty() || next.indent != ln.indent {
                break;
            }
            let kw_end = code
                .char_indices()
                .find(|(_, c)| !c.is_alphanumeric() && *c != '_')
                .map(|(i, _)| i)
                .unwrap_or(code.len());
            let kw = &code[..kw_end];
            match kw {
                "catch" | "捕获" => {
                    if catch_body.is_some() || catch_name.is_some() {
                        return Err(FloatError::new("一个 try 只能有一个 catch", self.line_span(&next)));
                    }
                    let mut rest = code[kw_end..].trim();
                    rest = rest.trim_end_matches('{').trim();
                    rest = rest.trim_start_matches('(').trim_end_matches(')').trim();
                    if !rest.is_empty() {
                        let name: String = rest
                            .chars()
                            .take_while(|c| c.is_alphanumeric() || *c == '_')
                            .collect();
                        if !name.is_empty() {
                            catch_name = Some(Rc::from(name.as_str()));
                        }
                    }
                    self.pos += 1;
                    catch_body = Some(self.parse_body_of(next.indent));
                }
                "finally" | "最后" => {
                    self.pos += 1;
                    finally = Some(self.parse_body_of(next.indent));
                    break;
                }
                _ => break,
            }
        }

        if catch_body.is_none() && catch_name.is_none() && finally.is_none() {
            return Err(FloatError::new(
                "try 后面要跟 catch 或 finally，否则接住了错误也没人处理",
                self.line_span(ln),
            ));
        }
        Ok(Stmt::new(
            StmtKind::Try { body, catch_name, catch_body, finally },
            self.line_span(ln),
        ))
    }

    fn parse_if(&mut self, ln: &Line, toks: &[Token]) -> EResult<Stmt> {        self.pos += 1;
        let mut rest = &toks[1..];
        let mut is_if_out = false;
        if rest.first().map(|t| t.text == "out").unwrap_or(false) {
            is_if_out = true;
            rest = &rest[1..];
        }
        let cond = if rest.is_empty() { None } else { Some(self.expr_of(rest)?) };
        if is_if_out {
            let cond = cond.ok_or_else(|| FloatError::new("if out 需要一个条件，例如 if out (i > 3)", self.line_span(ln)))?;
            return Ok(Stmt::new(StmtKind::IfOut(cond), self.line_span(ln)));
        }
        let cond = cond.ok_or_else(|| FloatError::new("if 后面要跟条件，例如 if (a > 1)", self.line_span(ln)))?;
        self.check_inline_brace_body(ln, "if")?;
        let mut branches = vec![(Some(cond), self.parse_body_required("if", ln))];
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
            /* 注释 / 空行夹在 if 和 elif 中间不能把链断掉（和 TS 侧同一规则） */
            while let Some(l) = self.lines.get(self.pos) {
                if l.skipped || l.is_comment || l.code.is_empty() {
                    self.pos += 1;
                } else {
                    break;
                }
            }
            let ln = match self.lines.get(self.pos) {
                Some(l) => l.clone(),
                None => return,
            };
            if ln.skipped {
                return;
            }
            let code_owned = ln.code.to_string();
            let mut code: &str = &code_owned;
            let mut brace_close = false;
            if let Some(rest) = code.strip_prefix('}') {
                brace_close = true;
                code = rest.trim();
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
                *else_body = Some(self.parse_body_required("else", &ln));
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
        self.check_inline_brace_body(ln, "for")?;
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
        self.check_inline_brace_body(ln, "函数定义")?;
        /* 两种写法都认：
         *   `fn int add(a, b)`  —— 返回类型跟在 fn 后面（老写法）
         *   `int add(a, b)`     —— 返回类型写在函数名前面（和变量声明一致）
         */
        let mut idx = 1usize;
        let mut ret: Option<Rc<str>> = None;
        if let (Some(t0), Some(t1)) = (toks.get(1), toks.get(2)) {
            if t0.is_ident()
                && TYPE_WORDS.contains(&t0.text.as_str())
                && t1.is_ident()
            {
                ret = Some(Rc::from(t0.text.as_str()));
                idx = 2;
            }
        }
        let name_tok = toks.get(idx).cloned().ok_or_else(|| FloatError::new("function 后面要跟函数名", self.line_span(ln)))?;
        if !name_tok.is_ident() && name_tok.kind != TokKind::Kw {
            return Err(FloatError::new("function 后面要跟函数名", self.line_span(ln)));
        }
        let mut ep = ExprParser::new(&toks[idx + 1..], &self.file);
        let mut params: Vec<Param> = Vec::new();
        if ep.at("(") {
            ep.pos += 1;
            if !ep.at(")") {
                loop {
                    // lazy 参数：`function f(x, lazy y)` —— y 在函数体里读一次算一次
                    let mut lazy = false;
                    if ep.at("lazy") {
                        ep.pos += 1;
                        lazy = true;
                    }
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
                        lazy,
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
            StmtKind::FuncDef { name: Rc::from(name_tok.text.as_str()), params, ret, body },
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

/// 汇编式算术的命令词 → 对应运算符
/// 这些内置函数支持汇编式传参（空格分隔）：`writefile "a.txt" "内容"`。
/// 只放新加的文件/网络函数——老函数不动，零回退风险；
/// now 没有参数，不在表里，用 `now()` 带括号调。中文别名一并认（兼容早期实现）。
const LOOSE_ARG_BUILTINS: [&str; 21] = [
    "readfile", "writefile", "appendfile", "hasfile", "fetch", "post",
    "chdir", "listdir", "makedir", "removedir", "removefile", "renamefile", "getenv", "pathjoin", "runcmd",
    "读文件", "写文件", "加文件", "有文件", "取网页", "发网页",
];

fn math_cmd_op(w: &str) -> Option<Op> {
    Some(match w {
        "add" => Op::Add,
        "sub" => Op::Sub,
        "mul" => Op::Mul,
        "div" => Op::Div,
        "mod" => Op::Mod,
        _ => return None,
    })}
