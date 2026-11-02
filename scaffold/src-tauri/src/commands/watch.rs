//! 工程文件监视（轮询 mtime，不引第三方依赖）：
//! AI / 别的编辑器在磁盘上改了 .cw / .json / .txt，前端 1 秒多内就能收到
//! `fs:changed` 事件，把新内容热灌进打开的缓冲区——不用刷新、不用重开工程。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// 只关心 IDE 支持的文件（Copper 新扩展 .co，旧 .cw 兼容）
fn wanted(name: &str) -> bool {
    let n = name.to_lowercase();
    n.ends_with(".co") || n.ends_with(".cw") || n.ends_with(".json") || n.ends_with(".txt")
        || n.ends_with(".md") || n.ends_with(".in")
}

fn scan(root: &Path, out: &mut HashMap<PathBuf, SystemTime>) {
    fn walk(dir: &Path, root: &Path, out: &mut HashMap<PathBuf, SystemTime>, depth: usize) {
        if depth > 10 || out.len() > 6000 {
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
                walk(&p, root, out, depth + 1);
            } else if wanted(&name) {
                if let Ok(m) = e.metadata().and_then(|m| m.modified()) {
                    out.insert(p, m);
                }
            }
        }
    }
    walk(root, root, out, 0);
}

struct WatchState {
    root: PathBuf,
    files: HashMap<PathBuf, SystemTime>,
}

static WATCH: Mutex<Option<WatchState>> = Mutex::new(None);

#[derive(Serialize)]
pub struct WatchInfo {
    watching: bool,
    root: String,
}

/// 开始监视一个工程（重复调用同一工程是幂等的；换工程会重置快照）
pub fn watch_start(app: AppHandle, project: String) -> Result<WatchInfo, String> {
    let root = PathBuf::from(&project);
    let mut files = HashMap::new();
    scan(&root, &mut files);

    {
        let mut g = WATCH.lock().map_err(|e| e.to_string())?;
        let need_new = match g.as_ref() {
            Some(st) => st.root != root,
            None => true,
        };
        if need_new {
            *g = Some(WatchState { root: root.clone(), files });
        } else if let Some(st) = g.as_mut() {
            st.files = files;   // 同一工程：刷新快照（可能有我们没收到的事件）
        }
    }

    let app2 = app.clone();
    let root2 = root.clone();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(1000));
            let changed: Vec<String> = {
                let mut g = match WATCH.lock() {
                    Ok(g) => g,
                    Err(_) => break,
                };
                let st = match g.as_mut() {
                    // 工程被换掉 / 前端重开：这条线程退役
                    Some(st) if st.root == root2 => st,
                    _ => break,
                };
                let mut now = HashMap::new();
                scan(&st.root, &mut now);
                let mut diff: Vec<PathBuf> = Vec::new();
                for (p, m) in &now {
                    match st.files.get(p) {
                        Some(old) if old == m => {}
                        _ => diff.push(p.clone()),
                    }
                }
                for p in st.files.keys() {
                    if !now.contains_key(p) {
                        diff.push(p.clone());
                    }
                }
                if diff.is_empty() {
                    continue;
                }
                st.files = now;
                diff.iter()
                    .filter_map(|p| p.strip_prefix(&st.root).ok())
                    .map(|r| r.to_string_lossy().replace('\\', "/"))
                    .collect()
            };
            let _ = app2.emit("fs:changed", &changed);
        }
    });

    Ok(WatchInfo { watching: true, root: project })
}

#[tauri::command]
pub fn fs_watch_start(app: AppHandle, project: String) -> Result<WatchInfo, String> {
    watch_start(app, project)
}
