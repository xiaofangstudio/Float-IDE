use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::Manager;

/* 插件宿主（Rust 侧）。
 *
 * 职责边界：
 *   Rust 只负责「找到插件、读清单、装/卸、安全地读插件内文件」；
 *   插件的执行、贡献点解析、API 注入全在 TypeScript 里做（见 src/plugins/）。
 *
 * 安全要点：
 *   1. 所有接受 id 的命令都必须把路径收敛到插件根目录内（防 ../ 穿越）。
 *   2. zip 安装要防 Zip Slip（条目路径带 ../ 的拒绝）。
 *   3. 卸载用 remove_dir_all，但先校验 id 合法（只允许字母数字点横线）。
 */

fn plugin_root(app: &tauri::AppHandle) -> PathBuf {
    let base = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir().join("floatide"));
    let dir = base.join("plugins");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// 只允许 letters / digits / . _ - 的 id，避免 ../ 之类
fn safe_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > 96 {
        return Err("插件 id 不合法".into());
    }
    if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_') {
        return Err(format!("插件 id 不合法：{id}"));
    }
    Ok(())
}

fn plugin_path(app: &tauri::AppHandle, id: &str) -> Result<PathBuf, String> {
    safe_id(id)?;
    let p = plugin_root(app).join(id);
    Ok(p)
}

#[derive(Serialize)]
pub struct PluginEntry {
    pub id: String,
    pub dir: String,
    /// plugin.json 原文；解析交给前端（版本/字段演进不用改 Rust）
    pub manifest: String,
    pub error: Option<String>,
}

#[tauri::command]
pub fn plugin_root_dir(app: tauri::AppHandle) -> String {
    plugin_root(&app).to_string_lossy().to_string()
}

/// 扫描插件目录，返回每个子目录的清单原文
#[tauri::command]
pub fn plugin_scan(app: tauri::AppHandle) -> Vec<PluginEntry> {
    let root = plugin_root(&app);
    let mut out = Vec::new();
    let rd = match std::fs::read_dir(&root) {
        Ok(r) => r,
        Err(_) => return out,
    };
    for e in rd.flatten() {
        if !e.path().is_dir() {
            continue;
        }
        let dir_name = e.file_name().to_string_lossy().to_string();
        if dir_name.starts_with('.') {
            continue;
        }
        let manifest_path = e.path().join("plugin.json");
        match std::fs::read_to_string(&manifest_path) {
            Ok(text) => out.push(PluginEntry {
                id: dir_name,
                dir: e.path().to_string_lossy().to_string(),
                manifest: text,
                error: None,
            }),
            Err(err) => out.push(PluginEntry {
                id: dir_name,
                dir: e.path().to_string_lossy().to_string(),
                manifest: String::new(),
                error: Some(format!("缺少或无法读取 plugin.json：{err}")),
            }),
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// 读插件目录内的文件（相对路径，禁止越界）
#[tauri::command]
pub fn plugin_read_file(app: tauri::AppHandle, id: String, rel: String) -> Result<String, String> {
    let base = plugin_path(&app, &id)?;
    if rel.contains("..") {
        return Err("插件内路径不合法".into());
    }
    let target = base.join(&rel);
    // canonicalize 后再确认仍在插件目录内（软链接也挡住）
    let canon_base = base.canonicalize().map_err(|e| e.to_string())?;
    let canon = target.canonicalize().map_err(|e| format!("读取失败 {rel}: {e}"))?;
    if !canon.starts_with(&canon_base) {
        return Err("插件内路径越界".into());
    }
    std::fs::read_to_string(&canon).map_err(|e| format!("读取失败 {rel}: {e}"))
}

/// 写插件私有文件（插件存配置用）
#[tauri::command]
pub fn plugin_write_file(app: tauri::AppHandle, id: String, rel: String, contents: String) -> Result<(), String> {
    let base = plugin_path(&app, &id)?;
    if rel.contains("..") {
        return Err("插件内路径不合法".into());
    }
    let target = base.join(&rel);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&target, contents).map_err(|e| e.to_string())
}

/// 从本地文件夹安装：复制整个目录到插件根/<id>
#[tauri::command]
pub fn plugin_install_dir(app: tauri::AppHandle, src: String) -> Result<String, String> {
    let src_path = Path::new(&src);
    if !src_path.is_dir() {
        return Err("请选择一个插件文件夹（里面要有 plugin.json）".into());
    }
    let manifest = src_path.join("plugin.json");
    if !manifest.is_file() {
        return Err("这个文件夹里没有 plugin.json，不是 Float 插件".into());
    }
    // 目录名当 id；不合法就报错，别悄悄改名
    let id = src_path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    safe_id(&id)?;
    let dest = plugin_path(&app, &id)?;
    if dest.exists() {
        std::fs::remove_dir_all(&dest).map_err(|e| e.to_string())?;
    }
    copy_dir(src_path, &dest)?;
    Ok(id)
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    let rd = std::fs::read_dir(from).map_err(|e| e.to_string())?;
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name();
        let target = to.join(&name);
        if p.is_dir() {
            copy_dir(&p, &target)?;
        } else {
            std::fs::copy(&p, &target).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn plugin_uninstall(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let p = plugin_path(&app, &id)?;
    if !p.exists() {
        return Ok(());
    }
    std::fs::remove_dir_all(&p).map_err(|e| e.to_string())
}

/// 让用户能直接打开插件目录放插件
#[tauri::command]
pub fn plugin_reveal(app: tauri::AppHandle) -> Result<(), String> {
    let dir = plugin_root(&app);
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(dir.to_string_lossy().to_string())
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = std::process::Command::new("xdg-open").arg(&dir).spawn();
    }
    Ok(())
}

#[derive(Serialize)]
pub struct PluginStateFile {
    pub path: String,
}

/// 插件启用/禁用状态存在 plugins/state.json（前端读写，Rust 只给路径）
#[tauri::command]
pub fn plugin_state_path(app: tauri::AppHandle) -> PluginStateFile {
    PluginStateFile {
        path: plugin_root(&app).join("state.json").to_string_lossy().to_string(),
    }
}

/// 插件市场索引（可选）：用户把 registry.json 放进插件根目录就能离线浏览
#[derive(Serialize, Deserialize, Clone)]
pub struct RegistryEntry {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub publisher: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    /// 本地插件包路径（.cw 包或文件夹）；空表示"未下载"
    #[serde(default, rename = "localPath")]
    pub local_path: String,
}

#[tauri::command]
pub fn plugin_registry(app: tauri::AppHandle) -> Vec<RegistryEntry> {
    let p = plugin_root(&app).join("registry.json");
    std::fs::read_to_string(p)
        .ok()
        .and_then(|s| serde_json::from_str::<Vec<RegistryEntry>>(&s).ok())
        .unwrap_or_default()
}
