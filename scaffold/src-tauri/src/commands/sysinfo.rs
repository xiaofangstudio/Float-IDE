//! 进程自身的内存占用：状态栏右下角那个"多少 MB"用的。
//!
//! 只读当前进程的工作集（Working Set），不做任何统计/采样，调用开销可以忽略。

/// 当前进程占用的物理内存（字节）。非 Windows 上返回 0（前端会隐藏这一格）。
#[tauri::command]
pub fn sysinfo_mem() -> u64 {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
        use windows_sys::Win32::System::Threading::GetCurrentProcess;
        unsafe {
            let mut pmc: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
            pmc.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
            if GetProcessMemoryInfo(GetCurrentProcess(), &mut pmc, pmc.cb) != 0 {
                return pmc.WorkingSetSize as u64;
            }
            0
        }
    }
    #[cfg(not(windows))]
    {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::sysinfo_mem;

    #[test]
    fn 能读到自己的内存占用() {
        let b = sysinfo_mem();
        // Windows 上一定读得到，而且肯定大于 1MB（光是 exe + 运行时就不止）
        #[cfg(windows)]
        assert!(b > 1024 * 1024, "内存读出来是 {} 字节，不对", b);
        println!("当前进程占用 {} MB", b / 1048576);
    }
}
