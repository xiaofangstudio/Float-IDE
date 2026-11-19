#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    /* 无界面 MCP 模式：
     *   floatide.exe --mcp 17871 --project D:\proj
     * 给命令行里的 AI 直接接一个工程用，不用先把窗口点开。 */
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--mcp") {
        let port = args
            .get(i + 1)
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(17871);
        let project = args
            .iter()
            .position(|a| a == "--project")
            .and_then(|j| args.get(j + 1).cloned())
            .or_else(|| std::env::var("COPPER_MCP_PROJECT").ok())
            .unwrap_or_else(|| {
                std::env::current_dir()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default()
            });
        if let Err(e) = floatide_lib::commands::mcp::serve_headless(port, project) {
            eprintln!("[MCP] 启动失败：{e}");
            std::process::exit(1);
        }
        return;
    }
    floatide_lib::run();
}
