use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde::Serialize;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, State};

/* 真·终端：ConPTY 子进程 + 输出流经事件推给 xterm.js。
 * 每个终端一个 id，前端多开终端互不串流。 */

pub struct PtyEntry {
    pub master: Box<dyn portable_pty::MasterPty + Send>,
    pub writer: Box<dyn Write + Send>,
    pub child: Box<dyn portable_pty::Child + Send + Sync>,
}

pub struct PtyState {
    pub map: Mutex<HashMap<String, PtyEntry>>,
}

impl PtyState {
    pub fn new() -> Self {
        Self { map: Mutex::new(HashMap::new()) }
    }
}

#[derive(Serialize, Clone)]
pub struct PtyOutput {
    pub id: String,
    pub data: String, // base64(原始字节)，避免 UTF-8 在块边界被截断
}

#[derive(Serialize, Clone)]
pub struct PtyExit {
    pub id: String,
}

fn shell_name() -> String {
    // 有 PowerShell 用 PowerShell（ANSI 支持好），否则退回 cmd
    if std::env::var("SystemRoot")
        .map(|p| std::path::Path::new(&p).join("System32\\WindowsPowerShell\\v1.0\\powershell.exe").is_file())
        .unwrap_or(true)
    {
        "powershell.exe".into()
    } else {
        "cmd.exe".into()
    }
}

use crate::AppState;

#[tauri::command]
pub fn pty_create(
    app: AppHandle,
    state: State<AppState>,
    id: String,
    cwd: String,
    rows: u16,
    cols: u16,
) -> Result<(), String> {
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize { rows: rows.max(2), cols: cols.max(8), pixel_width: 0, pixel_height: 0 })
        .map_err(|e| format!("打开终端失败：{}", e))?;

    let mut cmd = CommandBuilder::new(shell_name());
    cmd.arg("-NoLogo");
    if std::path::Path::new(&cwd).is_dir() {
        cmd.cwd(&cwd);
    }

    let child = pair.slave.spawn_command(cmd).map_err(|e| format!("启动 shell 失败：{}", e))?;
    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;

    // 从属端使命完成，立即释放（关闭后 stdout 不再丢输出）
    drop(pair.slave);

    let app2 = app.clone();
    let id2 = id.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let payload = PtyOutput {
                        id: id2.clone(),
                        data: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &buf[..n]),
                    };
                    let _ = app2.emit("pty-output", payload);
                }
                Err(_) => break,
            }
        }
        let _ = app2.emit("pty-exit", PtyExit { id: id2 });
    });

    state
        .ptys
        .map
        .lock()
        .map_err(|e| e.to_string())?
        .insert(id, PtyEntry { master: pair.master, writer, child });
    Ok(())
}

#[tauri::command]
pub fn pty_write(state: State<AppState>, id: String, data: String) -> Result<(), String> {
    let mut map = state.ptys.map.lock().map_err(|e| e.to_string())?;
    if let Some(e) = map.get_mut(&id) {
        e.writer.write_all(data.as_bytes()).map_err(|e| e.to_string())?;
        e.writer.flush().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn pty_resize(state: State<AppState>, id: String, cols: u16, rows: u16) -> Result<(), String> {
    let map = state.ptys.map.lock().map_err(|e| e.to_string())?;
    if let Some(e) = map.get(&id) {
        e.master
            .resize(PtySize { rows: rows.max(2), cols: cols.max(8), pixel_width: 0, pixel_height: 0 })
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn pty_close(state: State<AppState>, id: String) -> Result<(), String> {
    if let Some(mut e) = state.ptys.map.lock().map_err(|e| e.to_string())?.remove(&id) {
        let _ = e.child.kill();
    }
    Ok(())
}
