//! 脚本包：把一包脚本拖进工程的 `packages/` 目录，就能被检测到、一键替换。
//!
//! 为什么要这一层：`float-lang` 内核是要单独升级的（换新版本的解释器/内建函数），
//! 但用户下载的是一个绿色版软件包。所以约定好目录结构，让"替换"变成拖拽：
//!
//! ```text
//! <工程根>/
//!   packages/
//!     .active.json            ← 记录当前启用的是哪一包哪一版
//!     demo@1.0.0/             ← 一个脚本包（拖进来的就是这一级）
//!       manifest.json
//!       main.cw
//!      demo@1.1.0/             ← 新版拖进来，不覆盖旧版，随时能回滚
//! ```
//!
//! manifest.json：
//! ```json
//! { "name": "demo", "version": "1.0.0", "entry": "main.cw", "title": "示例", "author": "", "runtime": ">=0.1.0" }
//! ```
//!
//! 这里只做纯文件操作（不碰 UI、不依赖 tauri），方便单测，也方便以后搬到 WASM 里。

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// 一个检测到的脚本包
#[derive(Debug, Clone, PartialEq)]
pub struct PackageInfo {
    pub name: String,
    pub version: String,
    pub title: String,
    pub entry: String,
    /// 包目录
    pub dir: PathBuf,
    /// 要求的运行时版本（没写就是空）
    pub runtime: String,
    /// 是不是当前启用的那一版
    pub active: bool,
}

impl PackageInfo {
    pub fn entry_path(&self) -> PathBuf {
        self.dir.join(&self.entry)
    }

    /// 版本号比较（按数字段比，1.10 > 1.9）
    pub fn version_cmp(&self, other: &str) -> i32 {
        cmp_version(&self.version, other)
    }
}

const MANIFEST: &str = "manifest.json";
const ACTIVE_FILE: &str = ".active.json";

/// 扫描 `<root>/packages` 下的所有脚本包（按名称+版本排序）
pub fn scan_packages(root: &Path) -> io::Result<Vec<PackageInfo>> {
    let dir = root.join("packages");
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let active = read_active(root);
    let mut out = Vec::new();
    for ent in fs::read_dir(&dir)? {
        let ent = ent?;
        let path = ent.path();
        if !path.is_dir() {
            continue;
        }
        // 跳过隐藏目录（.active 之类）
        if path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.starts_with('.'))
            .unwrap_or(false)
        {
            continue;
        }
        if let Some(mut info) = read_manifest(&path) {
            info.active = match &active {
                Some((an, av)) => an == &info.name && av == &info.version,
                None => false,
            };
            out.push(info);
        }
    }
    out.sort_by(|a, b| {
        a.name.cmp(&b.name).then_with(|| match cmp_version(&b.version, &a.version) {
            x if x < 0 => std::cmp::Ordering::Less,
            0 => std::cmp::Ordering::Equal,
            _ => std::cmp::Ordering::Greater,
        })
    });
    Ok(out)
}

/// 当前启用的是哪一包
pub fn active_package(root: &Path) -> Option<PackageInfo> {
    scan_packages(root).ok()?.into_iter().find(|p| p.active)
}

/// 已安装的某一包的所有版本，最新在前
pub fn versions_of(root: &Path, name: &str) -> Vec<PackageInfo> {
    scan_packages(root)
        .unwrap_or_default()
        .into_iter()
        .filter(|p| p.name == name)
        .collect()
}

/// 把 `src` 这个包目录装进 `<root>/packages/`，并切换成启用状态。
///
/// 已经装过同样的 name+version 时直接覆盖（这就是"一键替换"）。
/// 返回装好之后的包信息。
pub fn install_package(root: &Path, src: &Path) -> Result<PackageInfo, String> {
    let info = read_manifest(src).ok_or_else(|| {
        format!(
            "这不是一个脚本包：{} 里没有可用的 manifest.json（要有 name / version）",
            src.display()
        )
    })?;
    let dest_root = root.join("packages");
    fs::create_dir_all(&dest_root).map_err(|e| format!("建不了 packages 目录：{}", e))?;

    let dest = dest_root.join(format!("{}@{}", info.name, info.version));
    if dest.exists() {
        fs::remove_dir_all(&dest).map_err(|e| format!("清理旧版本失败：{}", e))?;
    }
    copy_dir(src, &dest).map_err(|e| format!("复制脚本包失败：{}", e))?;

    write_active(root, &info.name, &info.version)?;
    let mut installed = read_manifest(&dest).ok_or_else(|| "装完之后读不回来".to_string())?;
    installed.active = true;
    Ok(installed)
}

/// 切换到已装的某个版本
pub fn switch_to(root: &Path, name: &str, version: &str) -> Result<(), String> {
    let target = root.join("packages").join(format!("{}@{}", name, version));
    if !target.is_dir() {
        return Err(format!("没装过 {}@{}", name, version));
    }
    write_active(root, name, version)
}

/// 删掉某个版本（正在启用的不给删）
pub fn remove_version(root: &Path, name: &str, version: &str) -> Result<(), String> {
    if let Some((an, av)) = read_active(root) {
        if an == name && av == version {
            return Err("这是当前启用的版本，先切到别的版本再删".into());
        }
    }
    let target = root.join("packages").join(format!("{}@{}", name, version));
    if target.is_dir() {
        fs::remove_dir_all(&target).map_err(|e| format!("删除失败：{}", e))?;
    }
    Ok(())
}

/* ───────────────────────── 内部 ───────────────────────── */

fn read_manifest(dir: &Path) -> Option<PackageInfo> {
    let text = fs::read_to_string(dir.join(MANIFEST)).ok()?;
    let name = json_str(&text, "name")?;
    let version = json_str(&text, "version")?;
    let entry = json_str(&text, "entry").unwrap_or_else(|| "main.cw".to_string());
    Some(PackageInfo {
        title: json_str(&text, "title").unwrap_or_else(|| name.clone()),
        runtime: json_str(&text, "runtime").unwrap_or_default(),
        name,
        version,
        entry,
        dir: dir.to_path_buf(),
        active: false,
    })
}

fn read_active(root: &Path) -> Option<(String, String)> {
    let text = fs::read_to_string(root.join("packages").join(ACTIVE_FILE)).ok()?;
    Some((json_str(&text, "name")?, json_str(&text, "version")?))
}

fn write_active(root: &Path, name: &str, version: &str) -> Result<(), String> {
    let dir = root.join("packages");
    fs::create_dir_all(&dir).map_err(|e| format!("建不了 packages 目录：{}", e))?;
    let body = format!(
        "{{\n  \"name\": \"{}\",\n  \"version\": \"{}\"\n}}\n",
        escape(name),
        escape(version)
    );
    fs::write(dir.join(ACTIVE_FILE), body).map_err(|e| format!("写启用记录失败：{}", e))
}

/// 极简 JSON 取值：只认字符串字段。
/// 不引 serde 是为了让内核保持零依赖（后面真需要再换）。
fn json_str(text: &str, key: &str) -> Option<String> {
    let pat = format!("\"{}\"", key);
    let i = text.find(&pat)?;
    let rest = &text[i + pat.len()..];
    let colon = rest.find(':')?;
    let rest = rest[colon + 1..].trim_start();
    if !rest.starts_with('"') {
        return None;
    }
    let mut out = String::new();
    let mut chars = rest[1..].chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some(other) => out.push(other),
                None => return Some(out),
            },
            other => out.push(other),
        }
    }
    None
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// 版本号比较：1.10.0 > 1.9.2
fn cmp_version(a: &str, b: &str) -> i32 {
    let pa: Vec<i64> = a.split(['.', '-', '+']).filter_map(|x| x.parse().ok()).collect();
    let pb: Vec<i64> = b.split(['.', '-', '+']).filter_map(|x| x.parse().ok()).collect();
    for i in 0..pa.len().max(pb.len()) {
        let x = pa.get(i).copied().unwrap_or(0);
        let y = pb.get(i).copied().unwrap_or(0);
        if x != y {
            return if x > y { 1 } else { -1 };
        }
    }
    0
}

fn copy_dir(src: &Path, dest: &Path) -> io::Result<()> {
    fs::create_dir_all(dest)?;
    for ent in fs::read_dir(src)? {
        let ent = ent?;
        let from = ent.path();
        let to = dest.join(ent.file_name());
        if from.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("float-pkg-test-{}-{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn make_pkg(root: &Path, name: &str, version: &str, body: &str) -> PathBuf {
        let dir = root.join(format!("drop-{}-{}", name, version));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("manifest.json"),
            format!(
                "{{\"name\": \"{}\", \"version\": \"{}\", \"title\": \"演示\", \"entry\": \"main.cw\"}}",
                name, version
            ),
        )
        .unwrap();
        fs::write(dir.join("main.cw"), body).unwrap();
        dir
    }

    #[test]
    fn 扫不到目录时返回空() {
        let root = tmpdir("empty");
        assert!(scan_packages(&root).unwrap().is_empty());
    }

    #[test]
    fn 装包并启用() {
        let root = tmpdir("install");
        let src = make_pkg(&root, "demo", "1.0.0", "print(1)\n");
        let info = install_package(&root, &src).unwrap();
        assert_eq!(info.name, "demo");
        assert_eq!(info.version, "1.0.0");
        assert!(info.active);
        let all = scan_packages(&root).unwrap();
        assert_eq!(all.len(), 1);
        assert!(all[0].active);
        assert_eq!(all[0].entry_path().exists(), true);
    }

    #[test]
    fn 拖进新版后能一键切过去再切回来() {
        let root = tmpdir("switch");
        let v1 = make_pkg(&root, "demo", "1.0.0", "print(1)\n");
        let v2 = make_pkg(&root, "demo", "1.1.0", "print(2)\n");
        install_package(&root, &v1).unwrap();
        install_package(&root, &v2).unwrap();

        let active = active_package(&root).unwrap();
        assert_eq!(active.version, "1.1.0");
        assert_eq!(versions_of(&root, "demo").len(), 2);

        switch_to(&root, "demo", "1.0.0").unwrap();
        assert_eq!(active_package(&root).unwrap().version, "1.0.0");
        let content = fs::read_to_string(active_package(&root).unwrap().entry_path()).unwrap();
        assert_eq!(content.trim(), "print(1)");
    }

    #[test]
    fn 没清单的目录会被忽略而不是报错() {
        let root = tmpdir("junk");
        let junk = root.join("packages").join("随便一个文件夹");
        fs::create_dir_all(&junk).unwrap();
        assert!(scan_packages(&root).unwrap().is_empty());
        let e = install_package(&root, &junk).unwrap_err();
        assert!(e.contains("manifest"), "实际：{}", e);
    }

    #[test]
    fn 版本号按数字段比较() {
        assert_eq!(cmp_version("1.10.0", "1.9.0"), 1);
        assert_eq!(cmp_version("1.0", "1.0.0"), 0);
        assert_eq!(cmp_version("0.9", "1.0"), -1);
    }

    #[test]
    fn 正在用的版本不能删() {
        let root = tmpdir("protect");
        let v1 = make_pkg(&root, "demo", "1.0.0", "print(1)\n");
        install_package(&root, &v1).unwrap();
        assert!(remove_version(&root, "demo", "1.0.0").is_err());
    }
}
