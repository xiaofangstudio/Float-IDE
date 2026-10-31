//! 语法树。
//!
//! 与 `src/lang/ast.ts` 对齐：表达式与语句分开，每个节点都带 Span（行/列），
//! 供出错定位、断点行号、后续 IDE 诊断使用。
//!
//! 与 TS 版的差异：Rust 版把节点做成 enum，不再用字符串打 tag，
//! 所以 VM 里 match 是编译期穷尽的——新增语句种类时编译器会逼你处理掉。

use crate::error::Span;
use crate::TypeName;
use std::rc::Rc;

#[derive(Clone, Debug)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

impl Expr {
    pub fn new(kind: ExprKind, span: Span) -> Expr {
        Expr { kind, span }
    }
}

#[derive(Clone, Debug)]
pub enum ExprKind {
    Num(f64),
    Str(Rc<str>),
    Bool(bool),
    Null,
    Var(Rc<str>),
    SelfRef,
    List(Vec<Expr>),
    Dict(Vec<(Expr, Expr)>),
    Un { op: Op, a: Box<Expr> },
    Bin { op: Op, a: Box<Expr>, b: Box<Expr> },
    Idx { obj: Box<Expr>, index: Box<Expr> },
    Member { obj: Box<Expr>, name: Rc<str> },
    Call { callee: Box<Expr>, args: Vec<Expr> },
    New { class: Rc<str>, args: Vec<Expr> },
}

/// 运算符是枚举而不是字符串：VM 里 match 一次就够，不用逐字符比较。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    Neg,
    Not,
    BitNot,
}

impl Op {
    pub fn from_str(s: &str) -> Option<Op> {
        Some(match s {
            "+" => Op::Add,
            "-" => Op::Sub,
            "*" => Op::Mul,
            "/" => Op::Div,
            "%" => Op::Mod,
            "**" => Op::Pow,
            "==" | "===" => Op::Eq,
            "!=" | "!==" => Op::Ne,
            "<" => Op::Lt,
            "<=" => Op::Le,
            ">" => Op::Gt,
            ">=" => Op::Ge,
            "&&" => Op::And,
            "||" => Op::Or,
            "&" => Op::BitAnd,
            "|" => Op::BitOr,
            "^" => Op::BitXor,
            "<<" => Op::Shl,
            ">>" => Op::Shr,
            _ => return None,
        })
    }

    pub fn negate(self) -> Op {
        match self {
            Op::Eq => Op::Ne,
            Op::Ne => Op::Eq,
            Op::Lt => Op::Ge,
            Op::Ge => Op::Lt,
            Op::Gt => Op::Le,
            Op::Le => Op::Gt,
            other => other,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AssignOp {
    Set,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
}

impl AssignOp {
    pub fn from_str(s: &str) -> Option<AssignOp> {
        Some(match s {
            "=" => AssignOp::Set,
            "+=" => AssignOp::Add,
            "-=" => AssignOp::Sub,
            "*=" => AssignOp::Mul,
            "/=" => AssignOp::Div,
            "%=" => AssignOp::Mod,
            "**=" => AssignOp::Pow,
            _ => return None,
        })
    }

    /// 复合赋值的算术部分
    pub fn to_op(self) -> Option<Op> {
        Some(match self {
            AssignOp::Set => return None,
            AssignOp::Add => Op::Add,
            AssignOp::Sub => Op::Sub,
            AssignOp::Mul => Op::Mul,
            AssignOp::Div => Op::Div,
            AssignOp::Mod => Op::Mod,
            AssignOp::Pow => Op::Pow,
        })
    }
}

#[derive(Clone, Debug)]
pub enum Target {
    Var(Rc<str>),
    Member(Expr, Rc<str>),
    Idx(Expr, Expr),
}

#[derive(Clone, Debug)]
pub struct Param {
    pub name: Rc<str>,
    pub ty: Option<TypeName>,
    pub default: Option<Expr>,
    pub line: u32,
    pub col: u32,
}

#[derive(Clone, Debug)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

impl Stmt {
    pub fn new(kind: StmtKind, span: Span) -> Stmt {
        Stmt { kind, span }
    }
}

#[derive(Clone, Debug)]
pub enum StmtKind {
    Decl {
        ty: TypeName,
        name: Rc<str>,
        init: Option<Expr>,
    },
    Assign {
        target: Target,
        op: AssignOp,
        value: Expr,
    },
    IncDec {
        target: Target,
        delta: i32,
        prefix: bool,
    },
    ExprStmt(Expr),
    Print(Vec<Expr>),
    /// 报告：写法跟 print 一样，但走「报告」通道（IDE 右侧面板），不进控制台
    Report(Vec<Expr>),
    /// ask "问题" —— 把回答写进 ans / __last_ans
    Ask(Option<Expr>),
    If {
        /// (条件, 块)。elif 也走这里；开头 else 时条件为 None
        branches: Vec<(Option<Expr>, Vec<Stmt>)>,
        else_body: Option<Vec<Stmt>>,
    },
    While {
        cond: Expr,
        body: Vec<Stmt>,
    },
    Repeat {
        count: Expr,
        body: Vec<Stmt>,
    },
    ForRange {
        var: Rc<str>,
        from: Option<Expr>,
        to: Expr,
        step: Option<Expr>,
        inclusive: bool,
        body: Vec<Stmt>,
    },
    ForIn {
        var: Rc<str>,
        value_name: Option<Rc<str>>,
        iter: Expr,
        body: Vec<Stmt>,
    },
    /// 解构赋值：a, b = f()
    Destructure {
        names: Vec<Rc<str>>,
        value: Expr,
    },
    Break,
    Continue,
    Stop,
    Return(Option<Expr>),
    FuncDef {
        name: Rc<str>,
        params: Vec<Param>,
        ret: Option<Rc<str>>,
        body: Vec<Stmt>,
    },
    /// 只有注释 / 空行，什么都不做
    Nop,
    /// 语法上认得、但这版运行时还没实现（如 class / try / import）
    Unsupported {
        what: String,
    },
}

/// 一段编译好的程序
#[derive(Clone, Debug)]
pub struct Program {
    pub file: Rc<str>,
    pub body: Vec<Stmt>,
    /// 编译期的硬错误（词法/语法）；非空时不应该拿去执行
    pub errors: Vec<crate::error::FloatError>,
}
