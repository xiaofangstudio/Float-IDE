//! 启动参数：双击 .cw 文件、或把文件拖到 exe 上时，Windows 会把路径作为
//! 第一个参数传进来。这里只负责"记住它"，具体开哪个工程由前端决定。

use std::path::PathBuf;
use std::sync::OnceLock;

static STARTUP_PATH: OnceLock<Option<String>> = OnceLock::new();

/// 解析命令行里的第一个真实存在的路径（文件或文件夹都算）
fn parse() -> Option<String> {
    for arg in std::env::args().skip(1) {
        // 跳过 Tauri / WebView 自己的开关（--xxx）
        if arg.starts_with('-') {
            continue;
        }
        let p = PathBuf::from(&arg);
        if p.exists() {
            return Some(p.to_string_lossy().to_string());
        }
    }
    None
}

/// 取启动路径（只解析一次，之后返回缓存）
pub fn path() -> Option<String> {
    STARTUP_PATH.get_or_init(parse).clone()
}

/// 前端把文件开完之后调一下，避免下次刷新又弹一遍
pub fn consume() -> Option<String> {
    let v = path();
    v
}
