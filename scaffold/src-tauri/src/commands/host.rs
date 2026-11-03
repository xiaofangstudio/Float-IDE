//! 宿主信息桥：给 F6（TS 解释通道）补上 F5（Rust 内核）本来就能拿到的那些东西。
//!
//! 为什么要有这个文件：`os.getenv / os.args / os.pid / os.runcmd` 在 Rust 内核里
//! 是直接问操作系统的，但 F6 跑在 WebView 里，拿不到进程信息。少了这几个命令，
//! 同一个脚本 F5 能跑、F6 就报错——这正是"两个引擎行为不一致"的来源，所以补这一层。
//!
//! 只做"读"和"按用户要求执行"，不做任何隐式的提权或路径改写。

use serde::Serialize;
use std::collections::HashMap;
use std::process::Command;

#[derive(Serialize)]
pub struct HostInfo {
    pub pid: u32,
    pub platform: String,
    pub cwd: String,
    pub temp: String,
    pub home: String,
    pub ip: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
}

/// 一次性把宿主信息打包给前端（前端缓存起来，不用每次调用都问一遍）。
#[tauri::command]
pub fn host_info() -> HostInfo {
    let env: HashMap<String, String> = std::env::vars().collect();
    HostInfo {
        pid: std::process::id(),
        platform: if cfg!(windows) {
            "windows"
        } else if cfg!(target_os = "macos") {
            "macos"
        } else {
            "linux"
        }
        .to_string(),
        cwd: std::env::current_dir()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default(),
        temp: std::env::temp_dir().to_string_lossy().replace('\\', "/"),
        home: std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .unwrap_or_default()
            .replace('\\', "/"),
        // 本机对外的网卡地址：连一个外网地址（不真发包）拿到出口 IP；
        // 没有网络就退回回环地址，别让脚本因为没网直接崩。
        ip: local_ip(),
        args: std::env::args().skip(1).collect(),
        env,
    }
}

fn local_ip() -> String {
    use std::net::UdpSocket;
    if let Ok(sock) = UdpSocket::bind("0.0.0.0:0") {
        if sock.connect("8.8.8.8:80").is_ok() {
            if let Ok(addr) = sock.local_addr() {
                return addr.ip().to_string();
            }
        }
    }
    "127.0.0.1".to_string()
}

/// 设一个环境变量（只影响 IDE 进程自己，和 F5 里 `setenv` 的行为一致）。
#[tauri::command]
pub fn host_setenv(key: String, value: String) -> Result<bool, String> {
    if key.is_empty() {
        return Err("os.setenv 的变量名不能为空".into());
    }
    std::env::set_var(key, value);
    Ok(true)
}

#[derive(Serialize)]
pub struct CmdResult {
    pub code: i32,
    /// 标准输出 + 标准错误，拼在一起（和 F5 一致）
    pub out: String,
}

/// 跑一条系统命令并等它结束，返回退出码和输出。
#[tauri::command]
pub fn host_run_cmd(cmd: String, dir: Option<String>) -> Result<CmdResult, String> {
    if cmd.trim().is_empty() {
        return Err("os.runcmd 的命令不能为空".into());
    }
    // 工作目录跟脚本一致：F5 里 runcmd 是在脚本目录下跑的，这里也要一样
    let mut c = if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.args(["/C", &cmd]);
        c
    } else {
        let mut c = Command::new("sh");
        c.args(["-c", &cmd]);
        c
    };
    if let Some(d) = dir.as_ref().filter(|d| !d.trim().is_empty()) {
        c.current_dir(d);
    }
    let out = c.output();
    let out = out.map_err(|e| format!("os.runcmd 起不来：{}", e))?;
    let mut text = String::from_utf8_lossy(&out.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    Ok(CmdResult { code: out.status.code().unwrap_or(-1), out: text })
}

/// 发一次 HTTP 请求，返回响应正文。
///
/// 为什么不让前端直接 `fetch()`：WebView 里的 fetch 受 CORS 限制，很多站点直接拒；
/// 走宿主这一层就等于 Rust 内核那条路（ureq，10 秒超时），F5/F6 结果一致。
#[tauri::command]
pub fn host_fetch(method: String, url: String, body: Option<String>) -> Result<String, String> {
    if url.trim().is_empty() {
        return Err("网址不能为空".into());
    }
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(10))
        .build();
    let m = method.to_uppercase();
    let req = if m == "POST" { agent.post(&url) } else { agent.get(&url) };
    let resp = match body {
        Some(b) if m != "GET" => req.send_string(&b),
        _ => req.call(),
    };
    match resp {
        Ok(r) => r.into_string().map_err(|e| format!("读响应失败：{}", e)),
        Err(ureq::Error::Status(code, r)) => {
            let detail = r.into_string().unwrap_or_default();
            Err(format!("HTTP {}：{}", code, detail.chars().take(300).collect::<String>()))
        }
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 能拿到宿主信息() {
        let h = host_info();
        assert!(h.pid > 0);
        assert!(!h.platform.is_empty());
        assert!(!h.temp.is_empty(), "临时目录不该是空的");
        println!("pid {} / {} / temp {}", h.pid, h.platform, h.temp);
    }

    #[test]
    fn 跑命令能拿到输出() {
        let r = host_run_cmd("echo copper".into(), None).expect("应该能跑");
        assert!(r.out.contains("copper"), "输出是 {:?}", r.out);
        assert_eq!(r.code, 0);
    }

    #[test]
    fn 空变量名要报错() {
        assert!(host_setenv(String::new(), "x".into()).is_err());
    }
}
