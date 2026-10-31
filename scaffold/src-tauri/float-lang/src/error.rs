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
        if self.span.line > 0 {
            write!(f, "第 {} 行：{}", self.span.line, self.message)
        } else {
            write!(f, "{}", self.message)
        }
    }
}

impl std::error::Error for FloatError {}

pub type FResult<T> = Result<T, FloatError>;
