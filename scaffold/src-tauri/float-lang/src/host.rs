//! 宿主环境钩子：文件 IO 的路径锚点 + 网络请求的实现注入。
//!
//! 内核本体保持零依赖（这是刻意设计：编译快、wasm 干净），
//! 所以网络这种需要 TLS 库的能力由调用方（Tauri 命令层）把实现挂进来；
//! 浏览器 demo（wasm）里没有挂实现，内置函数会返回明确的中文错误。

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

/* ───────── 相对路径锚点 ───────── */

thread_local! {
    /// 主脚本所在目录：读文件/写文件的相对路径按它解析。
    static BASE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// 运行前设置：`base` 是主文件所在目录（api 层的 run_* 会自动传进来）。
pub fn set_base(base: Option<&Path>) {
    BASE.with(|b| *b.borrow_mut() = base.map(|p| p.to_path_buf()));
}

/// 把脚本里的路径解析成绝对路径：绝对路径原样用，相对路径锚到主脚本目录。
pub fn resolve_path(p: &str) -> PathBuf {
    let path = Path::new(p);
    if path.is_absolute() {
        return path.to_path_buf();
    }
    BASE.with(|b| {
        match b.borrow().as_ref() {
            Some(dir) => dir.join(path),
            None => PathBuf::from(p),
        }
    })
}

/// 当前"脚本所在目录"（绝对路径，正斜杠；没设置就给进程当前目录）。
/// cwd() / runcmd 用它回答"我在哪儿"。
pub fn base_dir_string() -> String {
    let dir = BASE.with(|b| b.borrow().clone());
    match dir {
        Some(d) => d.to_string_lossy().replace('\\', "/"),
        None => std::env::current_dir()
            .map(|d| d.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default(),
    }
}

/* ───────── 网络请求钩子 ───────── */

/// 发一次 HTTP 请求。参数：(方法 GET/POST, 网址, POST 正文)。
/// 返回响应正文；失败给中文错误。
pub type FetchImpl = Arc<dyn Fn(&str, &str, Option<&str>) -> Result<String, String> + Send + Sync>;

fn fetch_slot() -> &'static std::sync::Mutex<Option<FetchImpl>> {
    static SLOT: OnceLock<std::sync::Mutex<Option<FetchImpl>>> = OnceLock::new();
    SLOT.get_or_init(|| std::sync::Mutex::new(None))
}

/// 宿主（Tauri 命令层 / 测试）把网络实现挂进来。挂一次就够了。
pub fn set_fetch(f: FetchImpl) {
    *fetch_slot().lock().unwrap() = Some(f);
}

/// 内置函数走这里发请求。没挂实现 = 当前环境不支持网络。
pub fn do_fetch(method: &str, url: &str, body: Option<&str>) -> Result<String, String> {
    let g = fetch_slot().lock().unwrap();
    match g.as_ref() {
        Some(f) => f(method, url, body),
        None => Err("当前环境不支持网络请求（桌面版 Float IDE 才有网络）".into()),
    }
}
