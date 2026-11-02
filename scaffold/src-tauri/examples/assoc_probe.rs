// 探针：真跑一遍「写关联 → 读状态 → 撤销」，用于验证 Win32 API 这条路通不通。
// 用法：cargo run --example assoc_probe -- <要关联的 exe 路径>
// 跑完会把关联撤销，不留在系统里。

fn main() {
    let exe = std::env::args().nth(1).unwrap_or_else(|| {
        std::env::current_exe()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default()
    });
    println!("目标程序：{exe}");

    println!("① 初始状态：{:?}", floatide_lib::commands::assoc::assoc_status());

    match floatide_lib::commands::assoc::register_with_exe(&exe) {
        Ok(()) => println!("② 写入关联：成功"),
        Err(e) => {
            println!("② 写入关联：失败 —— {e}");
            return;
        }
    }

    println!("③ 写完状态：{:?}", floatide_lib::commands::assoc::assoc_status());
    match floatide_lib::commands::assoc::assoc_default_icon() {
        Some(v) => {
            println!("   DefaultIcon = {v}");
            let path = v.trim_start_matches('"').trim_end_matches(",0").trim_end_matches('"');
            println!("   图标文件存在：{}", std::path::Path::new(path).exists());
        }
        None => println!("   DefaultIcon 没写进去"),
    }

    // 第二个参数传 keep 就保留关联（正式挂载用），否则测完撤销
    if std::env::args().nth(2).as_deref() == Some("keep") {
        println!("④ 按 keep 要求保留关联，不撤销");
        return;
    }

    match floatide_lib::commands::assoc::assoc_unregister() {
        Ok(()) => println!("④ 撤销：成功"),
        Err(e) => println!("④ 撤销：失败 —— {e}"),
    }

    println!("⑤ 撤销后状态：{:?}", floatide_lib::commands::assoc::assoc_status());
}
