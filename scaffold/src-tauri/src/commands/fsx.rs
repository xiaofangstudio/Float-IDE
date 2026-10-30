use serde::Serialize;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/* ───────── 写安全 ─────────
 * 1) 原子写：先写同目录临时文件再 rename，避免写到一半被另一个进程/窗口读到半截内容。
 * 2) 版本校验：写前比对 mtime，若被别的进程（或另一个 Float IDE 窗口）改过则拒绝，
 *    交回前端提示「文件已被外部修改」，而不是静默覆盖用户代码。
 */

fn mtime_ms(p: &Path) -> u128 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .map(|t| t.duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0))
        .unwrap_or(0)
}

/// 原子写：内容先落盘到 <name>.float-tmp-<pid>-<n>，再 rename 覆盖目标。
fn atomic_write(path: &Path, contents: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let pid = std::process::id();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp_name = format!(
        ".{}.float-tmp-{}-{}",
        path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "f".into()),
        pid,
        nonce
    );
    let tmp = path.parent().unwrap_or(Path::new(".")).join(tmp_name);
    let r = (|| -> Result<(), String> {
        let mut f = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
        std::io::Write::write_all(&mut f, contents.as_bytes()).map_err(|e| e.to_string())?;
        f.sync_all().map_err(|e| e.to_string())?;
        drop(f);
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    })();
    if r.is_err() {
        let _ = std::fs::remove_file(&tmp); // 清理残留，别在工程里留垃圾
    }
    r
}

#[derive(Serialize)]
pub struct FileEntry {
    pub name: String,
    #[serde(rename = "relPath")]
    pub rel_path: String,
    #[serde(rename = "isDir")]
    pub is_dir: bool,
    pub size: u64,
}

fn list_one(dir: &str) -> Result<Vec<FileEntry>, String> {
    let base = Path::new(dir);
    let mut out = Vec::new();
    let rd = std::fs::read_dir(base).map_err(|e| e.to_string())?;
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if name == "." || name == ".." { continue; }
        let meta = e.metadata().map_err(|e| e.to_string())?;
        out.push(FileEntry {
            name: name.clone(),
            rel_path: name,
            is_dir: meta.is_dir(),
            size: if meta.is_file() { meta.len() } else { 0 },
        });
    }
    out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(out)
}

#[tauri::command]
pub fn fs_list(dir: String) -> Result<Vec<FileEntry>, String> {
    list_one(&dir)
}

#[tauri::command]
pub fn fs_read(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| format!("读取失败 {}: {}", path, e))
}

/// 文件版本指纹：修改时间（ms）+ 字节数。前端用来判断磁盘内容是否被外部改动过。
#[derive(Serialize, Clone, Copy)]
pub struct FileStat {
    #[serde(rename = "mtimeMs")]
    pub mtime_ms: u128,
    pub size: u64,
    pub exists: bool,
}

#[tauri::command]
pub fn fs_stat(path: String) -> FileStat {
    let p = Path::new(&path);
    match std::fs::metadata(p) {
        Ok(m) => FileStat {
            mtime_ms: mtime_ms(p),
            size: m.len(),
            exists: true,
        },
        Err(_) => FileStat { mtime_ms: 0, size: 0, exists: false },
    }
}

/// 带版本校验的写：expect_mtime_ms 为 0 表示「不校验」（新建/强制覆盖）。
/// 磁盘 mtime 与期望不一致 → 返回 Err("CONFLICT")，由前端决定覆盖还是重载。
#[tauri::command]
pub fn fs_write_checked(
    path: String,
    contents: String,
    expect_mtime_ms: u128,
) -> Result<FileStat, String> {
    let p = Path::new(&path);
    let cur = mtime_ms(p);
    if p.exists() && expect_mtime_ms != 0 && cur != expect_mtime_ms {
        return Err("CONFLICT".into());
    }
    atomic_write(p, &contents)?;
    let after = std::fs::metadata(p).map_err(|e| e.to_string())?;
    Ok(FileStat { mtime_ms: mtime_ms(p), size: after.len(), exists: true })
}

#[tauri::command]
pub fn fs_write(path: String, contents: String) -> Result<(), String> {
    atomic_write(Path::new(&path), &contents).map_err(|e| format!("写入失败 {}: {}", path, e))
}

/* ───────── 工程占用锁 ─────────
 * 两个 Float IDE 同时打开同一个工程时，互相覆盖文件是最容易丢代码的情况。
 * 这里用「心跳锁」解决：占用方每 4 秒续期一次，超过 12 秒没续期视为已退出（崩溃/强杀也能自愈）。
 * 第二个实例打开时会被明确告知，而不是默默抢锁。
 */

const LOCK_FRESH_MS: u128 = 12_000;

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

#[derive(Serialize, serde::Deserialize, Clone)]
pub struct LockInfo {
    pub id: String,
    pub ts: u128,
    pub pid: u32,
}

fn lock_path(dir: &str) -> std::path::PathBuf {
    Path::new(dir).join(".float").join("lock.json")
}

fn read_lock(dir: &str) -> Option<LockInfo> {
    let s = std::fs::read_to_string(lock_path(dir)).ok()?;
    serde_json::from_str::<LockInfo>(&s).ok()
}

/// 返回 Err 表示已被别的实例占用（消息里带上对方 pid）。
#[tauri::command]
pub fn project_lock_acquire(dir: String, id: String) -> Result<LockInfo, String> {
    let lp = lock_path(&dir);
    if let Some(p) = lp.parent() {
        std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    if let Some(cur) = read_lock(&dir) {
        if cur.id != id && now_ms().saturating_sub(cur.ts) < LOCK_FRESH_MS {
            return Err(format!("已有一个 Float IDE（进程 {}）正在编辑这个工程", cur.pid));
        }
    }
    let info = LockInfo { id, ts: now_ms(), pid: std::process::id() };
    let s = serde_json::to_string(&info).map_err(|e| e.to_string())?;
    std::fs::write(&lp, s).map_err(|e| e.to_string())?;
    Ok(info)
}

/// 心跳续期；若锁已被别人抢走（例如本实例卡死超时），返回 false 让前端提示。
#[tauri::command]
pub fn project_lock_touch(dir: String, id: String) -> bool {
    match read_lock(&dir) {
        Some(cur) if cur.id == id => {
            let next = LockInfo { id: id.clone(), ts: now_ms(), pid: std::process::id() };
            if let Ok(s) = serde_json::to_string(&next) {
                let _ = std::fs::write(lock_path(&dir), s);
                return true;
            }
            false
        }
        _ => false,
    }
}

#[tauri::command]
pub fn project_lock_release(dir: String, id: String) {
    if let Some(cur) = read_lock(&dir) {
        if cur.id == id {
            let _ = std::fs::remove_file(lock_path(&dir));
        }
    }
}

#[tauri::command]
pub fn fs_mkdir(path: String) -> Result<(), String> {
    std::fs::create_dir_all(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn fs_rename(from: String, to: String) -> Result<(), String> {
    std::fs::rename(&from, &to).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn fs_delete(path: String) -> Result<(), String> {
    let p = Path::new(&path);
    if p.is_dir() {
        std::fs::remove_dir_all(p).map_err(|e| e.to_string())
    } else {
        std::fs::remove_file(p).map_err(|e| e.to_string())
    }
}
