//! F5（IDE 的 Rust 内核通道）跑真实工程：ask 走阻塞回调，像界面送答案那样继续。
//! 守的是「F5 也能 ask、整段只跑一次」这条线，以及方岛物语这种大脚本能否真的玩起来。

use float_lang::api::run_blocking;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

fn game_path() -> Option<std::path::PathBuf> {
    // tests 在 float-lang 下跑，工程样本在 ../../.. 上层的 language/samples
    let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for _ in 0..4 {
        p.pop();
    }
    let cand = p.join("language/samples/main.co");
    if cand.exists() {
        Some(cand)
    } else {
        None
    }
}

#[test]
fn f5_内核能带着ask把方岛物语跑起来() {
    let path = match game_path() {
        Some(p) => p,
        None => return, // 脱离工程目录跑测试时跳过，不当失败
    };
    let src = std::fs::read_to_string(&path).expect("读 main.co");
    let first = Arc::new(AtomicUsize::new(0));
    let first2 = first.clone();
    let o = run_blocking(
        "main.co",
        &src,
        path.parent(),
        vec![],
        std::collections::HashMap::new(),
        Box::new(move |prompt: &str| {
            // 第一次（标题页）选 1 进游戏，之后一律「退出」，靠游戏自己的退路收尾
            if first2.fetch_add(1, Ordering::SeqCst) == 0 {
                assert!(prompt.contains('＞'), "标题页的提问应该带提示符：{prompt}");
                "1".to_string()
            } else {
                "退出".to_string()
            }
        }),
    )
    .expect("F5 通道应该能跑通");
    assert!(o.output.contains("第 1 天"), "应该进入第 1 天，实际开头：{}", &o.output.chars().take(200).collect::<String>());
    assert!(first.load(Ordering::SeqCst) >= 2, "至少问过两次（标题页 + 游戏循环）");
}
