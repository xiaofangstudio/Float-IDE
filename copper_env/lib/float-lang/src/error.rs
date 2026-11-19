//! 错误与位置。

use std::fmt;
use std::rc::Rc;

#[derive(Clone, Debug)]
pub struct Span {
    pub file: Rc<str>,
    pub line: u32,
    pub col: u32,
    pub end_line: u32,
    pub end_col: u32,
}

impl Span {
    pub fn new(file: &Rc<str>, line: u32, col: u32, end_col: u32) -> Span {
        Span { file: Rc::clone(file), line, col, end_line: line, end_col }
    }

    pub fn point(file: &Rc<str>, line: u32, col: u32) -> Span {
        Span { file: Rc::clone(file), line, col, end_line: line, end_col: col + 1 }
    }
}

/// 语言的统一错误类型。带位置的错都转成它，好往上抛、也好给 IDE 出诊断。
#[derive(Clone, Debug)]
pub struct FloatError {
    pub message: String,
    pub span: Span,
    /// 运行时错误额外带一个调用栈（最内层在前）
    pub trace: Vec<(Rc<str>, u32)>,
}

impl FloatError {
    pub fn new(message: impl Into<String>, span: Span) -> FloatError {
        FloatError { message: message.into(), span, trace: Vec::new() }
    }

    /// 加一帧调用栈。VM 从内往外冒泡时会一路 push。
    pub fn with_frame(mut self, file: &Rc<str>, line: u32) -> FloatError {
        self.trace.push((Rc::clone(file), line));
        self
    }
}

impl fmt::Display for FloatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // 彩蛋：第 27 行报错，吉祥物廿七（三脚橘猫，正好三条腿）就在这行瘸了。
        // 这行不再走「第 N 行：」前缀，喵那一句本身就是行号。
        if self.span.line == 27 {
            let m = self.message.strip_prefix("第 27 行：").unwrap_or(&self.message);
            return write!(f, "喵～第 27 行瘸了…：{}", m);
        }
        // 运行时错误在 err_at 里就已经带上「第 N 行：」了，这里不能再加一遍
        let already = self.message.starts_with("第 ") && self.message.contains(" 行：");
        if self.span.line > 0 && !already {
            write!(f, "第 {} 行：{}", self.span.line, self.message)
        } else {
            write!(f, "{}", self.message)
        }
    }
}

impl std::error::Error for FloatError {}

pub type FResult<T> = Result<T, FloatError>;

#[cfg(test)]
mod tests {
    use super::*;

    fn err_at(line: u32, message: &str) -> FloatError {
        FloatError::new(message, Span::new(&Rc::from("main.co"), line, 1, 1))
    }

    #[test]
    fn 普通行照常报行号() {
        assert_eq!(err_at(26, "除数不能为 0").to_string(), "第 26 行：除数不能为 0");
    }

    #[test]
    fn 第27行报错变成喵() {
        assert_eq!(err_at(27, "除数不能为 0").to_string(), "喵～第 27 行瘸了…：除数不能为 0");
        // err_at 已经带了「第 27 行：」前缀时不要重复
        assert_eq!(err_at(27, "第 27 行：数组越界").to_string(), "喵～第 27 行瘸了…：数组越界");
    }

    #[test]
    fn 第28行不触发彩蛋() {
        assert_eq!(err_at(28, "除数不能为 0").to_string(), "第 28 行：除数不能为 0");
    }
}
