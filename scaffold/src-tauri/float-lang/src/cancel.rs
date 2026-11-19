//! 运行取消：界面按「停止」时，让正在跑的 VM 主动退出。
//!
//! 为什么不用杀线程：VM 跑在带着 64 MB 栈的 Rust 线程里，硬杀会留下半截状态，
//! 而且脚本可能有写文件这类副作用。这里只放一个原子标志，VM 在循环回跳的
//! fuel 检查点上看一眼——最多几千条指令就能停下来。

use std::sync::atomic::{AtomicBool, Ordering};

static STOP: AtomicBool = AtomicBool::new(false);

/// 请求停止当前运行（下一次 fuel 检查点生效）
pub fn request_stop() {
    STOP.store(true, Ordering::Relaxed);
}

/// 清掉停止标记：每次新运行开始时调用，否则上一轮的停止会误伤这一轮
pub fn clear_stop() {
    STOP.store(false, Ordering::Relaxed);
}

pub fn stop_requested() -> bool {
    STOP.load(Ordering::Relaxed)
}
