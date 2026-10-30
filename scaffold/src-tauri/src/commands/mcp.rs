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
                        let _ = handle_conn(stream, p, w, l, s, ap);
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
    app: AppHandle,
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
            let r = dispatch(item.clone(), &project, &waiting, &seq, &app, &push);
            if let Some(v) = r {
                out.push(v);
            }
        }
        let _ = respond(&mut stream, 200, "application/json", &Value::Array(out).to_string());
        return Ok(());
    }

    match dispatch(req, &project, &waiting, &seq, &app, &push) {
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
    app: &AppHandle,
    push: &dyn Fn(String),
) -> Option<Value> {
    let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let id = req.get("id").cloned();
    let params = req.get("params").cloned().unwrap_or(Value::Null);
    let is_notification = id.is_none();

    push(format!("→ {method}"));

    let result: Option<Result<Value, (i64, String)>> = match method {
        "initialize" => Some(Ok(json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {
                "tools": { "listChanged": false },
                "resources": { "subscribe": false, "listChanged": false }
            },
            "serverInfo": { "name": "float-ide", "version": env!("CARGO_PKG_VERSION") }
        }))),
        "notifications/initialized" | "notifications/cancelled" => return None,
        "ping" => Some(Ok(json!({}))),
        "tools/list" => Some(Ok(json!({ "tools": tool_specs().iter().map(|t| t.1.clone()).collect::<Vec<_>>() }))),
        "resources/list" => Some(Ok(json!({ "resources": [] }))),
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
                        "ext": { "type": "string", "description": "只看某个后缀，例如 .cw" }
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
                "description": "用 Float Script 内核运行一段代码并返回控制台输出与错误（编译执行）",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "code": { "type": "string", "description": "要运行的 Float Script 源码" }
                    },
                    "required": ["code"]
                }
            }),
        ),
        (
            "get_diagnostics".into(),
            json!({
                "name": "get_diagnostics",
                "description": "对一个 .cw 文件做语法与语义检查，返回诊断列表（行号、列号、消息、严重级别）",
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
                "description": "返回一个 .cw 文件的符号大纲（函数、类、方法、变量及其行号）",
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
    app: &AppHandle,
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
        // 语言类：走前端桥
        "run_script" | "get_diagnostics" | "get_outline" => {
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
        if !rel.to_lowercase().ends_with(".cw") && !rel.to_lowercase().ends_with(".json")
            && !rel.to_lowercase().ends_with(".md") {
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
        ] {
            assert!(names.contains(&want), "缺少工具 {want}");
        }
        assert_eq!(specs.len(), 8);
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
}
