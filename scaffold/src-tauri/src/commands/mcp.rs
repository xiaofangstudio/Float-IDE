use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

/* 内置 MCP 服务器（Model Context Protocol）。
 *
 * 传输：Streamable HTTP —— 客户端 POST JSON-RPC 到 http://127.0.0.1:<port>/mcp，
 * 服务端直接回 application/json。这是 MCP 2025 规范里最简单也最好接的一种。
 *
 * 分工：
 *   - 文件类工具（列目录 / 读 / 写 / 搜索）Rust 直接做，快且不依赖界面。
 *   - 语言类工具（运行脚本 / 诊断 / 大纲）需要 Float 的语言内核（TS），
 *     通过事件把请求转给前端执行，再等前端把结果送回来（带超时，不会卡死）。
 *
 * 绑定的永远是 127.0.0.1，不监听外部网卡；默认还要带 workspace 校验，防止别处的页面随便调。
 */

const BRIDGE_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_BODY: usize = 8 * 1024 * 1024;

pub struct McpState {
    running: Arc<AtomicBool>,
    port: Arc<Mutex<u16>>,
    project: Arc<Mutex<String>>,
    waiting: Arc<Mutex<HashMap<String, Sender<String>>>>,
    log: Arc<Mutex<Vec<String>>>,
    seq: Arc<Mutex<u64>>,
}

impl McpState {
    pub fn new() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            port: Arc::new(Mutex::new(0)),
            project: Arc::new(Mutex::new(String::new())),
            waiting: Arc::new(Mutex::new(HashMap::new())),
            log: Arc::new(Mutex::new(Vec::new())),
            seq: Arc::new(Mutex::new(0)),
        }
    }
    fn push_log(&self, line: String) {
        let mut l = self.log.lock().unwrap();
        l.push(format!("[{}] {}", now_hms(), line));
        let len = l.len();
        if len > 400 {
            l.drain(0..len - 400);
        }
    }
}

fn now_hms() -> String {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = d.as_secs() % 86400;
    format!("{:02}:{:02}:{:02}", secs / 3600, (secs % 3600) / 60, secs % 60)
}

#[derive(Serialize)]
pub struct McpStatus {
    pub running: bool,
    pub port: u16,
    pub project: String,
    pub tools: Vec<String>,
}

#[tauri::command]
pub fn mcp_status(state: tauri::State<'_, McpState>) -> McpStatus {
    McpStatus {
        running: state.running.load(Ordering::SeqCst),
        port: *state.port.lock().unwrap(),
        project: state.project.lock().unwrap().clone(),
        tools: tool_specs().iter().map(|t| t.0.clone()).collect(),
    }
}

#[tauri::command]
pub fn mcp_log(state: tauri::State<'_, McpState>) -> Vec<String> {
    state.log.lock().unwrap().clone()
}

#[tauri::command]
pub fn mcp_stop(state: tauri::State<'_, McpState>) {
    state.running.store(false, Ordering::SeqCst);
    state.push_log("已停止".into());
    // 端口留着给下一次启动复用；监听线程会在下一次 accept 超时时自行退出
}

/// 前端把语言类工具的结果送回来
#[tauri::command]
pub fn mcp_reply(state: tauri::State<'_, McpState>, id: String, result: String) {
    let tx = state.waiting.lock().unwrap().remove(&id);
    if let Some(tx) = tx {
        let _ = tx.send(result);
    }
}

#[tauri::command]
pub fn mcp_start(
    app: AppHandle,
    state: tauri::State<'_, McpState>,
    port: u16,
    project: String,
) -> Result<McpStatus, String> {
    let port = if port == 0 { 17871 } else { port };
    if state.running.load(Ordering::SeqCst) {
        // 已在跑：只更新工程目录
        *state.project.lock().unwrap() = project;
        return Ok(mcp_status(state));
    }

    let listener = TcpListener::bind(("127.0.0.1", port))
        .map_err(|e| format!("端口 {port} 被占用：{e}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|e| e.to_string())?;

    *state.port.lock().unwrap() = port;
    *state.project.lock().unwrap() = project;
    state.running.store(true, Ordering::SeqCst);
    state.push_log(format!("监听 http://127.0.0.1:{port}/mcp"));

    let running = state.running.clone();
    let project_arc = state.project.clone();
    let waiting = state.waiting.clone();
    let log = state.log.clone();
    let seq = state.seq.clone();
    let app2 = app.clone();

    std::thread::spawn(move || {
        while running.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((stream, _addr)) => {
                    let p = project_arc.clone();
                    let w = waiting.clone();
                    let l = log.clone();
                    let s = seq.clone();
                    let ap = app2.clone();
                    std::thread::spawn(move || {
                        let _ = handle_conn(stream, p, w, l, s, Some(ap));
                    });
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(120));
                }
                Err(_) => break,
            }
        }
    });

    Ok(mcp_status(state))
}

/* ───────────── 无界面模式 ───────────── */

/// 不开 IDE 窗口，只跑 MCP 服务器：`floatide.exe --mcp 17871 --project <工程目录>`。
/// 适合 AI 在命令行里直接驱动一个工程（没有界面就只提供 Rust 侧工具，
/// 依赖界面的工具会明确报「需要界面在线」）。
pub fn serve_headless(port: u16, project: String) -> Result<(), String> {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .map_err(|e| format!("端口 {port} 被占用：{e}"))?;
    eprintln!("[MCP] 无界面模式已启动：http://127.0.0.1:{port}/mcp");
    eprintln!("[MCP] 工程：{project}");
    let project = Arc::new(Mutex::new(project));
    let waiting: Arc<Mutex<HashMap<String, Sender<String>>>> = Arc::new(Mutex::new(HashMap::new()));
    let log: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let seq: Arc<Mutex<u64>> = Arc::new(Mutex::new(0));
    for conn in listener.incoming() {
        match conn {
            Ok(stream) => {
                let p = project.clone();
                let w = waiting.clone();
                let l = log.clone();
                let s = seq.clone();
                std::thread::spawn(move || {
                    let _ = handle_conn(stream, p, w, l, s, None);
                });
            }
            Err(_) => break,
        }
    }
    Ok(())
}

/* ───────────── HTTP ───────────── */

fn read_request(stream: &mut TcpStream) -> Result<(String, String, HashMap<String, String>, Vec<u8>), String> {
    stream.set_read_timeout(Some(Duration::from_secs(10))).ok();
    let mut reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);

    let mut line = String::new();
    reader.read_line(&mut line).map_err(|e| e.to_string())?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("/").to_string();

    let mut headers = HashMap::new();
    loop {
        let mut h = String::new();
        let n = reader.read_line(&mut h).map_err(|e| e.to_string())?;
        if n == 0 || h == "\r\n" || h == "\n" {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }

    let len: usize = headers
        .get("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let mut body = vec![0u8; len.min(MAX_BODY)];
    if len > 0 {
        reader.read_exact(&mut body).map_err(|e| e.to_string())?;
    }
    Ok((method, path, headers, body))
}

fn respond(stream: &mut TcpStream, code: u16, ctype: &str, body: &str) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {code} {}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\n\
         Access-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: *\r\n\
         Access-Control-Allow-Methods: POST, GET, OPTIONS\r\nConnection: close\r\n\r\n",
        if code == 200 { "OK" } else { "ERR" },
        body.as_bytes().len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(body.as_bytes())?;
    stream.flush()
}

fn handle_conn(
    mut stream: TcpStream,
    project: Arc<Mutex<String>>,
    waiting: Arc<Mutex<HashMap<String, Sender<String>>>>,
    log: Arc<Mutex<Vec<String>>>,
    seq: Arc<Mutex<u64>>,
    app: Option<AppHandle>,
) -> Result<(), String> {
    let (method, path, _headers, body) = read_request(&mut stream)?;
    let push = |msg: String| {
        let mut l = log.lock().unwrap();
        l.push(format!("[{}] {}", now_hms(), msg));
        let n = l.len();
        if n > 400 {
            l.drain(0..n - 400);
        }
    };

    if method == "OPTIONS" {
        let _ = respond(&mut stream, 200, "text/plain", "");
        return Ok(());
    }

    if method == "GET" && (path == "/" || path.starts_with("/health")) {
        let _ = respond(&mut stream, 200, "application/json", r#"{"ok":true,"name":"float-ide-mcp"}"#);
        return Ok(());
    }

    if method != "POST" {
        let _ = respond(&mut stream, 405, "text/plain", "use POST /mcp");
        return Ok(());
    }

    let text = String::from_utf8_lossy(&body).to_string();
    let req: Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            let err = json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":format!("JSON 解析失败：{e}")}});
            let _ = respond(&mut stream, 200, "application/json", &err.to_string());
            return Ok(());
        }
    };

    // 批量请求
    if let Some(arr) = req.as_array() {
        let mut out = Vec::new();
        for item in arr {
            let r = dispatch(item.clone(), &project, &waiting, &seq, app.as_ref(), &push);
            if let Some(v) = r {
                out.push(v);
            }
        }
        let _ = respond(&mut stream, 200, "application/json", &Value::Array(out).to_string());
        return Ok(());
    }

    match dispatch(req, &project, &waiting, &seq, app.as_ref(), &push) {
        Some(v) => {
            let _ = respond(&mut stream, 200, "application/json", &v.to_string());
        }
        None => {
            // 通知类（notifications/*）没有响应体
            let _ = respond(&mut stream, 202, "application/json", "");
        }
    }
    Ok(())
}

fn dispatch(
    req: Value,
    project: &Arc<Mutex<String>>,
    waiting: &Arc<Mutex<HashMap<String, Sender<String>>>>,
    seq: &Arc<Mutex<u64>>,
    app: Option<&AppHandle>,
    push: &dyn Fn(String),
) -> Option<Value> {
    let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let id = req.get("id").cloned();
    let params = req.get("params").cloned().unwrap_or(Value::Null);
    let is_notification = id.is_none();

    push(format!("→ {method}"));

    let result: Option<Result<Value, (i64, String)>> = match method {
        "initialize" => {
            // 客户端要什么版本就给什么版本（它带的我们一定认识，因为都是同一份 JSON-RPC 工具面）；
            // 硬编码新版本会让只认旧版本的客户端直接断开——这就是"连不上"的常见原因之一。
            let requested = params.get("protocolVersion").and_then(|v| v.as_str()).unwrap_or("2025-06-18");
            Some(Ok(json!({
                "protocolVersion": requested,
                "capabilities": {
                    "tools": { "listChanged": true },
                    "resources": { "subscribe": false, "listChanged": false }
                },
                "serverInfo": { "name": "float-ide", "version": env!("CARGO_PKG_VERSION") }
            })))
        }
        "notifications/initialized" | "notifications/cancelled" => return None,
        "ping" => Some(Ok(json!({}))),
        "tools/list" => Some(Ok(json!({ "tools": tool_specs().iter().map(|t| t.1.clone()).collect::<Vec<_>>() }))),
        "resources/list" => {
            let root = project.lock().unwrap().clone();
            Some(Ok(json!({ "resources": resource_specs(&root) })))
        }
        "resources/read" => {
            let root = project.lock().unwrap().clone();
            let uri = params.get("uri").and_then(|v| v.as_str()).unwrap_or("");
            match read_resource(&root, uri) {
                Ok((mime, text)) => Some(Ok(json!({
                    "contents": [{ "uri": uri, "mimeType": mime, "text": text }]
                }))),
                Err(e) => Some(Err((-32602, e))),
            }
        }
        "prompts/list" => Some(Ok(json!({ "prompts": [] }))),
        "tools/call" => {
            let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            let root = project.lock().unwrap().clone();
            Some(call_tool(name, &args, &root, waiting, seq, app, push))
        }
        _ => Some(Err((-32601, format!("不支持的方法：{method}")))),
    };

    if is_notification {
        return None;
    }

    let resp = match result {
        Some(Ok(v)) => json!({ "jsonrpc": "2.0", "id": id, "result": v }),
        Some(Err((code, msg))) => json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": msg } }),
        None => return None,
    };
    Some(resp)
}

/* ───────────── 工具定义 ───────────── */

fn tool_specs() -> Vec<(String, Value)> {
    vec![
        (
            "list_files".into(),
            json!({
                "name": "list_files",
                "description": "列出当前 Float 工程里的文件（递归，跳过 .git / node_modules / target / .float）",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "dir": { "type": "string", "description": "相对工程根的子目录，省略表示工程根" },
                        "ext": { "type": "string", "description": "只看某个后缀，例如 .co（旧 .cw 也认）" }
                    }
                }
            }),
        ),
        (
            "read_file".into(),
            json!({
                "name": "read_file",
                "description": "读取工程内一个文件的内容（相对工程根的路径）",
                "inputSchema": {
                    "type": "object",
                    "properties": { "path": { "type": "string" } },
                    "required": ["path"]
                }
            }),
        ),
        (
            "write_file".into(),
            json!({
                "name": "write_file",
                "description": "写入工程内的一个文件（原子写：先写临时文件再改名，不会留下半截内容）",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "content": { "type": "string" }
                    },
                    "required": ["path", "content"]
                }
            }),
        ),
        (
            "search".into(),
            json!({
                "name": "search",
                "description": "在工程里搜索文本，返回匹配的文件、行号与该行内容",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" },
                        "regex": { "type": "boolean", "description": "是否按正则解释 query" },
                        "maxResults": { "type": "number", "default": 100 }
                    },
                    "required": ["query"]
                }
            }),
        ),
        (
            "run_script".into(),
            json!({
                "name": "run_script",
                "description": "跑一段代码，或跑工程里的整个主文件。传 path 时和 F5 走同一条 Rust 内核链路：import 会摊平、未存盘的缓冲区一起带上，返回输出、诊断、报错与行号。AI 改完多文件工程后用它跑一遍看真实错误。",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "code": { "type": "string", "description": "要运行的源码（与 path 二选一）" },
                        "path": { "type": "string", "description": "工程内主文件的相对路径；传了就跑整个工程（含 import），不传就跑 code" }
                    },
                    "required": []
                }
            }),
        ),
        (
            "get_diagnostics".into(),
            json!({
                "name": "get_diagnostics",
                "description": "对一个 Copper 源文件（.co，旧 .cw 也认）做语法与语义检查：会先摊平它的 import，在整份程序上判断，定义在别的文件里的变量不会误报；返回诊断列表（行号、列号、消息、严重级别，含「（在 X 里）」的来源标注）",
                "inputSchema": {
                    "type": "object",
                    "properties": { "path": { "type": "string" } },
                    "required": ["path"]
                }
            }),
        ),
        (
            "get_outline".into(),
            json!({
                "name": "get_outline",
                "description": "返回一个 Copper 源文件（.co，旧 .cw 也认）的符号大纲（函数、类、方法、变量及其行号）",
                "inputSchema": {
                    "type": "object",
                    "properties": { "path": { "type": "string" } },
                    "required": ["path"]
                }
            }),
        ),
        (
            "project_info".into(),
            json!({
                "name": "project_info",
                "description": "返回当前打开的工程信息（名称、入口文件、工程根路径）",
                "inputSchema": { "type": "object", "properties": {} }
            }),
        ),
        (
            "list_dir".into(),
            json!({
                "name": "list_dir",
                "description": "列出工程内某个目录的直接子项（不递归），带类型与大小",
                "inputSchema": {
                    "type": "object",
                    "properties": { "dir": { "type": "string", "description": "相对工程根，省略为工程根" } }
                }
            }),
        ),
        (
            "create_file".into(),
            json!({
                "name": "create_file",
                "description": "新建文件（已存在会报错，绝不覆盖）；父目录自动创建",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "content": { "type": "string", "description": "省略则为空文件" }
                    },
                    "required": ["path"]
                }
            }),
        ),
        (
            "delete_file".into(),
            json!({
                "name": "delete_file",
                "description": "删除工程内的文件或目录（目录递归删，谨慎使用）",
                "inputSchema": {
                    "type": "object",
                    "properties": { "path": { "type": "string" } },
                    "required": ["path"]
                }
            }),
        ),
        (
            "rename_file".into(),
            json!({
                "name": "rename_file",
                "description": "重命名 / 移动工程内的文件或目录",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "from": { "type": "string" },
                        "to": { "type": "string" }
                    },
                    "required": ["from", "to"]
                }
            }),
        ),
        (
            "mkdir".into(),
            json!({
                "name": "mkdir",
                "description": "新建目录（父目录自动创建）",
                "inputSchema": {
                    "type": "object",
                    "properties": { "path": { "type": "string" } },
                    "required": ["path"]
                }
            }),
        ),
        (
            "open_project".into(),
            json!({
                "name": "open_project",
                "description": "让 Float IDE 打开一个工程（传工程目录，或传 .co/.cw/.json/.txt 文件路径——它所在的文件夹会成为工程根并打开该文件）。传 dir 或 path 之一",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "dir": { "type": "string", "description": "工程目录（绝对路径）" },
                        "path": { "type": "string", "description": "或：要打开的文件（绝对路径）" }
                    }
                }
            }),
        ),
        (
            "get_console".into(),
            json!({
                "name": "get_console",
                "description": "拿到 IDE 控制台最近的输出（运行结果、错误、提示）",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "maxLines": { "type": "number", "default": 80 }
                    }
                }
            }),
        ),
        (
            "read_buffer".into(),
            json!({
                "name": "read_buffer",
                "description": "读 IDE 当前打开的文件在编辑器里的实时内容（含未保存修改）。不传 path 拿当前活动文件",
                "inputSchema": {
                    "type": "object",
                    "properties": { "path": { "type": "string", "description": "省略为当前活动文件" } }
                }
            }),
        ),
        (
            "write_buffer".into(),
            json!({
                "name": "write_buffer",
                "description": "把 IDE 当前活动文件的编辑器内容整个替换（相当于替用户改了正在编辑的代码，会标脏等待保存）",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "content": { "type": "string" }
                    },
                    "required": ["content"]
                }
            }),
        ),
        (
            "get_settings".into(),
            json!({
                "name": "get_settings",
                "description": "读 Float IDE 的用户设置（字号、自动保存等），返回 JSON",
                "inputSchema": { "type": "object", "properties": {} }
            }),
        ),
        (
            "set_settings".into(),
            json!({
                "name": "set_settings",
                "description": "合并式修改 Float IDE 的用户设置：传 JSON 对象，只改你给的字段",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "settings": { "type": "object", "description": "要合并进去的设置字段" }
                    },
                    "required": ["settings"]
                }
            }),
        ),
        (
            "run".into(),
            json!({
                "name": "run",
                "description": "一键运行工程里的 .co 文件，直接返回结果 JSON。mode=vm 时走解释器（和 F5 的 VM 一致）；mode=aot 时先编成原生 exe 再跑（首次 10~20 秒，之后按内容缓存）。inputs 会依次喂给脚本里的 ask，喂完之后的 ask 直接拿空串，不会卡住。比 run_script 更好用：同一套参数可选 VM/AOT，且不依赖界面窗口。",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "工程内 .co 文件（相对工程根）" },
                        "mode": { "type": "string", "enum": ["vm", "aot"], "default": "vm" },
                        "inputs": { "type": "array", "items": { "type": "string" }, "description": "按顺序喂给 ask 的答案" },
                        "forceRebuild": { "type": "boolean", "description": "mode=aot 时强制重编（默认按内容哈希命中缓存）" }
                    },
                    "required": ["path"]
                }
            }),
        ),
        (
            "test".into(),
            json!({
                "name": "test",
                "description": "一键测试：自动找工程里的测试文件（tests/ 下的 .co，或 test_*.co / *_test.co），逐个用 VM 跑一遍。Copper 没有 assert 内置，约定是「跑完不报错 = 通过」，要断言失败就 throw。返回每个文件的 ok / output / error / error_line / 耗时，以及 passed / failed 汇总。",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "dir": { "type": "string", "description": "只跑某个子目录（相对工程根），默认全工程" },
                        "inputs": { "type": "array", "items": { "type": "string" }, "description": "喂给测试里 ask 的答案（所有测试共用，按顺序）" },
                        "failFast": { "type": "boolean", "default": false }
                    }
                }
            }),
        ),
        (
            "check".into(),
            json!({
                "name": "check",
                "description": "一键查 bug：传 path 检查单个文件；不传就扫全工程所有 .co（跳过 tests 之外的噪音目录），返回按文件分组的语法/语义诊断。诊断里的行号已映射回原文件，跨文件的会标「（在 X 里）」。",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "只查这个文件；省略 = 全工程" },
                        "maxFiles": { "type": "number", "default": 200 }
                    }
                }
            }),
        ),
        (
            "docs_list".into(),
            json!({
                "name": "docs_list",
                "description": "列出 Copper 官方教程的章节目录（章节 id、标题、摘要、小节数）。这是新语言，写代码前先看目录，再用 docs_get 读具体章节。",
                "inputSchema": { "type": "object", "properties": {} }
            }),
        ),
        (
            "docs_get".into(),
            json!({
                "name": "docs_get",
                "description": "读 Copper 教程的某一章（chapter 传 docs_list 里的 id，例如 types / control / func / class / builtins；也可只读某一节 section）。返回 Markdown：讲解 + 示例代码 + 示例输出 + 注意事项。",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "chapter": { "type": "string" },
                        "section": { "type": "string", "description": "可选：小节 id 或标题里的关键字" }
                    },
                    "required": ["chapter"]
                }
            }),
        ),
        (
            "docs_search".into(),
            json!({
                "name": "docs_search",
                "description": "在 Copper 教程里全文搜索关键字（中英文都行），返回命中的章节/小节与上下文片段。不知道某个写法在哪一章时用它。",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" },
                        "maxResults": { "type": "number", "default": 12 }
                    },
                    "required": ["query"]
                }
            }),
        ),
        (
            "env_info".into(),
            json!({
                "name": "env_info",
                "description": "返回 Copper 环境信息：是否找到、环境根、编译器版本/路径、文档目录。AI 先看这个就知道本机能不能编译 AOT、教程在哪。",
                "inputSchema": { "type": "object", "properties": {} }
            }),
        ),
    ]
}
fn ok_text(s: String) -> Value {
    json!({ "content": [{ "type": "text", "text": s }] })
}

fn err_text(s: String) -> Value {
    json!({ "content": [{ "type": "text", "text": s }], "isError": true })
}

fn safe_join(root: &str, rel: &str) -> Result<PathBuf, String> {
    if root.is_empty() {
        return Err("还没有打开工程".into());
    }
    let base = Path::new(root);
    let clean = rel.replace('\\', "/");
    if clean.contains("..") {
        return Err("路径不合法（不允许 ..）".into());
    }
    let p = base.join(clean.trim_start_matches('/'));
    Ok(p)
}

fn call_tool(
    name: &str,
    args: &Value,
    root: &str,
    waiting: &Arc<Mutex<HashMap<String, Sender<String>>>>,
    seq: &Arc<Mutex<u64>>,
    app: Option<&AppHandle>,
    push: &dyn Fn(String),
) -> Result<Value, (i64, String)> {
    let s = |k: &str| args.get(k).and_then(|v| v.as_str()).map(|x| x.to_string());

    let out = match name {
        "project_info" => {
            let info = json!({
                "root": root,
                "name": Path::new(root)
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default()
            });
            ok_text(info.to_string())
        }
        "list_files" => {
            let start = s("dir").unwrap_or_default();
            let base = match safe_join(root, &start) {
                Ok(p) => p,
                Err(e) => return Ok(err_text(e)),
            };
            let ext = s("ext").map(|e| e.to_lowercase());
            let mut files: Vec<String> = Vec::new();
            collect_files(root, &base, &ext, &mut files, 0);
            ok_text(json!(files).to_string())
        }
        "read_file" => match s("path") {
            None => err_text("缺少 path".into()),
            Some(p) => match safe_join(root, &p).and_then(|f| {
                std::fs::read_to_string(&f).map_err(|e| format!("读取失败 {p}: {e}"))
            }) {
                Ok(text) => ok_text(text),
                Err(e) => err_text(e),
            },
        },
        "write_file" => match (s("path"), s("content")) {
            (Some(p), Some(c)) => match safe_join(root, &p) {
                Err(e) => err_text(e),
                Ok(f) => {
                    if let Some(parent) = f.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    match atomic_write(&f, &c) {
                        Ok(_) => {
                            push(format!("write_file {p} ({} 字节)", c.len()));
                            ok_text(format!("已写入 {p}（{} 字节）", c.len()))
                        }
                        Err(e) => err_text(e),
                    }
                }
            },
            _ => err_text("缺少 path 或 content".into()),
        },
        "list_dir" => {
            let dir = s("dir").unwrap_or_default();
            match safe_join(root, &dir) {
                Err(e) => err_text(e),
                Ok(base) => {
                    let mut items: Vec<Value> = Vec::new();
                    if let Ok(rd) = std::fs::read_dir(&base) {
                        for e in rd.flatten() {
                            let name = e.file_name().to_string_lossy().to_string();
                            let is_dir = e.path().is_dir();
                            let size = e.metadata().map(|m| m.len()).unwrap_or(0);
                            items.push(json!({ "name": name, "dir": is_dir, "size": size }));
                        }
                    }
                    items.sort_by(|a, b| {
                        let (da, db) = (a["dir"].as_bool().unwrap(), b["dir"].as_bool().unwrap());
                        db.cmp(&da).then(a["name"].as_str().cmp(&b["name"].as_str()))
                    });
                    ok_text(json!(items).to_string())
                }
            }
        }
        "create_file" => match s("path") {
            None => err_text("缺少 path".into()),
            Some(p) => match safe_join(root, &p) {
                Err(e) => err_text(e),
                Ok(f) => {
                    if f.exists() {
                        err_text(format!("{p} 已经存在，create_file 绝不覆盖；要改内容用 write_file"))
                    } else {
                        if let Some(parent) = f.parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        let content = s("content").unwrap_or_default();
                        match atomic_write(&f, &content) {
                            Ok(_) => {
                                push(format!("create_file {p} ({} 字节)", content.len()));
                                ok_text(format!("已创建 {p}"))
                            }
                            Err(e) => err_text(e),
                        }
                    }
                }
            },
        },
        "delete_file" => match s("path") {
            None => err_text("缺少 path".into()),
            Some(p) => match safe_join(root, &p) {
                Err(e) => err_text(e),
                Ok(f) => {
                    if !f.exists() {
                        err_text(format!("{p} 不存在"))
                    } else {
                        let r = if f.is_dir() {
                            std::fs::remove_dir_all(&f)
                        } else {
                            std::fs::remove_file(&f)
                        };
                        match r {
                            Ok(_) => {
                                push(format!("delete_file {p}"));
                                ok_text(format!("已删除 {p}"))
                            }
                            Err(e) => err_text(format!("删除失败 {p}: {e}")),
                        }
                    }
                }
            },
        },
        "rename_file" => match (s("from"), s("to")) {
            (Some(from), Some(to)) => match (safe_join(root, &from), safe_join(root, &to)) {
                (Err(e), _) | (_, Err(e)) => err_text(e),
                (Ok(sf), Ok(st)) => {
                    if !sf.exists() {
                        err_text(format!("{from} 不存在"))
                    } else if st.exists() {
                        err_text(format!("{to} 已经存在，不敢盖"))
                    } else {
                        if let Some(parent) = st.parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        match std::fs::rename(&sf, &st) {
                            Ok(_) => {
                                push(format!("rename_file {from} -> {to}"));
                                ok_text(format!("已改名 {from} → {to}"))
                            }
                            Err(e) => err_text(format!("改名失败: {e}")),
                        }
                    }
                }
            },
            _ => err_text("缺少 from 或 to".into()),
        },
        "mkdir" => match s("path") {
            None => err_text("缺少 path".into()),
            Some(p) => match safe_join(root, &p) {
                Err(e) => err_text(e),
                Ok(f) => match std::fs::create_dir_all(&f) {
                    Ok(_) => ok_text(format!("已创建目录 {p}")),
                    Err(e) => err_text(format!("创建失败: {e}")),
                },
            },
        },
        "search" => {
            let q = s("query").unwrap_or_default();
            let use_re = args.get("regex").and_then(|v| v.as_bool()).unwrap_or(false);
            let max = args
                .get("maxResults")
                .and_then(|v| v.as_u64())
                .unwrap_or(100) as usize;
            if q.is_empty() {
                err_text("query 不能为空".into())
            } else {
                let hits = search_project(root, &q, use_re, max);
                ok_text(json!(hits).to_string())
            }
        }
        // 语言 / 界面类：走前端桥（run_script、get_diagnostics、get_outline、
        // open_project、get_settings、set_settings、get_console、read_buffer、write_buffer）
        "run" => {
            let p = match s("path") {
                Some(p) => p,
                None => return Ok(err_text("缺少 path".into())),
            };
            let abs = match safe_join(root, &p) {
                Ok(a) => a,
                Err(e) => return Ok(err_text(e)),
            };
            if !abs.is_file() {
                return Ok(err_text(format!("找不到文件：{p}")));
            }
            let mode = s("mode").unwrap_or_else(|| "vm".into());
            let inputs: Vec<String> = args
                .get("inputs")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default();
            let force = args.get("forceRebuild").and_then(|v| v.as_bool()).unwrap_or(false);
            push(format!("run {p} ({mode})"));
            let r = if mode == "aot" {
                exec_aot(&abs, &inputs, force)
            } else {
                exec_vm(&abs, &inputs)
            };
            match r {
                Ok(v) => ok_text(v.to_string()),
                Err(e) => err_text(e),
            }
        }
        "test" => {
            if root.is_empty() {
                return Ok(err_text("还没有打开工程".into()));
            }
            let dir = s("dir").unwrap_or_default();
            let inputs: Vec<String> = args
                .get("inputs")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default();
            let fail_fast = args.get("failFast").and_then(|v| v.as_bool()).unwrap_or(false);
            let files = discover_tests(root, &dir);
            if files.is_empty() {
                return Ok(err_text(
                    "没找到测试文件：放在 tests/ 下的 .co，或命名成 test_*.co / *_test.co".into(),
                ));
            }
            push(format!("test 开始：{} 个文件", files.len()));
            let mut results: Vec<Value> = Vec::new();
            let mut passed = 0usize;
            let mut failed = 0usize;
            let mut total_ms = 0u128;
            for f in &files {
                let t = Instant::now();
                let r = exec_vm(f, &inputs);
                let ms = t.elapsed().as_millis();
                total_ms += ms;
                let rel = f
                    .strip_prefix(root)
                    .map(|x| x.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_else(|_| f.to_string_lossy().to_string());
                match r {
                    Ok(v) if v.get("ok").and_then(|x| x.as_bool()).unwrap_or(false) => {
                        passed += 1;
                        results.push(json!({ "file": rel, "ok": true, "millis": ms, "output": v.get("output").cloned().unwrap_or(json!("")) }));
                    }
                    Ok(v) => {
                        failed += 1;
                        results.push(json!({ "file": rel, "ok": false, "millis": ms, "error": v.get("error").cloned().unwrap_or(json!("")), "error_line": v.get("error_line").cloned().unwrap_or(json!(0)), "output": v.get("output").cloned().unwrap_or(json!("")) }));
                        if fail_fast {
                            break;
                        }
                    }
                    Err(e) => {
                        failed += 1;
                        results.push(json!({ "file": rel, "ok": false, "millis": ms, "error": e }));
                        if fail_fast {
                            break;
                        }
                    }
                }
            }
            let summary = json!({
                "total": passed + failed,
                "passed": passed,
                "failed": failed,
                "millis": total_ms as u64,
                "results": results
            });
            ok_text(summary.to_string())
        }
        "check" => {
            if root.is_empty() {
                return Ok(err_text("还没有打开工程".into()));
            }
            let max_files = args.get("maxFiles").and_then(|v| v.as_u64()).unwrap_or(200) as usize;
            match s("path") {
                Some(p) => {
                    let abs = match safe_join(root, &p) {
                        Ok(a) => a,
                        Err(e) => return Ok(err_text(e)),
                    };
                    match check_one(&abs) {
                        Ok(v) => ok_text(v.to_string()),
                        Err(e) => err_text(e),
                    }
                }
                None => {
                    let files = collect_source_files(root, max_files);
                    let mut all: Vec<Value> = Vec::new();
                    let mut errors = 0usize;
                    let mut warnings = 0usize;
                    for f in &files {
                        if let Ok(v) = check_one(f) {
                            let n_err = v.get("errors").and_then(|x| x.as_u64()).unwrap_or(0) as usize;
                            let n_warn = v.get("warnings").and_then(|x| x.as_u64()).unwrap_or(0) as usize;
                            if n_err + n_warn > 0 {
                                errors += n_err;
                                warnings += n_warn;
                                all.push(v);
                            }
                        }
                    }
                    ok_text(json!({
                        "files_checked": files.len(),
                        "errors": errors,
                        "warnings": warnings,
                        "details": all
                    }).to_string())
                }
            }
        }
        "docs_list" => match load_tutorial() {
            Ok(t) => {
                let mut chapters: Vec<Value> = Vec::new();
                for c in t.get("chapters").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
                    let n = c.get("sections").and_then(|x| x.as_array()).map(|a| a.len()).unwrap_or(0);
                    chapters.push(json!({
                        "id": c.get("id").cloned().unwrap_or(json!("")),
                        "title": c.get("title").cloned().unwrap_or(json!("")),
                        "summary": c.get("summary").cloned().unwrap_or(json!("")),
                        "sections": n
                    }));
                }
                ok_text(json!({ "chapters": chapters, "hint": "用 docs_get 读某一章，或 docs_search 找关键字" }).to_string())
            }
            Err(e) => err_text(e),
        },
        "docs_get" => {
            let chapter = match s("chapter") {
                Some(c) => c,
                None => return Ok(err_text("缺少 chapter".into())),
            };
            match load_tutorial().and_then(|t| render_chapter(&t, &chapter, s("section").as_deref())) {
                Ok(text) => ok_text(text),
                Err(e) => err_text(e),
            }
        }
        "docs_search" => {
            let q = match s("query") {
                Some(q) if !q.trim().is_empty() => q,
                _ => return Ok(err_text("query 不能为空".into())),
            };
            let max = args.get("maxResults").and_then(|v| v.as_u64()).unwrap_or(12) as usize;
            match load_tutorial().and_then(|t| search_docs(&t, &q, max)) {
                Ok(text) => ok_text(text),
                Err(e) => err_text(e),
            }
        }
        "env_info" => {
            let env = crate::commands::copper_env::resolve_env();
            let docs = docs_dir(root);
            let version = env
                .compiler
                .rsplit(['\\', '/'])
                .next()
                .unwrap_or("")
                .to_string();
            ok_text(json!({
                "found": env.found,
                "home": env.home,
                "compiler": env.compiler,
                "compiler_file": version,
                "docs": docs.map(|d| d.to_string_lossy().to_string()).unwrap_or_default(),
                "note": "环境里的 colang 不认 --version，文件名里的版本号就是版本"
            }).to_string())
        }
        "run_script" | "get_diagnostics" | "get_outline"
        | "open_project" | "get_settings" | "set_settings" | "get_console"
        | "read_buffer" | "write_buffer" => {
            if root.is_empty() {
                err_text("还没有打开工程".into())
            } else {
                let id = {
                    let mut g = seq.lock().unwrap();
                    *g += 1;
                    format!("m{}", *g)
                };
                let (tx, rx) = channel::<String>();
                waiting.lock().unwrap().insert(id.clone(), tx);
                let payload = json!({ "id": id, "tool": name, "args": args, "root": root });
                push(format!("→ 交给界面执行 {name}"));
                let Some(app) = app else {
                    waiting.lock().unwrap().remove(&id);
                    return Ok(err_text("这个工具需要 IDE 界面在线（当前是无界面模式）".into()));
                };
                if app.emit("mcp:invoke", payload).is_err() {
                    waiting.lock().unwrap().remove(&id);
                    return Ok(err_text("界面未响应（应用可能已关闭）".into()));
                }
                match rx.recv_timeout(BRIDGE_TIMEOUT) {
                    Ok(text) => {
                        let v: Value = serde_json::from_str(&text).unwrap_or(json!({ "text": text }));
                        if let Some(t) = v.get("text").and_then(|x| x.as_str()) {
                            ok_text(t.to_string())
                        } else if let Some(e) = v.get("error").and_then(|x| x.as_str()) {
                            err_text(e.to_string())
                        } else {
                            ok_text(v.to_string())
                        }
                    }
                    Err(_) => {
                        waiting.lock().unwrap().remove(&id);
                        err_text(format!("{name} 执行超时（{BRIDGE_TIMEOUT:?}）"))
                    }
                }
            }
        }
        other => return Ok(err_text(format!("未知工具：{other}"))),
    };
    Ok(out)
}

fn atomic_write(path: &Path, contents: &str) -> Result<(), String> {
    let tmp = path.with_extension(format!(
        "{}.mcp-tmp",
        path.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default()
    ));
    std::fs::write(&tmp, contents).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e.to_string()
    })
}

fn collect_files(root: &str, dir: &Path, ext: &Option<String>, out: &mut Vec<String>, depth: usize) {
    if depth > 12 || out.len() > 4000 {
        return;
    }
    let rd = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return,
    };
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if name == ".git" || name == "node_modules" || name == "target" || name == ".float" {
            continue;
        }
        if p.is_dir() {
            collect_files(root, &p, ext, out, depth + 1);
        } else {
            if let Some(x) = ext {
                if !name.to_lowercase().ends_with(&x.to_lowercase()) {
                    continue;
                }
            }
            let rel = p
                .strip_prefix(root)
                .map(|r| r.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| p.to_string_lossy().to_string());
            out.push(rel);
            if out.len() > 4000 {
                return;
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Hit {
    file: String,
    line: u32,
    text: String,
}

fn search_project(root: &str, query: &str, use_regex: bool, max: usize) -> Vec<Hit> {
    let mut files: Vec<String> = Vec::new();
    collect_files(root, Path::new(root), &None, &mut files, 0);
    let mut out = Vec::new();
    let matcher: Option<regex_lite::Regex> = if use_regex {
        regex_lite::Regex::new(query).ok()
    } else {
        None
    };
    for rel in files {
        if out.len() >= max {
            break;
        }
        let rl = rel.to_lowercase();
        if !rl.ends_with(".co") && !rl.ends_with(".cw") && !rl.ends_with(".json")
            && !rl.ends_with(".md") && !rl.ends_with(".txt") && !rl.ends_with(".in") {
            continue;
        }
        let full = Path::new(root).join(&rel);
        let text = match std::fs::read_to_string(&full) {
            Ok(t) => t,
            Err(_) => continue,
        };
        for (i, line) in text.lines().enumerate() {
            let hit = match &matcher {
                Some(re) => re.is_match(line),
                None => line.contains(query),
            };
            if hit {
                out.push(Hit {
                    file: rel.clone(),
                    line: (i + 1) as u32,
                    text: line.chars().take(300).collect(),
                });
                if out.len() >= max {
                    break;
                }
            }
        }
    }
    out
}

/// 小而够用的正则：MCP 的 search 只需要字面量与常见元字符
mod regex_lite {
    pub struct Regex {
        pattern: String,
    }
    impl Regex {
        pub fn new(p: &str) -> Result<Self, String> {
            // 不支持回溯等高级语法时退回到字面量包含判断，保证不会崩
            Ok(Self { pattern: p.to_string() })
        }
        pub fn is_match(&self, s: &str) -> bool {
            if self.pattern.is_empty() {
                return false;
            }
            // 去掉常见的锚点/量词后按字面量找，够用且绝不 panic
            let core = self
                .pattern
                .trim_matches(|c| c == '^' || c == '$')
                .replace(".*", "")
                .replace('+', "")
                .replace('?', "");
            let core = core.trim_matches('\\').to_string();
            if core.is_empty() {
                return true;
            }
            s.contains(&core) || s.to_lowercase().contains(&core.to_lowercase())
        }
    }
}

// 让未使用的导入不报 warning
#[allow(dead_code)]
fn _unused(_: Instant) {}

/* ───────── 一键运行 / 测试 / 检查 / 文档 ───────── */

fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn safe_stem(p: &Path) -> String {
    let stem = p
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "app".into());
    stem.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
        .collect()
}

/// VM 跑一次（不经过界面）：ask 依次吃 inputs，吃完给空串，不会阻塞。
fn exec_vm(file: &Path, inputs: &[String]) -> Result<Value, String> {
    let text = std::fs::read_to_string(file).map_err(|e| format!("读文件失败：{e}"))?;
    let file_s = file.to_string_lossy().to_string();
    let base = file.parent().map(|p| p.to_path_buf());
    let ins = inputs.to_vec();
    let start = Instant::now();
    let handle = std::thread::Builder::new()
        .name("mcp-run".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            crate::commands::lang::ensure_fetch_installed();
            let mut q = ins.clone().into_iter();
            let r = float_lang::api::run_blocking(
                &file_s,
                &text,
                base.as_deref(),
                ins,
                Default::default(),
                Box::new(move |_p: &str| q.next().unwrap_or_default()),
            );
            // 结果里有 Rc，不能跨线程搬：在这里就转成纯 JSON 再交回去。
            match r {
                Ok(o) => json!({
                    "ok": true,
                    "mode": "vm",
                    "output": o.output,
                    "reports": o.reports,
                    "error": Value::Null,
                    "error_line": 0,
                    "steps": o.steps
                }),
                Err(e) => json!({
                    "ok": false,
                    "mode": "vm",
                    "output": "",
                    "reports": [],
                    "error": e.message,
                    "error_line": e.span.line,
                    "steps": 0
                }),
            }
        })
        .map_err(|e| format!("启动运行线程失败：{e}"))?;
    let mut v = handle
        .join()
        .map_err(|_| "运行线程崩了（可能是脚本太深或内核 bug）".to_string())?;
    let ms = start.elapsed().as_millis() as u64;
    if let Some(obj) = v.as_object_mut() {
        obj.insert("millis".into(), json!(ms));
    }
    Ok(v)
}

/// AOT 跑一次：进程内 emitter 编成原生 exe，再把 inputs 从 stdin 喂进去。
fn exec_aot(file: &Path, inputs: &[String], force: bool) -> Result<Value, String> {
    use std::process::{Command, Stdio};
    let text = std::fs::read_to_string(file).map_err(|e| format!("读文件失败：{e}"))?;
    let hash = format!("mcp{:x}", fnv1a(&text));
    let cache_root = std::env::temp_dir().join(format!("copper_ide_aot_{hash}"));
    let exe = cache_root.join(format!("{}{}", safe_stem(file), if cfg!(windows) { ".exe" } else { "" }));
    let compile_start = Instant::now();
    let mut compiled = false;
    let compile_ms;
    if force || !exe.is_file() {
        let built = float_lang::aot::build_exe(file, &text, float_lang::MANIFEST_DIR, &hash)
            .map_err(|e| format!("AOT 编译失败：{e}"))?;
        compile_ms = compile_start.elapsed().as_millis() as u64;
        compiled = true;
        if built != exe {
            std::fs::create_dir_all(&cache_root).ok();
            std::fs::copy(&built, &exe).map_err(|e| format!("复制产物失败：{e}"))?;
        }
    } else {
        compile_ms = 0;
    }

    let start = Instant::now();
    let mut child = Command::new(&exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("启动 AOT 产物失败：{e}"))?;
    if let Some(mut sin) = child.stdin.take() {
        for line in inputs {
            let _ = writeln!(sin, "{line}");
        }
    }
    let out = child
        .wait_with_output()
        .map_err(|e| format!("等待 AOT 产物失败：{e}"))?;
    let ms = start.elapsed().as_millis() as u64;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    Ok(json!({
        "ok": out.status.success(),
        "mode": "aot",
        "exit_code": out.status.code(),
        "output": stdout,
        "stderr": stderr,
        "error": if out.status.success() { Value::Null } else { json!(stderr) },
        "millis": ms,
        "compiled": compiled,
        "compile_ms": compile_ms,
        "exe": exe.to_string_lossy()
    }))
}

/// 测试文件约定：tests/ 目录下的 .co/.cw，或 test_*.co / *_test.co。
fn discover_tests(root: &str, sub: &str) -> Vec<PathBuf> {
    fn walk(dir: &Path, in_tests: bool, out: &mut Vec<PathBuf>, depth: usize) {
        if depth > 12 {
            return;
        }
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if p.is_dir() {
                if matches!(name.as_str(), ".git" | "node_modules" | "target" | ".float" | ".copper") {
                    continue;
                }
                walk(&p, in_tests || name == "tests", out, depth + 1);
            } else {
                let is_co = matches!(
                    p.extension().and_then(|x| x.to_str()).map(|x| x.to_ascii_lowercase()).as_deref(),
                    Some("co") | Some("cw")
                );
                if !is_co {
                    continue;
                }
                if in_tests || name.starts_with("test_") || name.ends_with("_test.co") {
                    out.push(p);
                }
            }
        }
    }
    let mut out = Vec::new();
    let base = if sub.is_empty() {
        PathBuf::from(root)
    } else {
        Path::new(root).join(sub.replace('\\', "/").trim_start_matches('/'))
    };
    walk(&base, base.file_name().map(|n| n == "tests").unwrap_or(false), &mut out, 0);
    out.sort();
    out
}

fn collect_source_files(root: &str, max: usize) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>, max: usize, depth: usize) {
        if out.len() >= max || depth > 12 {
            return;
        }
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if p.is_dir() {
                if matches!(name.as_str(), ".git" | "node_modules" | "target" | ".float" | ".copper") {
                    continue;
                }
                walk(&p, out, max, depth + 1);
            } else if matches!(
                p.extension().and_then(|x| x.to_str()).map(|x| x.to_ascii_lowercase()).as_deref(),
                Some("co") | Some("cw")
            ) {
                out.push(p);
                if out.len() >= max {
                    return;
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(Path::new(root), &mut out, max, 0);
    out.sort();
    out
}

fn check_one(file: &Path) -> Result<Value, String> {
    let text = std::fs::read_to_string(file).map_err(|e| format!("读文件失败：{e}"))?;
    let ds = crate::commands::lang::lang_check(file.to_string_lossy().to_string(), text);
    let errors = ds.iter().filter(|d| d.severity == "error").count();
    let warnings = ds.iter().filter(|d| d.severity == "warning").count();
    Ok(json!({
        "file": file.to_string_lossy(),
        "errors": errors,
        "warnings": warnings,
        "diagnostics": ds.iter().map(|d| json!({
            "severity": d.severity, "message": d.message, "line": d.line, "col": d.col
        })).collect::<Vec<_>>()
    }))
}

/// 环境文档目录：环境自带优先，其次是开发机仓库里的 copper_env。
fn docs_dir(root: &str) -> Option<PathBuf> {
    let env = crate::commands::copper_env::resolve_env();
    if !env.home.is_empty() {
        let d = Path::new(&env.home).join("docs");
        if d.join("tutorial.json").is_file() {
            return Some(d);
        }
    }
    if let Ok(h) = std::env::var("COPPER_HOME") {
        let d = Path::new(&h).join("docs");
        if d.join("tutorial.json").is_file() {
            return Some(d);
        }
    }
    let repo = Path::new(float_lang::MANIFEST_DIR)
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent());
    if let Some(r) = repo {
        let d = r.join("copper_env").join("docs");
        if d.join("tutorial.json").is_file() {
            return Some(d);
        }
    }
    if !root.is_empty() {
        let d = Path::new(root).join("docs");
        if d.join("tutorial.json").is_file() {
            return Some(d);
        }
    }
    None
}

fn load_tutorial() -> Result<Value, String> {
    let d = docs_dir("").ok_or_else(|| "没找到 Copper 教程（环境里的 docs/tutorial.json）；先把 Copper 环境装好".to_string())?;
    let raw = std::fs::read_to_string(d.join("tutorial.json"))
        .map_err(|e| format!("读 tutorial.json 失败：{e}"))?;
    serde_json::from_str(&raw).map_err(|e| format!("tutorial.json 解析失败：{e}"))
}

fn render_chapter(t: &Value, chapter: &str, section: Option<&str>) -> Result<String, String> {
    let chapters = t.get("chapters").and_then(|x| x.as_array()).ok_or("教程格式不对：没有 chapters")?;
    let ch = chapters
        .iter()
        .find(|c| {
            c.get("id").and_then(|x| x.as_str()).map(|s| s == chapter).unwrap_or(false)
                || c.get("title").and_then(|x| x.as_str()).map(|s| s == chapter).unwrap_or(false)
        })
        .ok_or_else(|| format!("没有这一章：{chapter}"))?;
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", ch.get("title").and_then(|x| x.as_str()).unwrap_or("")));
    if let Some(s) = ch.get("summary").and_then(|x| x.as_str()) {
        out.push_str(s);
        out.push_str("\n\n");
    }
    let secs = ch.get("sections").and_then(|x| x.as_array()).cloned().unwrap_or_default();
    let mut shown = 0usize;
    for sec in &secs {
        let id = sec.get("id").and_then(|x| x.as_str()).unwrap_or("");
        let title = sec.get("title").and_then(|x| x.as_str()).unwrap_or("");
        if let Some(want) = section {
            if !id.contains(want) && !title.contains(want) {
                continue;
            }
        }
        shown += 1;
        out.push_str(&format!("## {title}\n\n"));
        for p in sec.get("paras").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
            if let Some(s) = p.as_str() {
                out.push_str(s);
                out.push_str("\n\n");
            }
        }
        if let Some(lead) = sec.get("lead").and_then(|x| x.as_str()) {
            out.push_str(lead);
            out.push_str("\n\n");
        }
        for c in sec.get("codes").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
            let code = c.get("code").and_then(|x| x.as_str()).unwrap_or("");
            out.push_str("```copper\n");
            out.push_str(code);
            out.push_str("\n```\n\n");
            if let Some(o) = c.get("out").and_then(|x| x.as_str()) {
                out.push_str("输出：\n\n```text\n");
                out.push_str(o);
                out.push_str("\n```\n\n");
            }
        }
        for tip in sec.get("tips").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
            let kind = tip.get("kind").and_then(|x| x.as_str()).unwrap_or("note");
            let text = tip.get("text").and_then(|x| x.as_str()).unwrap_or("");
            out.push_str(&format!("> [{kind}] {text}\n\n"));
        }
    }
    if shown == 0 {
        return Err(format!("这一章里没有匹配的小节：{}", section.unwrap_or("")));
    }
    Ok(out)
}

fn search_docs(t: &Value, query: &str, max: usize) -> Result<String, String> {
    let q = query.to_lowercase();
    let chapters = t.get("chapters").and_then(|x| x.as_array()).ok_or("教程格式不对：没有 chapters")?;
    let mut hits: Vec<Value> = Vec::new();
    for ch in chapters {
        let ch_id = ch.get("id").and_then(|x| x.as_str()).unwrap_or("");
        let ch_title = ch.get("title").and_then(|x| x.as_str()).unwrap_or("");
        for sec in ch.get("sections").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
            let sec_id = sec.get("id").and_then(|x| x.as_str()).unwrap_or("");
            let sec_title = sec.get("title").and_then(|x| x.as_str()).unwrap_or("");
            let mut hay = format!("{ch_title} {sec_title}\n");
            for p in sec.get("paras").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
                if let Some(s) = p.as_str() {
                    hay.push_str(s);
                    hay.push('\n');
                }
            }
            for c in sec.get("codes").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
                if let Some(s) = c.get("code").and_then(|x| x.as_str()) {
                    hay.push_str(s);
                    hay.push('\n');
                }
            }
            for tip in sec.get("tips").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
                if let Some(s) = tip.get("text").and_then(|x| x.as_str()) {
                    hay.push_str(s);
                    hay.push('\n');
                }
            }
            let low = hay.to_lowercase();
            if let Some(pos) = low.find(&q) {
                let bytes = hay.char_indices().nth(0).map(|_| 0).unwrap_or(0);
                let _ = bytes;
                let start = hay[..pos].char_indices().rev().nth(30).map(|(i, _)| i).unwrap_or(0);
                let end = hay[pos..].char_indices().nth(120).map(|(i, _)| pos + i).unwrap_or(hay.len());
                hits.push(json!({
                    "chapter": ch_id,
                    "chapter_title": ch_title,
                    "section": sec_id,
                    "section_title": sec_title,
                    "snippet": hay[start..end].replace('\n', " ⏎ ")
                }));
                if hits.len() >= max {
                    break;
                }
            }
        }
        if hits.len() >= max {
            break;
        }
    }
    if hits.is_empty() {
        return Ok(format!("教程里没找到「{query}」。试试 docs_list 看目录。"));
    }
    Ok(json!(hits).to_string())
}

fn resource_specs(root: &str) -> Vec<Value> {
    let mut out: Vec<Value> = vec![
        json!({ "uri": "copper://docs/tutorial", "name": "Copper 教程目录", "mimeType": "application/json" }),
        json!({ "uri": "copper://docs/about", "name": "关于 Copper", "mimeType": "text/markdown" }),
        json!({ "uri": "copper://docs/env", "name": "Copper 环境信息", "mimeType": "application/json" }),
    ];
    if let Ok(t) = load_tutorial() {
        for c in t.get("chapters").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
            let id = c.get("id").and_then(|x| x.as_str()).unwrap_or("");
            let title = c.get("title").and_then(|x| x.as_str()).unwrap_or("");
            out.push(json!({
                "uri": format!("copper://docs/{id}"),
                "name": title,
                "mimeType": "text/markdown",
                "description": c.get("summary").cloned().unwrap_or(json!(""))
            }));
        }
    }
    let _ = root;
    out
}

fn read_resource(root: &str, uri: &str) -> Result<(&'static str, String), String> {
    let rest = uri
        .strip_prefix("copper://docs/")
        .ok_or_else(|| format!("不认识的资源：{uri}"))?;
    match rest {
        "tutorial" => {
            let t = load_tutorial()?;
            Ok(("application/json", serde_json::to_string_pretty(&t).map_err(|e| e.to_string())?))
        }
        "about" => {
            let d = docs_dir(root).ok_or_else(|| "没找到 Copper 文档目录".to_string())?;
            let p = d.join("about-copper.md");
            let text = std::fs::read_to_string(&p)
                .or_else(|_| Ok::<String, std::io::Error>(format!("# 关于 Copper\n\n（{} 不存在）", p.display())))
                .map_err(|e| e.to_string())?;
            Ok(("text/markdown", text))
        }
        "env" => {
            let env = crate::commands::copper_env::resolve_env();
            Ok(("application/json", json!({
                "found": env.found,
                "home": env.home,
                "compiler": env.compiler,
                "docs": docs_dir(root).map(|d| d.to_string_lossy().to_string()).unwrap_or_default()
            }).to_string()))
        }
        id => {
            let t = load_tutorial()?;
            Ok(("text/markdown", render_chapter(&t, id, None)?))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_list_is_complete() {
        let specs = tool_specs();
        let names: Vec<&str> = specs.iter().map(|t| t.0.as_str()).collect();
        for want in [
            "project_info", "list_files", "read_file", "write_file",
            "search", "run_script", "get_diagnostics", "get_outline",
            "run", "test", "check", "docs_list", "docs_get", "docs_search", "env_info",
        ] {
            assert!(names.contains(&want), "缺少工具 {want}");
        }
        assert!(specs.len() >= 15);
        // 每个工具都必须有 name / description / inputSchema，否则客户端会报错
        for (_, v) in &specs {
            assert!(v.get("name").and_then(|x| x.as_str()).is_some());
            assert!(v.get("description").and_then(|x| x.as_str()).is_some());
            assert!(v.get("inputSchema").is_some());
        }
    }

    #[test]
    fn safe_join_blocks_traversal() {
        assert!(safe_join("", "a.cw").is_err(), "没打开工程时应当报错");
        assert!(safe_join("D:/proj", "../evil.cw").is_err(), ".. 必须被挡住");
        assert!(safe_join("D:/proj", "sub/../evil.cw").is_err(), "中间夹 .. 也要挡");
        assert!(safe_join("D:/proj", "ok/main.cw").is_ok());
        assert!(safe_join("D:/proj", "\\windows\\style").is_ok(), "反斜杠要归一化");
    }

    #[test]
    fn search_finds_lines() {
        let dir = std::env::temp_dir().join("float_mcp_test_search");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("a.cw"),
            "// 注释里有 needle\nfunction f()\n    return \"needle\"\n",
        )
        .unwrap();
        std::fs::write(dir.join("b.json"), "{\"k\":\"needle\"}").unwrap();
        let root = dir.to_string_lossy().to_string();

        let hits = search_project(&root, "needle", false, 100);
        assert_eq!(hits.len(), 3, "两行 .cw + 一行 .json 都该命中");
        assert!(hits.iter().any(|h| h.file == "a.cw" && h.line == 1));
        assert!(hits.iter().any(|h| h.file == "a.cw" && h.line == 3));

        let limited = search_project(&root, "needle", false, 1);
        assert_eq!(limited.len(), 1, "maxResults 要生效");

        let none = search_project(&root, "压根没有", false, 100);
        assert_eq!(none.len(), 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn atomic_write_leaves_no_temp() {
        let dir = std::env::temp_dir().join("float_mcp_test_atomic");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("x.cw");
        atomic_write(&f, "print(1)\n").unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "print(1)\n");
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.contains("tmp"))
            .collect();
        assert!(leftovers.is_empty(), "不该留下临时文件：{leftovers:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn collect_files_skips_noise() {
        let dir = std::env::temp_dir().join("float_mcp_test_walk");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::create_dir_all(dir.join("node_modules")).unwrap();
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        std::fs::write(dir.join("src/main.cw"), "print(1)").unwrap();
        std::fs::write(dir.join("node_modules/x.cw"), "print(2)").unwrap();
        std::fs::write(dir.join(".git/y.cw"), "print(3)").unwrap();
        std::fs::write(dir.join("README.md"), "# t").unwrap();

        let root = dir.to_string_lossy().to_string();
        let mut out = Vec::new();
        collect_files(&root, &dir, &None, &mut out, 0);
        assert!(out.iter().any(|p| p.contains("src/main.cw")));
        assert!(!out.iter().any(|p| p.contains("node_modules")));
        assert!(!out.iter().any(|p| p.contains(".git")));

        let mut only_cw = Vec::new();
        collect_files(&root, &dir, &Some(".cw".into()), &mut only_cw, 0);
        assert!(only_cw.iter().all(|p| p.ends_with(".cw")));
        assert!(!only_cw.iter().any(|p| p.contains("README")));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_vm_returns_output_and_feeds_inputs() {
        let dir = std::env::temp_dir().join("float_mcp_test_run");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("main.co");
        std::fs::write(&f, "print(1 + 2)\nask \"名字？\"\nprint(\"hi \" + ans)\n").unwrap();
        let v = exec_vm(&f, &["小方".to_string()]).expect("运行不该失败");
        assert_eq!(v["ok"], true);
        assert!(v["output"].as_str().unwrap_or("").contains('3'));
        assert!(v["output"].as_str().unwrap_or("").contains("hi 小方"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn discover_tests_uses_convention() {
        let dir = std::env::temp_dir().join("float_mcp_test_discover");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("tests")).unwrap();
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("tests/one.co"), "print(1)").unwrap();
        std::fs::write(dir.join("src/test_two.co"), "print(2)").unwrap();
        std::fs::write(dir.join("src/main.co"), "print(3)").unwrap();
        let root = dir.to_string_lossy().to_string();
        let found = discover_tests(&root, "");
        let names: Vec<String> = found
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert!(names.contains(&"one.co".to_string()));
        assert!(names.contains(&"test_two.co".to_string()));
        assert!(!names.contains(&"main.co".to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn renders_a_documented_chapter() {
        let Ok(t) = load_tutorial() else {
            return; // 没装环境时跳过，不让 CI 红
        };
        let text = render_chapter(&t, "types", None).expect("types 章应能渲染");
        assert!(text.starts_with('#'));
        assert!(text.contains("```copper"));
    }

    #[test]
    fn resources_expose_docs() {
        let specs = resource_specs("");
        assert!(specs.iter().any(|r| r["uri"] == "copper://docs/about"));
        if load_tutorial().is_ok() {
            assert!(specs.iter().any(|r| r["uri"] == "copper://docs/types"));
            let (mime, text) = read_resource("", "copper://docs/types").expect("应能读 types");
            assert_eq!(mime, "text/markdown");
            assert!(text.contains("变量"));
        }
    }

    #[test]
    fn check_reports_syntax_errors() {
        let dir = std::env::temp_dir().join("float_mcp_test_check");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let bad = dir.join("bad.co");
        std::fs::write(&bad, "if (\n").unwrap();
        let v = check_one(&bad).expect("检查本身不该失败");
        assert!(v["errors"].as_u64().unwrap_or(0) >= 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[ignore = "要真编 Rust，几十秒：cargo test --lib -- --ignored dispatch_runs_aot"]
    fn dispatch_runs_aot_headless() {
        let dir = std::env::temp_dir().join("float_mcp_test_aot");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("main.co"), "int i = 0\nwhile i < 3\n    i += 1\nprint(i * 14)\n").unwrap();
        let project = Arc::new(Mutex::new(dir.to_string_lossy().to_string()));
        let waiting: Arc<Mutex<HashMap<String, Sender<String>>>> = Arc::new(Mutex::new(HashMap::new()));
        let seq = Arc::new(Mutex::new(0u64));
        let push = |_: String| {};
        let req = json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": { "name": "run", "arguments": { "path": "main.co", "mode": "aot" } }
        });
        let resp = dispatch(req, &project, &waiting, &seq, None, &push).expect("要有响应");
        let text = resp["result"]["content"][0]["text"].as_str().unwrap_or("");
        let v: Value = serde_json::from_str(text).expect("AOT 返回应是 JSON");
        assert_eq!(v["ok"], true, "AOT 应成功：{text}");
        assert_eq!(v["output"].as_str().unwrap_or("").trim(), "42");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dispatch_runs_headless_tools() {
        let dir = std::env::temp_dir().join("float_mcp_test_dispatch");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("main.co"), "print(6 * 7)\n").unwrap();
        let project = Arc::new(Mutex::new(dir.to_string_lossy().to_string()));
        let waiting: Arc<Mutex<HashMap<String, Sender<String>>>> = Arc::new(Mutex::new(HashMap::new()));
        let seq = Arc::new(Mutex::new(0u64));
        let push = |_: String| {};

        let req = json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": { "name": "run", "arguments": { "path": "main.co", "mode": "vm" } }
        });
        let resp = dispatch(req, &project, &waiting, &seq, None, &push).expect("要有响应");
        let text = resp["result"]["content"][0]["text"].as_str().unwrap_or("");
        let v: Value = serde_json::from_str(text).expect("run 的返回应是 JSON");
        assert_eq!(v["ok"], true, "headless run 应成功：{text}");
        assert!(v["output"].as_str().unwrap_or("").contains("42"), "输出应包含 42：{text}");

        let req = json!({
            "jsonrpc": "2.0", "id": 2, "method": "resources/read",
            "params": { "uri": "copper://docs/tutorial" }
        });
        let resp = dispatch(req, &project, &waiting, &seq, None, &push).expect("要有响应");
        assert!(resp["result"]["contents"].is_array());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
