//! Windows 文件关联：绿色版没有安装器，资源管理器里的 .co 图标不会自动长出来。
//! 这里用 **Win32 注册表 API** 直接写用户级 HKCU（不需要管理员、不依赖 reg.exe），
//! 由用户在「关于」页手动点，或正式版首次启动时自动执行一次；绝不改别人已有的设置。

const PROGID: &str = "Copper.SourceFile";

/// Copper 的图标（吉祥物廿七）。编译进程序里，注册关联时释放到磁盘，
/// 这样 .co 在资源管理器里显示的是这只猫，而不是 Float IDE 自己的图标。
const COPPER_ICO: &[u8] = include_bytes!("../../icons/copper.ico");

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;
    use std::ptr;

    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyW, RegDeleteTreeW, RegDeleteValueW, RegGetValueW, RegOpenKeyW,
        RegSetValueExW, HKEY, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ,
    };
    use windows_sys::Win32::UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};

    use super::PROGID;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn create_key(path: &str) -> Result<HKEY, String> {
        let p = wide(path);
        let mut h: HKEY = ptr::null_mut();
        let rc = unsafe { RegCreateKeyW(HKEY_CURRENT_USER, p.as_ptr(), &mut h) };
        if rc != ERROR_SUCCESS {
            return Err(format!("建不了注册表项 {path}（错误码 {rc}）"));
        }
        Ok(h)
    }

    /// 往 HKCU 写一个字符串值；name 传 None 就是写默认值
    fn set_sz(path: &str, name: Option<&str>, val: &str) -> Result<(), String> {
        let h = create_key(path)?;
        let n = name.map(wide);
        let v = wide(val);
        let rc = unsafe {
            RegSetValueExW(
                h,
                n.as_ref().map_or(ptr::null(), |x| x.as_ptr()),
                0,
                REG_SZ,
                v.as_ptr() as *const u8,
                (v.len() * 2) as u32,
            )
        };
        unsafe { RegCloseKey(h) };
        if rc != ERROR_SUCCESS {
            return Err(format!("写不进 {path}（错误码 {rc}）"));
        }
        Ok(())
    }

    fn read_sz(path: &str) -> Option<String> {
        let p = wide(path);
        let mut buf = [0u16; 512];
        let mut size = (buf.len() * 2) as u32;
        let rc = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                p.as_ptr(),
                ptr::null(),
                RRF_RT_REG_SZ,
                ptr::null_mut(),
                buf.as_mut_ptr() as *mut c_void,
                &mut size,
            )
        };
        if rc != ERROR_SUCCESS {
            return None;
        }
        let n = (size as usize / 2).saturating_sub(1);
        Some(String::from_utf16_lossy(&buf[..n.min(buf.len())]))
    }

    fn delete_default_value(path: &str) {
        let p = wide(path);
        let mut h: HKEY = ptr::null_mut();
        let rc = unsafe { RegOpenKeyW(HKEY_CURRENT_USER, p.as_ptr(), &mut h) };
        if rc == ERROR_SUCCESS {
            unsafe {
                RegDeleteValueW(h, ptr::null());
                RegCloseKey(h);
            }
        }
    }

    fn publish_exe_path(key: &str, val: &str) -> Result<(), String> {
        set_sz(key, None, val)
    }

    pub fn status() -> Result<bool, String> {
        Ok(read_sz(r"Software\Classes\.co").as_deref() == Some(PROGID))
    }

    pub fn default_icon() -> Option<String> {
        read_sz(&format!(r"Software\Classes\{PROGID}\DefaultIcon"))
    }

    /// 把内嵌的 .ico 释放到 %LOCALAPPDATA%\FloatIDE\，文件名带内容哈希（copper-xxxxxxxx.ico）。
    /// 不用固定名的原因：Windows 的图标缓存对「同一路径换了内容」经常不刷新，
    /// 资源管理器会一直显示旧图；文件名随内容变，缓存必然失效重取。
    fn install_icon() -> Result<std::path::PathBuf, String> {
        use super::COPPER_ICO;
        let base = std::env::var_os("LOCALAPPDATA")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let dir = base.join("FloatIDE");
        std::fs::create_dir_all(&dir).map_err(|e| format!("建不了图标目录：{e}"))?;
        // DJB2：够用、稳定、不引第三方
        let hash: u32 = {
            let mut h: u32 = 5381;
            for &b in COPPER_ICO {
                h = h.wrapping_mul(33).wrapping_add(b as u32);
            }
            h
        };
        let name = format!("copper-{hash:08x}.ico");
        let path = dir.join(&name);
        if !path.exists() {
            std::fs::write(&path, COPPER_ICO).map_err(|e| format!("写不了图标文件：{e}"))?;
        }
        // 清掉旧版本的图标文件（含改名前的固定名 copper.ico），不留垃圾
        let mut stale = vec!["copper.ico".to_string()];
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for e in entries.flatten() {
                let n = e.file_name().to_string_lossy().to_string();
                if n.starts_with("copper-") && n.ends_with(".ico") && n != name {
                    stale.push(n);
                }
            }
        }
        for n in stale {
            let _ = std::fs::remove_file(dir.join(n));
        }
        Ok(path)
    }

    pub fn register(exe: &str) -> Result<(), String> {
        let exe = exe.to_string();

        // 图标用廿七（Copper 的吉祥物），不用 exe 自己的图标
        let icon = install_icon()?;
        let icon_val = format!("\"{}\",0", icon.display());

        for ext in ["co", "cw"] {
            let key = format!(r"Software\Classes\.{ext}");
            publish_exe_path(&key, PROGID)?;
            /* 有些 Windows 版本只看扩展自己的 DefaultIcon，不跟 ProgID；
             * 两边都写一份，谁先命中都能显示猫。 */
            let _ = set_sz(&format!(r"{key}\DefaultIcon"), None, &icon_val);
        }

        let base = format!(r"Software\Classes\{PROGID}");
        set_sz(&base, None, "Copper 源文件")?;
        set_sz(&format!(r"{base}\DefaultIcon"), None, &icon_val)?;
        set_sz(
            &format!(r"{base}\shell\open\command"),
            None,
            &format!("\"{exe}\" \"%1\""),
        )?;

        // 通知外壳：图标和"打开方式"立刻刷新
        unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED as i32, SHCNF_IDLIST, ptr::null(), ptr::null()) };
        // 有些图标缓存顽固，补一发系统自己的刷新入口（失败不影响关联）
        let _ = std::process::Command::new("ie4uinit.exe").arg("-show").spawn();
        Ok(())
    }

    pub fn unregister() -> Result<(), String> {
        for ext in ["co", "cw"] {
            delete_default_value(&format!(r"Software\Classes\.{ext}"));
            delete_default_value(&format!(r"Software\Classes\.{ext}\DefaultIcon"));
        }
        let p = wide(&format!(r"Software\Classes\{PROGID}"));
        unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, p.as_ptr()) };
        unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED as i32, SHCNF_IDLIST, ptr::null(), ptr::null()) };
        Ok(())
    }

}

#[cfg(not(windows))]
mod imp {
    pub fn status() -> Result<bool, String> {
        Ok(false)
    }
    pub fn default_icon() -> Option<String> {
        None
    }
    pub fn register(_exe: &str) -> Result<(), String> {
        Err("文件关联目前只支持 Windows".to_string())
    }
    pub fn unregister() -> Result<(), String> {
        Err("文件关联目前只支持 Windows".to_string())
    }
}

/// 当前 .co 是不是已经指向我们（「关于」页显示状态用）
#[tauri::command]
pub fn assoc_status() -> Result<bool, String> {
    imp::status()
}

/// 把 .co / .cw 关联到指定程序（正式版传当前 exe；探针可以传别的路径来试）
pub fn register_with_exe(exe: &str) -> Result<(), String> {
    imp::register(exe)
}

/// 当前 DefaultIcon 写的是什么（探针/排查用）
pub fn assoc_default_icon() -> Option<String> {
    imp::default_icon()
}

/// 把 .co / .cw 关联到当前程序，并通知外壳刷新图标
#[tauri::command]
pub fn assoc_register() -> Result<(), String> {
    let exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|e| format!("拿不到当前程序路径：{e}"))?;
    imp::register(&exe)
}

/// 取消关联（只删我们写的默认值和 ProgID，不动别的值）
#[tauri::command]
pub fn assoc_unregister() -> Result<(), String> {
    imp::unregister()
}
