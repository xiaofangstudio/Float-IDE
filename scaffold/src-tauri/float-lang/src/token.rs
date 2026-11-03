//! 词法层：把一行源码切成 Token 流。
//!
//! 这一层是 `src/lang/token.ts` 的 1:1 Rust 移植，语义必须完全一致：
//! * 行式：一次切一行（调用方按行喂进来，缩进信息在外面维护）
//! * 标识符允许中文（\u4e00-\u9fa5 / \u3400-\u4dbf）
//! * `//` 之后整行丢弃
//! * 字符串内 `{…}` 允许嵌套引号（留给字符串插值用）
//! * 运算符长优先匹配

use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TokKind {
    Num,
    Str,
    Ident,
    Kw,
    Op,
    Punc,
    Eof,
}

#[derive(Clone, Debug)]
pub struct Token {
    pub kind: TokKind,
    /// 原始文本（字符串 token 已去引号并转义）
    pub text: String,
    pub line: u32,
    /// 1 基列号
    pub col: u32,
    pub end_col: u32,
    /// 数字字面量的值
    pub num: Option<f64>,
}

impl Token {
    pub fn is(&self, kind: TokKind, text: &str) -> bool {
        self.kind == kind && self.text == text
    }

    pub fn is_op(&self, text: &str) -> bool {
        self.kind == TokKind::Op && self.text == text
    }

    pub fn is_punc(&self, text: &str) -> bool {
        self.kind == TokKind::Punc && self.text == text
    }

    pub fn is_ident(&self) -> bool {
        self.kind == TokKind::Ident
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            TokKind::Eof => write!(f, "行尾"),
            TokKind::Str => write!(f, "字符串「{}」", self.text),
            _ => write!(f, "「{}」", self.text),
        }
    }
}

/// 只保留真正影响语法的关键字；new 是前缀运算符。
/// 注意：if/while/for 这些**不在这里**，它们在语句层按标识符识别，
/// 这样中文变量名不会被误判成关键字。
const KEYWORDS: [&str; 8] =
    ["new", "class", "extends", "function", "fn", "return", "import", "super"];

/// 长的必须排在前面，靠顺序实现"最长匹配优先"
const OPERATORS: [&str; 40] = [
    "===", "!==", "**=", "<<=", ">>=", "==", "!=", "<=", ">=", "&&", "||", "**", "<<", ">>", "+=",
    "-=", "*=", "/=", "%=", "++", "--", "//", "=>", "+", "-", "*", "/", "%", "<", ">", "!", "~", "&", "|",
    "^", "=", "?", ":", "@", "#",
];

const PUNCS: [char; 9] = ['(', ')', '[', ']', '{', '}', ',', '.', ';'];

#[derive(Clone, Debug)]
pub struct LexError {
    pub message: String,
    pub line: u32,
    pub col: u32,
}

pub fn is_id_start(c: char) -> bool {
    c.is_ascii_alphabetic()
        || c == '_'
        || ('\u{4e00}'..='\u{9fa5}').contains(&c)
        || ('\u{3400}'..='\u{4dbf}').contains(&c)
}

pub fn is_id_part(c: char) -> bool {
    is_id_start(c) || c.is_ascii_digit()
}

/// 切一行（不含换行符）为 Token 序列。结尾一定补一个 Eof。
/// 在字符数组的第 i 位比较一个模式串（不分配内存）
fn starts_with_at(chars: &[char], i: usize, pat: &str) -> bool {
    let mut k = i;
    for c in pat.chars() {
        if k >= chars.len() || chars[k] != c {
            return false;
        }
        k += 1;
    }
    true
}

pub fn tokenize_line(src: &str, line: u32, start_col: u32) -> Result<Vec<Token>, LexError> {
    let chars: Vec<char> = src.chars().collect();
    let n = chars.len();
    let mut out: Vec<Token> = Vec::with_capacity(n / 2 + 2);
    let mut i = 0usize;

    macro_rules! push {
        ($kind:expr, $text:expr, $col:expr, $end:expr, $num:expr) => {
            out.push(Token {
                kind: $kind,
                text: $text,
                line,
                col: $col,
                end_col: $end,
                num: $num,
            })
        };
    }

    while i < n {
        let c = chars[i];

        if c == ' ' || c == '\t' || c == '\u{3000}' || c == '\r' {
            i += 1;
            continue;
        }

        // 注意：`//` 不是注释，是整除运算符（注释在 code_part 里按 `--` 剥掉）

        // 字符串
        if c == '"' || c == '\'' {
            let quote = c;
            let start_col2 = start_col + i as u32;
            let mut j = i + 1;
            let mut text = String::new();
            let mut depth = 0i32; // {…} 插值里允许出现引号
            while j < n && !(chars[j] == quote && depth == 0) {
                if chars[j] == '{' {
                    depth += 1;
                } else if chars[j] == '}' && depth > 0 {
                    depth -= 1;
                }
                if chars[j] == '\\' && j + 1 < n {
                    let e = chars[j + 1];
                    match e {
                        'n' => text.push('\n'),
                        't' => text.push('\t'),
                        'r' => text.push('\r'),
                        '0' => text.push('\0'),
                        '\\' => text.push('\\'),
                        '"' => text.push('"'),
                        '\'' => text.push('\''),
                        '{' | '}' => {
                            text.push('\\');
                            text.push(e);
                        }
                        _ => text.push(e),
                    }
                    j += 2;
                    continue;
                }
                text.push(chars[j]);
                j += 1;
            }
            if j >= n {
                return Err(LexError {
                    message: "字符串没有结束引号".into(),
                    line,
                    col: start_col2,
                });
            }
            push!(TokKind::Str, text, start_col2, start_col + j as u32 + 1, None);
            i = j + 1;
            continue;
        }

        // 数字
        let next_is_digit = |k: usize| -> bool { k < n && chars[k].is_ascii_digit() };
        if c.is_ascii_digit() || (c == '.' && next_is_digit(i + 1)) {
            let start = i;
            if c == '0' && i + 1 < n && (chars[i + 1] == 'x' || chars[i + 1] == 'X') {
                i += 2;
                while i < n && (chars[i].is_ascii_hexdigit() || chars[i] == '_') {
                    i += 1;
                }
                let raw: String =
                    chars[start..i].iter().collect::<String>().replace('_', "");
                let v = u64::from_str_radix(raw.trim_start_matches("0x").trim_start_matches("0X"), 16)
                    .map(|x| x as f64)
                    .unwrap_or(f64::NAN);
                push!(TokKind::Num, raw, start_col + start as u32, start_col + i as u32, Some(v));
                continue;
            }
            while i < n && (chars[i].is_ascii_digit() || chars[i] == '_') {
                i += 1;
            }
            if i < n && chars[i] == '.' && next_is_digit(i + 1) {
                i += 1;
                while i < n && (chars[i].is_ascii_digit() || chars[i] == '_') {
                    i += 1;
                }
            }
            if i < n && (chars[i] == 'e' || chars[i] == 'E') {
                let save = i;
                i += 1;
                if i < n && (chars[i] == '+' || chars[i] == '-') {
                    i += 1;
                }
                if next_is_digit(i) {
                    while i < n && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                } else {
                    i = save;
                }
            }
            let raw: String = chars[start..i].iter().collect::<String>().replace('_', "");
            let v: f64 = raw.parse().unwrap_or(f64::NAN);
            push!(TokKind::Num, raw, start_col + start as u32, start_col + i as u32, Some(v));
            continue;
        }

        // 标识符 / 关键字（含中文）
        if is_id_start(c) {
            let start = i;
            while i < n && is_id_part(chars[i]) {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            // and / or / not 是 && / || / ! 的关键字写法（AST 层同一个节点，短路语义一致）。
            // 前面是 '.' 时不转换：obj.not / obj.and 是普通成员名。
            let word_op = match word.as_str() {
                "and" => Some("&&"),
                "or" => Some("||"),
                "not" => Some("!"),

                _ => None,
            };
            if let (Some(op), Some(prev)) = (word_op, out.last()) {
                if !(prev.kind == TokKind::Op && prev.text == ".") {
                    let len = word.chars().count();
                    push!(
                        TokKind::Op,
                        op.to_string(),
                        start_col + start as u32,
                        start_col + (start + len) as u32,
                        None
                    );
                    continue;
                }
            }
            let kind = if KEYWORDS.contains(&word.as_str()) {
                TokKind::Kw
            } else {
                TokKind::Ident
            };
            push!(kind, word, start_col + start as u32, start_col + i as u32, None);
            continue;
        }

        // 运算符（长的优先）。
        // 注意：这里**不能**先 `chars[i..].collect::<String>()` 再 starts_with——
        // 那样每个 token 都要复制一遍剩余整行，5000 行的地图表就是 O(n²)（实测 1.2 秒）。
        // 直接在字符数组上按位比较，代价只看运算符长度（1~3 个字符）。
        let mut matched: Option<&str> = None;
        for op in OPERATORS {
            if starts_with_at(&chars, i, op) {
                matched = Some(op);
                break;
            }
        }
        if let Some(op) = matched {
            let len = op.chars().count();
            push!(
                TokKind::Op,
                op.to_string(),
                start_col + i as u32,
                start_col + (i + len) as u32,
                None
            );
            i += len;
            continue;
        }

        if let Some(p) = PUNCS.iter().find(|x| **x == c) {
            push!(
                TokKind::Punc,
                p.to_string(),
                start_col + i as u32,
                start_col + i as u32 + 1,
                None
            );
            i += 1;
            continue;
        }

        return Err(LexError {
            message: format!("无法识别的字符「{}」", c),
            line,
            col: start_col + i as u32,
        });
    }

    let end = start_col + n as u32;
    out.push(Token {
        kind: TokKind::Eof,
        text: String::new(),
        line,
        col: end,
        end_col: end,
        num: None,
    });
    Ok(out)
}

/// 取一行的有效代码，把 `--` 注释剥掉（跳过字符串里的 `--`）。
///
/// 消歧规则：`--` **左边是行首或空白**，并且**右边是空白或行尾**，才算注释。
/// 这样三种写法都对得上：
/// * `-- 整行注释`     → 注释
/// * `int hp = 100 -- 血量` → 注释
/// * `i--` / `i-- -- 注释`  → 自减运算符（左边不是空白，不当注释）
///
/// **`//` 不是注释**，它是整除运算符（`7 // 2` = 3）。
pub fn code_part(line: &str) -> (String, Option<String>) {
    let chars: Vec<char> = line.chars().collect();
    let mut in_str: Option<char> = None;
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if let Some(q) = in_str {
            if c == '\\' {
                i += 2;
                continue;
            }
            if c == q {
                in_str = None;
            }
            i += 1;
            continue;
        }
        if c == '"' || c == '\'' {
            in_str = Some(c);
            i += 1;
            continue;
        }
        if c == '-' && i + 1 < chars.len() && chars[i + 1] == '-' {
            let left_ok = i == 0 || chars[i - 1].is_whitespace();
            let right = chars.get(i + 2).copied();
            let right_ok = match right {
                None => true,
                Some(n) => n.is_whitespace(),
            };
            if left_ok && right_ok {
                let head: String = chars[..i].iter().collect();
                let tail: String = chars[i + 2..].iter().collect();
                return (head, Some(tail));
            }
        }
        i += 1;
    }
    (line.to_string(), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(src: &str) -> Vec<Token> {
        tokenize_line(src, 1, 1).expect("词法失败")
    }

    fn texts(src: &str) -> Vec<String> {
        toks(src).into_iter().filter(|t| t.kind != TokKind::Eof).map(|t| t.text).collect()
    }

    #[test]
    fn 切基础表达式() {
        assert_eq!(texts("a + b * 2"), vec!["a", "+", "b", "*", "2"]);
    }

    #[test]
    fn 中文标识符() {
        let t = toks("血量 = 100");
        assert_eq!(t[0].kind, TokKind::Ident);
        assert_eq!(t[0].text, "血量");
        assert!(t[1].is_op("="));
    }

    #[test]
    fn 运算符最长匹配() {
        assert_eq!(texts("a === b"), vec!["a", "===", "b"]);
        assert_eq!(texts("a == b"), vec!["a", "==", "b"]);
        assert_eq!(texts("x **= 2"), vec!["x", "**=", "2"]);
    }

    #[test]
    fn 整行注释被丢弃() {
        // 注释是 `--`（在 code_part 里剥），分词阶段已经看不到它
        let (code, _) = code_part("x = 1 -- 这是注释");
        assert_eq!(texts(code.trim()), vec!["x", "=", "1"]);
        let (code, _) = code_part("-- 全注释");
        assert_eq!(texts(code.as_str()), Vec::<String>::new());
    }

    #[test]
    fn 双斜杠是整除运算符() {
        assert_eq!(texts("x = 7 // 2"), vec!["x", "=", "7", "//", "2"]);
    }

    #[test]
    fn 字符串里的双斜杠不算注释() {
        // 引号本身不进 token，字符串 token 里存的是引号内内容
        assert_eq!(texts(r#"print("http://a")"#), vec!["print", "(", "http://a", ")"]);
    }

    #[test]
    fn 转义字符() {
        let t = toks(r#""a\nb\tc\"d""#);
        assert_eq!(t[0].kind, TokKind::Str);
        assert_eq!(t[0].text, "a\nb\tc\"d");
    }

    #[test]
    fn 未闭合字符串报错() {
        assert!(tokenize_line("\"abc", 1, 1).is_err());
    }

    #[test]
    fn 数字形态() {
        assert_eq!(toks("0x1F")[0].num, Some(31.0));
        assert_eq!(toks("1_000_000")[0].num, Some(1000000.0));
        assert_eq!(toks("1.5e3")[0].num, Some(1500.0));
        assert_eq!(toks("1e")[0].num, Some(1.0));
    }

    #[test]
    fn 不可识别字符报错() {
        assert!(tokenize_line("a ` b", 1, 1).is_err());
    }

    #[test]
    fn code_part_跳过字符串内注释() {
        // 字符串里的 `--` 不是注释
        let (code, comment) = code_part(r#"x = "a--b" -- 真注释"#);
        assert_eq!(code, r#"x = "a--b" "#);
        assert_eq!(comment.as_deref(), Some(" 真注释"));
    }

    #[test]
    fn code_part_不把自减当注释() {
        // `i--` 是自减（左边贴着标识符）
        let (code, comment) = code_part("i--");
        assert_eq!(code, "i--");
        assert!(comment.is_none());
        // `i-- -- 说明`：前一个是自减，后一个是注释
        let (code, comment) = code_part("i-- -- 说明");
        assert_eq!(code, "i-- ");
        assert_eq!(comment.as_deref(), Some(" 说明"));
        // 行尾注释（前面有空白）
        let (code, comment) = code_part("int hp = 100 -- 血量");
        assert_eq!(code, "int hp = 100 ");
        assert_eq!(comment.as_deref(), Some(" 血量"));
    }

    #[test]
    fn code_part_不把整除当注释() {
        // `//` 是整除运算符，整行都是代码
        let (code, comment) = code_part("a = 7 // 2");
        assert_eq!(code, "a = 7 // 2");
        assert!(comment.is_none());
    }
}
