//! 文本替换 —— 类似 C 的 `#define`，但更简单、更好玩。
//!
//! 正式写法是英文的两种（都能用）：
//!
//! ```text
//! alias 名字 = 内容
//! #define 名字 内容
//! ```
//!
//! 这一行**之后**，所有出现「名字」的地方（整词匹配）都会换成「内容」。
//! 比如：
//!
//! ```text
//! alias 你好 = print("你好，世界")
//! 你好
//!
//! #define 双倍 x * 2
//! int x = 21
//! print(双倍)
//! ```
//!
//! 中文的 `换 名字 成 内容` 也能用（早期实现留下的写法，纯兼容）。
//!
//! 实现要点：
//! * 纯文本级替换（在词法分析之前做），所以内容可以是任意代码片段，不限于表达式；
//! * **行数不变**：定义行本身变成空行，替换都在同一行内完成，
//!   所以报错的行号永远还是用户看到的那一行（这是信任问题，不能含糊）；
//! * 支持重新定义（后面的覆盖前面的），支持多轮展开（A→B、B→2），
//!   绕成环会明确报错而不是死循环。

use crate::error::{FloatError, Span};

/// 把「换 名字 成 内容」全部展开掉。行数与输入严格一致。
pub fn expand(file: &str, src: &str) -> Result<String, FloatError> {
    let mut table: Vec<(String, String)> = Vec::new();
    let mut out: Vec<String> = Vec::with_capacity(src.lines().count());
    for (i, raw) in src.lines().enumerate() {
        let ln = (i + 1) as u32;
        match parse_def(raw, file, ln)? {
            Some((name, value)) => {
                // 定义行变成空行：行号不变，块结构也不受影响
                out.push(String::new());
                // 后者覆盖前者，跟 #define 重新定义一个意思
                if let Some(slot) = table.iter_mut().find(|(n, _)| *n == name) {
                    slot.1 = value;
                } else {
                    table.push((name, value));
                }
            }
            None => {
                let mut stack: Vec<usize> = Vec::new();
                let line = expand_line(raw, &table, &mut stack, file, ln)?;
                out.push(line);
            }
        }
    }
    Ok(out.join("\n"))
}

/// 展开一行：遇到别名就换成内容，换出来的内容再继续展开（A→B、B→2）。
///
/// 用「正在展开」的栈防止自己套自己：
/// * `换 你好 成 print("你好")` —— 换出来的内容里又出现「你好」，
///   这时候它正在展开中，就当普通文字留着（C 的 blue paint 规则），
///   否则会 print("print("print("… 无限套下去；
/// * `换 a 成 b` + `换 b 成 a` —— 这是真的绕成环了，直接报错。
fn expand_line(
    line: &str,
    table: &[(String, String)],
    stack: &mut Vec<usize>,
    file: &str,
    ln: u32,
) -> Result<String, FloatError> {
    if stack.len() > 16 {
        return Err(cycle_err(file, ln, "别名套得太深（16 层），多半是互相指了"));
    }
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len());
    let mut i = 0usize;
    while i < chars.len() {
        let mut matched = false;
        for (idx, (from, to)) in table.iter().enumerate() {
            let f: Vec<char> = from.chars().collect();
            if f.is_empty() || chars.len() - i < f.len() || chars[i..i + f.len()] != f[..] {
                continue;
            }
            let prev_ok = i == 0 || !is_word_char(chars[i - 1]);
            let next_ok = i + f.len() == chars.len() || !is_word_char(chars[i + f.len()]);
            if !prev_ok || !next_ok {
                continue;
            }
            if let Some(&top) = stack.last() {
                if stack.contains(&idx) {
                    // 自己引用自己：当成普通文字留着（不然永远展开不完）
                    if top == idx {
                        out.push_str(from);
                        i += f.len();
                        matched = true;
                        break;
                    }
                    return Err(cycle_err(file, ln, &format!("「{}」和「{}」互相指", table[top].0, from)));
                }
            }
            stack.push(idx);
            let sub = expand_line(to, table, stack, file, ln)?;
            stack.pop();
            out.push_str(&sub);
            i += f.len();
            matched = true;
            break;
        }
        if matched {
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    Ok(out)
}

fn cycle_err(file: &str, ln: u32, detail: &str) -> FloatError {
    FloatError::new(
        format!(
            "别名绕成环了：{}。检查一下你的 换 … 成 … 有没有互相指",
            detail
        ),
        Span::new(&file.into(), ln, 1, 2),
    )
}

/// 认一行是不是定义行。三种写法（英文两种是正式写法，中文一种兼容早期实现）：
///   alias 名字 = 内容
///   #define 名字 内容
///   换 名字 成 内容
fn parse_def(raw: &str, file: &str, ln: u32) -> Result<Option<(String, String)>, FloatError> {
    let s = raw.trim();
    let span = || Span::new(&file.into(), ln, 1, 2);

    // ── #define 名字 内容（C 风格，最省事）──
    if let Some(after) = strip_word_prefix(s, "#define") {
        let after = after.trim_start();
        let (name, rest) = split_name(after);
        if name.is_empty() {
            return Err(FloatError::new(
                "#define 后面要跟名字和内容，例如：#define PI 3.14",
                span(),
            ));
        }
        let value = rest.trim();
        if value.is_empty() {
            return Err(FloatError::new(
                format!("#define {} 后面没写内容：#define 名字 内容", name),
                span(),
            ));
        }
        return Ok(Some((name.to_string(), value.to_string())));
    }

    // ── alias 名字 = 内容 ──
    if let Some(after) = strip_word_prefix(s, "alias") {
        let after = after.trim_start();
        let (name, rest) = split_name(after);
        if name.is_empty() {
            return Err(FloatError::new(
                "alias 后面要跟名字和内容，例如：alias PI = 3.14",
                span(),
            ));
        }
        let rest = rest.trim_start();
        let value = match rest.strip_prefix('=') {
            Some(v) => v.trim(),
            None => {
                return Err(FloatError::new(
                    format!("alias 的写法是 alias 名字 = 内容，例如：alias {} = 42", name),
                    span(),
                ))
            }
        };
        if value.is_empty() {
            return Err(FloatError::new(
                format!("alias {} = 后面没写内容", name),
                span(),
            ));
        }
        return Ok(Some((name.to_string(), value.to_string())));
    }

    // ── 换 名字 成 内容（兼容）──
    if !s.starts_with('换') {
        return Ok(None);
    }
    let rest = &s['换'.len_utf8()..];
    // 「换」后面必须是空白：否则 换行/换算 这种普通标识符会被误判成定义
    if !rest.starts_with(|c: char| c.is_whitespace()) {
        return Ok(None);
    }
    let rest = rest.trim_start();
    let name_end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
    let name = &rest[..name_end];
    if name.is_empty() {
        return Err(FloatError::new(
            "换 后面要写名字和内容，例如：换 你好 成 print(\"你好\")（推荐写成 alias 名字 = 内容）",
            span(),
        ));
    }
    let after = rest[name_end..].trim_start();
    let after = match after.strip_prefix('成') {
        Some(a) if a.is_empty() || a.starts_with(|c: char| c.is_whitespace()) => a.trim_start(),
        _ => {
            return Err(FloatError::new(
                format!(
                    "「换 {} …」少了「成」：写法是 换 名字 成 内容，或者用 alias {} = 内容",
                    name, name
                ),
                span(),
            ))
        }
    };
    if after.trim().is_empty() {
        return Err(FloatError::new(
            format!("「换 {} 成」后面没写内容：要把 {} 换成什么写出来", name, name),
            span(),
        ));
    }
    Ok(Some((name.to_string(), after.trim_end().to_string())))
}

/// 前缀必须是完整单词（`alias` 后面跟空白，避免把 aliasfoo 当成关键字）
fn strip_word_prefix<'a>(s: &'a str, word: &str) -> Option<&'a str> {
    let rest = s.strip_prefix(word)?;
    if rest.is_empty() || rest.starts_with(|c: char| c.is_whitespace()) {
        Some(rest)
    } else {
        None
    }
}

/// 切出第一个空格前的名字
fn split_name(s: &str) -> (&str, &str) {
    match s.find(|c: char| c.is_whitespace()) {
        Some(i) => (&s[..i], &s[i..]),
        None => (s, ""),
    }
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ex(src: &str) -> String {
        expand("main.cw", src).expect("应展开成功")
    }

    #[test]
    fn 基本替换() {
        assert_eq!(ex("alias 两次 = 2\nprint(两次)"), "\nprint(2)");
        assert_eq!(ex("#define 两次 2\nprint(两次)"), "\nprint(2)");
        // 中文写法兼容
        assert_eq!(ex("换 两次 成 2\nprint(两次)"), "\nprint(2)");
    }

    #[test]
    fn 别名可以是英文名() {
        assert_eq!(ex("alias PI = 3.14\nprint(PI)"), "\nprint(3.14)");
        assert_eq!(ex("#define MAX 100\nprint(MAX)"), "\nprint(100)");
    }

    #[test]
    fn 换成代码片段() {
        assert_eq!(ex("alias 你好 = print(\"你好\")\n你好"), "\nprint(\"你好\")");
        assert_eq!(ex("#define 你好 print(\"你好\")\n你好"), "\nprint(\"你好\")");
    }

    #[test]
    fn 整词才算() {
        assert_eq!(ex("换 值 成 1\nprint(值, 变量值)"), "\nprint(1, 变量值)");
    }

    #[test]
    fn 重新定义覆盖旧的() {
        assert_eq!(ex("换 a 成 1\nprint(a)\n换 a 成 2\nprint(a)"), "\nprint(1)\n\nprint(2)");
    }

    #[test]
    fn 多轮展开() {
        assert_eq!(ex("换 a 成 b\n换 b 成 9\nprint(a)"), "\n\nprint(9)");
    }

    #[test]
    fn 环会报错() {
        let e = expand("main.cw", "换 a 成 b\n换 b 成 a\nprint(a)").unwrap_err();
        assert!(e.message.contains("环"), "{}", e.message);
    }

    #[test]
    fn 缺成或缺内容会报错() {
        let e = expand("main.cw", "换 a 42").unwrap_err();
        assert!(e.message.contains("成"), "{}", e.message);
        let e = expand("main.cw", "换 a 成").unwrap_err();
        assert!(e.message.contains("没写内容"), "{}", e.message);
        let e = expand("main.cw", "alias a 42").unwrap_err();
        assert!(e.message.contains("alias"), "{}", e.message);
        let e = expand("main.cw", "#define a").unwrap_err();
        assert!(e.message.contains("#define"), "{}", e.message);
    }

    #[test]
    fn 普通标识符不会被误判() {
        // 换行 是以「换」开头的普通变量名；aliasfoo 也不是 alias 定义
        assert_eq!(ex("int 换行 = 1\nprint(换行)"), "int 换行 = 1\nprint(换行)");
        assert_eq!(ex("int aliasfoo = 1\nprint(aliasfoo)"), "int aliasfoo = 1\nprint(aliasfoo)");
    }

    #[test]
    fn 行数严格不变() {
        let src = "alias a = 1\nprint(a)\nprint(2)\nprint(3)";
        assert_eq!(ex(src).lines().count(), src.lines().count());
    }}
