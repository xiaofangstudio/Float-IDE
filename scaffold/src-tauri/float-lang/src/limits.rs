//! 语言运行的全部上限，集中在一个地方（与前端 src/lang/limits.ts 一一对应）。
//! 报错信息统一用「当前 X（上限 Y）」格式，让用户知道卡在哪条线上。

/// 函数调用（递归）最大深度。超了报「递归太深」，不让 Rust 爆栈。
pub const CALL_DEPTH: usize = 400;

/// import 最大嵌套层数。链子太长多半是库互相引用，该报错而不是吃栈。
pub const IMPORT_DEPTH: usize = 100;

/// 缩进块嵌套上限。正常人写代码很少超过 30 层；给了 200 层余量，
/// 真超了多半是生成的代码或者复制粘贴出了问题。
pub const NEST: usize = 200;

/// 单次运行最多缓存多少输出（字节）。死循环里不停 print 会把内存吃光，
/// 到线就明确停下来报错，而不是把整台机器拖死。
pub const MAX_OUTPUT: usize = 32 * 1024 * 1024;

/* ── 内存护栏 ──────────────────────────────────────────────────────────────
 * 原来只有 MAX_OUTPUT 拦 print 死循环；`while true { add 1 to xs }` 会一路
 * 把列表撑到几个 G，整机开始 swap（实测把资源管理器都拖崩过）。
 * 给容器和字符串划一条线：到线明确报错，而不是偷偷把内存吃光。
 * 这些常量/函数被 VM、builtins 和 AOT 生成程序共用（AOT prelude 直接 pub use
 * 了 float_lang::vm::arith / builtins::list_method），所以一处改动三处生效。 */

/// 单个列表最多多少项（100 万个 Value 约几十 MB，正常脚本够用）。
pub const MAX_LIST_ITEMS: usize = 1_000_000;
/// 单个字典最多多少个键（Map 每项比列表重，线画低一点）。
pub const MAX_DICT_ITEMS: usize = 500_000;
/// 单个字符串最多多少字节（和 MAX_OUTPUT 32MB 呼应，留一半余量）。
pub const MAX_STRING_BYTES: usize = 16 * 1024 * 1024;

/// 统一措辞：让用户知道卡在哪条线上、还差多少。
pub fn grow_error(what: &str, cur: usize, max: usize) -> String {
    format!(
        "{} 太大（当前 {}，上限 {}），是不是循环里忘了停？",
        what, cur, max
    )
}

pub fn guard_list(cur: usize, add: usize) -> Result<(), String> {
    let n = cur.saturating_add(add);
    if n > MAX_LIST_ITEMS { Err(grow_error("列表", n, MAX_LIST_ITEMS)) } else { Ok(()) }
}

pub fn guard_dict(cur: usize, add: usize) -> Result<(), String> {
    let n = cur.saturating_add(add);
    if n > MAX_DICT_ITEMS { Err(grow_error("字典", n, MAX_DICT_ITEMS)) } else { Ok(()) }
}

pub fn guard_string(cur: usize, add: usize) -> Result<(), String> {
    let n = cur.saturating_add(add);
    if n > MAX_STRING_BYTES { Err(grow_error("字符串", n, MAX_STRING_BYTES)) } else { Ok(()) }
}
