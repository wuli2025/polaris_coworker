# 任务 R6：P5a 审查意见修复

先读 `docs/reviews/P5a-round1-triage.md`。修复"中 2（高）"与"轻 1"，并在配置文档写明"空环境变量视同未设置"。驳回项不要改代码。

- 中 2：`src-tauri/crates/polaris-kernel/src/doctor/pi_shell.rs` 的 Windows 回落排除 `%SystemRoot%`（System32、SysWOW64、Sysnative）下的 bash 与 WindowsApps 别名；排除后无可用 shell 时明确报告不就绪；`resolve` 返回来源枚举（bundled / program_files / path / explicit），doctor 的 shell 步骤 detail 显示实际来源。测试（可在非 Windows 上用注入的候选列表/环境模拟）：System32 bash 被排除、各来源报告正确、全部排除时不就绪。
- 轻 1：`doctor/path_tests.rs` 补 `#[cfg(not(windows))]` 的 `path_contains_dir` 用例。

允许修改：`src-tauri/crates/polaris-kernel/src/doctor/`、`docs/`。无新依赖；≤500 行/≤500 字符；新 Rust 代码无 unwrap/expect；编译警告保持 0。
必须通过：两壳 `cargo check --offline`（警告 0）；`cargo test -p polaris-kernel --lib --tests --offline`（0 失败，沙箱 TCP 项除外并列出）。不 commit。
