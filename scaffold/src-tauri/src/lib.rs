mod commands;
pub mod startup;

use commands::terminal::PtyState;
use tauri::Manager;

pub struct AppState {
    pub ptys: PtyState,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
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
