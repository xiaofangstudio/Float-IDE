mod commands;
pub mod startup;

use commands::terminal::PtyState;
use tauri::Manager;

pub struct AppState {
    pub ptys: PtyState,
}

/// 崩溃落盘：Rust 侧 panic 时把时间和调用栈写进 %APPDATA%\Float\crash.log。
///
/// 为什么要有这个：以前一旦后端 panic，用户看到的就是"界面不动了/一片黑"，
/// 我们这边连一行线索都拿不到，只能猜。现在至少有据可查。
fn install_crash_log() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let mut path = std::env::var("APPDATA").map(std::path::PathBuf::from).unwrap_or_default();
        path.push("Float");
        let _ = std::fs::create_dir_all(&path);
        path.push("crash.log");
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let text = format!(
            "\n===== {} =====\n{}\n(位置: {:?})\n",
            stamp,
            info,
            info.location()
        );
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            let _ = f.write_all(text.as_bytes());
        }
        prev(info);
    }));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    install_crash_log();
    tauri::Builder::default()
        .manage(AppState { ptys: PtyState::new() })
        .manage(commands::mcp::McpState::new())
        .invoke_handler(tauri::generate_handler![
            // 文件系统
            commands::fsx::fs_list,
            commands::fsx::fs_read,
            commands::fsx::fs_write,
            commands::fsx::fs_write_checked,
            commands::fsx::fs_stat,
            commands::fsx::project_lock_acquire,
            commands::fsx::project_lock_touch,
            commands::fsx::project_lock_release,
            commands::fsx::fs_mkdir,
            commands::fsx::fs_rename,
            commands::fsx::fs_delete,
            // 工程
            commands::project::project_new,
            commands::project::recent_get,
            commands::project::recent_push,
            commands::project::recent_remove,
            commands::lang::lang_run,
            commands::lang::lang_check,
            commands::project::pick_folder,
            commands::project::pick_open_file,
            commands::project::startup_path,
            commands::project::pick_save_file,
            commands::project::pick_image,
            commands::project::project_export_zip,
            commands::project::project_import_zip,
            commands::project::open_path,
            commands::project::detect_env,
            // 终端
            commands::terminal::pty_create,
            commands::terminal::pty_write,
            commands::terminal::pty_resize,
            commands::terminal::pty_close,
            // 插件
            commands::plugins::plugin_root_dir,
            commands::plugins::plugin_scan,
            commands::plugins::plugin_read_file,
            commands::plugins::plugin_write_file,
            commands::plugins::plugin_install_dir,
            commands::plugins::plugin_uninstall,
            commands::plugins::plugin_reveal,
            commands::plugins::plugin_state_path,
            commands::plugins::plugin_registry,
            // MCP
            commands::mcp::mcp_start,
            commands::mcp::mcp_stop,
            commands::mcp::mcp_status,
            commands::mcp::mcp_log,
            commands::mcp::mcp_reply,
            // Git
            commands::git::git_available,
            commands::git::git_run,
        ])
        .setup(|app| {
            let win = app.get_webview_window("main").expect("主窗口缺失");
            win.set_title("Float IDE").ok();
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
