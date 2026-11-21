pub mod fsx;
pub mod host;
pub mod sysinfo;
pub mod lang;
pub mod project;
pub mod terminal;
pub mod git;
pub mod plugins;
pub mod mcp;
pub mod watch;
pub mod assoc;
pub mod copper_env;

/// 从 GUI 进程启动控制台子程序时，禁止 Windows 为它新建控制台窗口。
/// 只影响窗口分配；stdin/stdout/stderr 的管道重定向不受影响，
/// 所以 AOT 的 `ask` 照旧通过 IDE 的输入框接答案。
pub fn hide_console_window(cmd: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        let _ = cmd;
    }
}
