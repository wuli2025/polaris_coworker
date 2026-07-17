//! 手跑的环境医生探测样例: 打印 env_check / 更新检测的真实结果, 用来在真机上肉眼核对
//! 「版本号探到没」「路径解析对不对」「更新检测判得准不准」。
//!
//! `cargo run -p polaris-kernel --example env_doctor_probe`
//!
//! 只在**默认(非 desktop) feature** 下有实体: desktop 下 `env_check` 是 async 的(要 tauri
//! 运行时才能 await), 样例跑不了。但 `cargo test --workspace` 会把 desktop feature 统一进来
//! 并连带编译 examples —— 故那一侧留个空壳, 保证怎么编都不炸。

#[cfg(not(feature = "desktop"))]
fn main() {
    let r = polaris_kernel::doctor::env_check();
    println!("os={} ready={} shell_ready={}", r.os, r.ready, r.shell_ready);
    for t in [&r.claude, &r.pwsh, &r.node, &r.npm, &r.uv, &r.python] {
        println!(
            "  {:<7} found={:<5} on_path={:<5} version={:<28} path={}",
            t.key,
            t.found,
            t.on_path,
            t.version.clone().unwrap_or_else(|| "-".into()),
            t.path.clone().unwrap_or_else(|| "-".into()),
        );
    }
    let u = polaris_kernel::doctor::env_claude_update_check();
    println!(
        "update: installed={} current={:?} latest={:?} available={} checked={} msg={}",
        u.installed, u.current, u.latest, u.update_available, u.checked, u.message
    );
}

#[cfg(feature = "desktop")]
fn main() {
    eprintln!(
        "本样例需在默认 feature 下跑(desktop 下 env_check 是 async 的):\n  \
         cargo run -p polaris-kernel --example env_doctor_probe"
    );
}
