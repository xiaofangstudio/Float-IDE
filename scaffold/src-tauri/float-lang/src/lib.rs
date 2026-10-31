//! CW Script 原生语言核心。
//!
//! 这是一份**从零用 Rust 实现**的语言运行时，目标是顶替原来跑在 V8 上的
//! TypeScript 解释器 + 「AST 转 JS」编译通道：
//!
//! * 不再依赖 JS：拥有自己的值模型、自己的堆、自己的调用栈
//! * 字节码虚拟机：不是树遍历，指令有紧凑编码，为后续 JIT / 指针腾出空间
//! * 一份核心两种形态：native（Tauri 里跑）+ wasm（浏览器 demo 里跑）
//!
//! 分层（自下而上）：
//!   token   词法
//!   ast     语法树
//!   parser  递归下降语法分析（行式 + 缩进块）
//!   value   值模型
//!   bytecode 字节码 + 编译器
//!   vm      虚拟机
//!   builtins 内建函数

pub mod api;
pub mod ast;
pub mod builtins;
pub mod bytecode;
pub mod error;
pub mod packages;
pub mod parser;
pub mod sema;
pub mod token;
pub mod value;
pub mod vm;

pub use error::{FloatError, Span};
pub use parser::parse_program;
pub use value::Value;
pub use vm::{RunOutcome, Vm};
pub use api::{run_source, run_text};

/// 语义里明确支持的类型标注词。目前是"装饰性"的（和 TS 版一致），
/// 后续做渐进类型时这里会变成真的检查入口。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TypeName {
    Int,
    Float,
    Str,
    Bool,
    List,
    Dict,
    /// 指针：手动分配的格子
    Ptr,
    Var,
    /// 用户自定义类名等，暂时不检查
    Named,
}

impl TypeName {
    pub fn from_word(w: &str) -> Option<TypeName> {
        Some(match w {
            "int" => TypeName::Int,
            "float" => TypeName::Float,
            "str" => TypeName::Str,
            "bool" => TypeName::Bool,
            "list" => TypeName::List,
            "dict" => TypeName::Dict,
            "ptr" => TypeName::Ptr,
            "var" => TypeName::Var,
            _ => return None,
        })
    }
}
