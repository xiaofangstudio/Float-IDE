use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::Manager;

fn app_data_dir(app: &tauri::AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir().join("tempus"))
}

fn recent_file(app: &tauri::AppHandle) -> PathBuf {
    app_data_dir(app).join("recent.json")
}

fn unique_dir(base: &Path, name: &str) -> PathBuf {
    let mut candidate = base.join(name);
    let mut i = 2;
    while candidate.exists() {
        candidate = base.join(format!("{}-{}", name, i));
        i += 1;
    }
    candidate
}

#[derive(Deserialize)]
pub struct NewProjectArgs {
    pub dir: String,
    pub name: String,
    pub files: std::collections::HashMap<String, String>,
}

#[tauri::command]
pub fn project_new(app: tauri::AppHandle, args: NewProjectArgs) -> Result<String, String> {
    let base = Path::new(&args.dir);
    if !base.is_dir() {
        return Err(format!("目录不存在：{}", args.dir));
    }
    let safe_name = args.name.replace(['\\', '/', ':', '*', '?', '"', '<', '>', '|'], "-");
    let proj = unique_dir(base, &safe_name);
    std::fs::create_dir_all(&proj).map_err(|e| e.to_string())?;
    for (rel, content) in &args.files {
        let p = proj.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&p, content).map_err(|e| e.to_string())?;
    }
    let path = proj.to_string_lossy().to_string();
    recent_push(app, path.clone(), safe_name)?;
    Ok(path)
}

#[tauri::command]
pub fn recent_get(app: tauri::AppHandle) -> Vec<RecentItem> {
    let p = recent_file(&app);
    std::fs::read_to_string(p)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RecentItem {
    pub name: String,
    pub path: String,
    #[serde(rename = "lastOpened")]
    pub last_opened: String,
}

#[tauri::command]
pub fn recent_push(app: tauri::AppHandle, path: String, name: String) -> Result<(), String> {
    let dir = app_data_dir(&app);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut list: Vec<RecentItem> = recent_get(app.clone());
    list.retain(|r| r.path != path);
    list.insert(0, RecentItem {
        name,
        path: path.clone(),
        last_opened: now_string(),
    });
    list.truncate(10);
    let json = serde_json::to_string_pretty(&list).map_err(|e| e.to_string())?;
    std::fs::write(recent_file(&app), json).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn recent_remove(app: tauri::AppHandle, path: String) -> Result<(), String> {
    let mut list: Vec<RecentItem> = recent_get(app.clone());
    list.retain(|r| r.path != path);
    let dir = app_data_dir(&app);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(&list).map_err(|e| e.to_string())?;
    std::fs::write(recent_file(&app), json).map_err(|e| e.to_string())
}

fn now_string() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    // 简单换算为 UTC 日期时间（不依赖外部库）
    let days = secs / 86400;
    let rem = secs % 86400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let (y, mo, d) = civil_from_days(days as i64);
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", y, mo, d, (h + 8) % 24, m, s)
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[tauri::command]
pub fn pick_folder(title: String) -> Option<String> {
    rfd::FileDialog::new().set_title(&title).pick_folder().map(|p| p.to_string_lossy().to_string())
}

/// 打开单个文件（.cw 之类）。之前的"打开"只能选文件夹，
/// 所以用户在对话框里根本看不到 .cw 文件——这个命令专门补这个洞。
#[tauri::command]
pub fn pick_open_file(title: String, exts: Vec<String>) -> Option<String> {
    let mut dlg = rfd::FileDialog::new().set_title(&title);
    let ext_refs: Vec<&str> = exts.iter().map(|e| e.as_str()).collect();
    if !ext_refs.is_empty() {
        dlg = dlg.add_filter("脚本", &ext_refs);
    }
    // 再给一个"所有文件"，免得别人把脚本起成奇怪后缀就打不开了
    dlg = dlg.add_filter("所有文件", &["*"]);
    if let Some(d) = std::env::current_dir().ok() {
        dlg = dlg.set_directory(d);
    }
    dlg.pick_file().map(|p| p.to_string_lossy().to_string())
}

/// 启动时带进来的路径（双击 .cw / 把文件拖到 exe 上）。
/// 只认第一个参数，且必须真实存在。
#[tauri::command]
pub fn startup_path() -> Option<String> {
    crate::startup::path()
}

#[tauri::command]
pub fn pick_image(title: String) -> Option<String> {
    rfd::FileDialog::new()
        .set_title(&title)
        .add_filter("图片", &["png", "jpg", "jpeg", "webp", "bmp", "gif"])
        .pick_file()
        .map(|p| p.to_string_lossy().to_string())
}

#[tauri::command]
pub fn pick_save_file(title: String, default_name: String, ext: String) -> Option<String> {
    let mut dlg = rfd::FileDialog::new().set_title(&title);
    if !default_name.is_empty() {
        dlg = dlg.set_file_name(&default_name);
    }
    if !ext.is_empty() {
        dlg = dlg.add_filter(&format!("*.{}", ext), &[&ext]);
    }
    dlg.save_file().map(|p| p.to_string_lossy().to_string())
}

fn walk_files(dir: &Path, out: &mut Vec<(PathBuf, String)>) -> Result<(), String> {
    for e in std::fs::read_dir(dir).map_err(|e| e.to_string())?.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if p.is_dir() {
            if name == "out" || name.starts_with('.') { continue; }
            walk_files(&p, out)?;
        } else {
            out.push((p.clone(), name));
        }
    }
    Ok(())
}

#[tauri::command]
pub fn project_export_zip(project_dir: String, out_path: String) -> Result<(), String> {
    let dir = Path::new(&project_dir);
    if !dir.is_dir() {
        return Err(format!("工程目录不存在：{}", project_dir));
    }
    let mut files: Vec<(PathBuf, String)> = Vec::new();
    walk_files(dir, &mut files)?;
    let file = std::fs::File::create(&out_path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (p, _) in &files {
        let rel = p.strip_prefix(dir).map_err(|e| e.to_string())?
            .to_string_lossy().replace('\\', "/");
        zip.start_file(rel, opts).map_err(|e| e.to_string())?;
        let data = std::fs::read(p).map_err(|e| e.to_string())?;
        zip.write_all(&data).map_err(|e| e.to_string())?;
    }
    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn project_import_zip(app: tauri::AppHandle, zip_path: String, dest_dir: String) -> Result<String, String> {
    let f = std::fs::File::open(&zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(f).map_err(|e| e.to_string())?;
    // 工程解到 dest_dir/<zip 文件名>/
    let stem = Path::new(&zip_path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "导入工程".into());
    let dest = unique_dir(Path::new(&dest_dir), &stem);
    std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let rel = entry.name().replace('\\', "/");
        if rel.contains("..") { continue; }
        let out = dest.join(&rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        } else {
            if let Some(p) = out.parent() {
                std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
            }
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf).map_err(|e| e.to_string())?;
            std::fs::write(&out, buf).map_err(|e| e.to_string())?;
        }
    }
    if !dest.join("project.json").is_file() {
        return Err("压缩包里没有 project.json，不是 Tempus 工程包".into());
    }
    let name = serde_json::from_str::<serde_json::Value>(
        &std::fs::read_to_string(dest.join("project.json")).map_err(|e| e.to_string())?
    )
    .ok()
    .and_then(|v| v.get("name").and_then(|n| n.as_str()).map(String::from))
    .unwrap_or(stem);
    let path = dest.to_string_lossy().to_string();
    recent_push(app, path.clone(), name)?;
    Ok(path)
}

#[tauri::command]
pub fn open_path(path: String) -> Result<(), String> {
    if Path::new(&path).is_dir() {
        Command::new("explorer").arg(&path).spawn().map_err(|e| e.to_string())?;
    } else {
        Command::new("cmd").args(["/c", "start", "", &path]).spawn().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[derive(Serialize)]
pub struct EnvInfo {
    pub edge: Option<String>,
    pub chrome: Option<String>,
    pub ffmpeg: Option<String>,
}

pub fn find_browser() -> Option<(String, &'static str)> {
    let candidates: [(String, &str); 8] = [
        (r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe".into(), "edge"),
        (r"C:\Program Files\Microsoft\Edge\Application\msedge.exe".into(), "edge"),
        (format!(r"{}\AppData\Local\Microsoft\Edge\Application\msedge.exe", std::env::var("LOCALAPPDATA").unwrap_or_default()), "edge"),
        (r"C:\Program Files\Google\Chrome\Application\chrome.exe".into(), "chrome"),
        (r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe".into(), "chrome"),
        (format!(r"{}\AppData\Local\Google\Chrome\Application\chrome.exe", std::env::var("LOCALAPPDATA").unwrap_or_default()), "chrome"),
        (r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge_proxy.exe".into(), "edge"),
        (r"C:\Program Files\Google\Chrome Beta\Application\chrome.exe".into(), "chrome"),
    ];
    for (p, kind) in candidates {
        if Path::new(&p).is_file() {
            return Some((p, kind));
        }
    }
    None
}

pub fn find_ffmpeg() -> Option<String> {
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let candidates = [
        exe_dir.join("ffmpeg").join("ffmpeg.exe"),
        exe_dir.join("src-tauri").join("ffmpeg").join("ffmpeg.exe"),
        PathBuf::from(r"D:\20533\deskbox\fangwork\Tempus\scaffold\src-tauri\ffmpeg\ffmpeg.exe"),
    ];
    for p in candidates {
        if p.is_file() {
            return Some(p.to_string_lossy().to_string());
        }
    }
    // PATH 里找
    if which_ok("ffmpeg") { return Some("ffmpeg".into()); }
    None
}

fn which_ok(name: &str) -> bool {
    Command::new(name).arg("-version").output().map(|o| o.status.success()).unwrap_or(false)
}

#[tauri::command]
pub fn detect_env() -> EnvInfo {
    let browser = find_browser();
    let (edge, chrome) = match browser {
        Some((p, "edge")) => (Some(p), None),
        Some((p, _)) => (None, Some(p)),
        None => (None, None),
    };
    EnvInfo { edge, chrome, ffmpeg: find_ffmpeg() }
}
