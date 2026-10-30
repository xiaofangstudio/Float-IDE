use serde::Serialize;
use std::process::Command;

/* Git 集成：直接调用系统 git.exe。前端负责解析与编排，
 * Rust 只保证"在工程目录里把命令跑起来、把输出原样带回来"。 */

#[derive(Serialize)]
pub struct GitOut {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

#[tauri::command]
pub fn git_available() -> bool {
    Command::new("git").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
}

#[tauri::command]
pub fn git_run(dir: String, args: Vec<String>) -> Result<GitOut, String> {
    if args.is_empty() {
        return Err("git 参数为空".into());
    }
    let mut c = Command::new("git");
    c.current_dir(&dir);
    for a in &args {
        c.arg(a);
    }
    // 关闭分页器与凭据交互弹窗，输出一律走管道
    c.env("GIT_PAGER", "cat");
    c.env("GIT_TERMINAL_PROMPT", "0");
    c.env("GIT_EDITOR", "echo");
    let out = c.output().map_err(|e| format!("git 启动失败（未安装或不在 PATH）：{}", e))?;
    Ok(GitOut {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}
