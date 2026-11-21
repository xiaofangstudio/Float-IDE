//! 语言运行上限集中定义。
//!
//! 当前策略：**不设人为上限**。递归、容器、字符串、输出、执行步数都不再拦截；
//! 计算要跑多久就跑多久，内存不够时由操作系统/进程自己处理，语言层不主动中止。
//! 这里保留常量和 guard 函数只是为了兼容所有调用点，守卫本身恒为通过。

pub const CALL_DEPTH: usize = usize::MAX;
pub const INT_CALL_DEPTH: usize = usize::MAX;
pub const IMPORT_DEPTH: usize = usize::MAX;
pub const NEST: usize = usize::MAX;
pub const MAX_OUTPUT: usize = usize::MAX;
pub const MAX_LIST_ITEMS: usize = usize::MAX;
pub const MAX_DICT_ITEMS: usize = usize::MAX;
pub const MAX_STRING_BYTES: usize = usize::MAX;

pub fn grow_error(what: &str, cur: usize, max: usize) -> String {
    format!("{} 太大（当前 {}，上限 {}）", what, cur, max)
}

pub fn guard_list(_cur: usize, _add: usize) -> Result<(), String> {
    Ok(())
}

pub fn guard_dict(_cur: usize, _add: usize) -> Result<(), String> {
    Ok(())
}

pub fn guard_string(_cur: usize, _add: usize) -> Result<(), String> {
    Ok(())
}
